//! Python linking: the whole-program binder (`binder`) and its answers to the shared backend
//! ([`PythonLinked`]).
//!
//! Linking reads only cached per-file facts: the binder facts extraction wrote into each
//! payload ([`crate::bind`]), the written imports, and the census. It never parses, never runs
//! an interpreter, and never looks at an installed package; the only things it reads besides the
//! facts are the settings, the project inputs, and (for paths discovery never walks) whether a
//! path exists on disk. Everything is computed once per link into hash-map indexes, then
//! answered from any thread.

pub(crate) mod answers;
pub(crate) mod binder;
mod census;
pub(crate) mod children;
mod classes;
mod decorators;
mod members;
mod memo;
pub(crate) mod modules;
mod mutations;
mod objects;
pub(crate) mod program;
pub(crate) mod value;

use std::sync::OnceLock;

use unfold_core::RepoPath;
use unfold_core::validate::Census;
use unfold_core::vocab::{EdgeKind, Language};
use unfold_engine::entrances::EntranceDetector;
use unfold_engine::evidence::{
    ClassShape, ConcreteTarget, Evidence, Export, ExternalLocation, LocatorMembers, MemberFact, ModuleTarget,
    OriginDeclaration, ParameterFact, Stability, ValueContext,
};
use unfold_engine::facts::CallContext;
use unfold_engine::{
    BlockRef, CallRef, DeclRef, FactsSet, FileId, Invocation, LanguageId, LanguageProblem, Linked,
    ProjectInputs, ValueRef,
};

pub use census::PythonCensus;

use answers::module_target;
use binder::Binder;
use program::{ExprRef, NodeRef};

use crate::detect::scripts::Scripts;
use crate::detect::{Sites, Source, SourceCandidates};
use crate::payload::ImportSyntax;
use crate::stdlib::module_label;

/// An import whose module more than one source root (or file and package) could supply.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AmbiguousImport {
    /// The importing file.
    pub path: RepoPath,
    /// The specifier as written.
    pub specifier: Box<str>,
}

/// The linked Python program.
pub struct PythonLinked<'a> {
    binder: Binder<'a>,
    inputs: &'a ProjectInputs,
    /// The call sites the source detectors read, indexed on first use.
    sites: OnceLock<Sites>,
    /// The console-script detector's findings, computed on first use.
    scripts: OnceLock<Scripts>,
    /// The source detectors' candidates, computed together on first use.
    source_candidates: OnceLock<SourceCandidates>,
    /// Each file's import outcomes, aligned with its import facts.
    dependencies: Vec<Vec<ModuleTarget>>,
    /// Each file's kept conditional import blocks, sorted.
    kept: Vec<Vec<u32>>,
    classes: Vec<ClassShape>,
    exports: Vec<OnceLock<Vec<Export>>>,
    census: PythonCensus,
    ambiguous: Vec<AmbiguousImport>,
}

impl<'a> PythonLinked<'a> {
    /// Links the Python files of `files`.
    pub fn new(files: &'a FactsSet, inputs: &'a ProjectInputs) -> Self {
        let mut binder = Binder::new(files, inputs);
        let mut dependencies = Vec::with_capacity(files.files().len());
        let mut kept = Vec::with_capacity(files.files().len());
        let mut census = PythonCensus::default();
        let mut ambiguous = Vec::new();
        let written: Vec<_> = binder
            .program
            .files
            .iter_mut()
            .map(|bind| {
                bind.as_mut().map(|bind| {
                    (
                        std::mem::take(&mut bind.imports),
                        std::mem::take(&mut bind.census),
                    )
                })
            })
            .collect();
        for ((id, file), written) in files.iter().zip(written) {
            let (Some(facts), Some((syntax, census_data))) = (file.facts.as_deref(), written) else {
                dependencies.push(Vec::new());
                kept.push(Vec::new());
                continue;
            };
            let path = file.path.as_str();
            let mut targets = Vec::with_capacity(syntax.len());
            let mut children = Vec::new();
            let mut modules = Vec::with_capacity(syntax.len());
            for (fact, written) in facts.graph.imports().iter().zip(&syntax) {
                let specifier = match written {
                    ImportSyntax::Module { module, .. } => Some(module.to_string()),
                    ImportSyntax::From { level, module, .. } => {
                        let mut text = ".".repeat(*level as usize);
                        text.push_str(module.as_deref().unwrap_or(""));
                        Some(text)
                    }
                    ImportSyntax::Dynamic => fact.specifier.as_deref().map(str::to_owned),
                    ImportSyntax::Child { .. } => None,
                };
                let module = specifier.map(|specifier| binder.modules.resolve(id, path, &specifier));
                modules.push(module);
            }
            for ((fact, written), module) in facts.graph.imports().iter().zip(&syntax).zip(&modules) {
                let target = match (written, module) {
                    (ImportSyntax::Child { from, name, node }, _) => {
                        let parent = modules.get(*from as usize).cloned().flatten();
                        let relative = matches!(syntax.get(*from as usize), Some(ImportSyntax::From { level, .. }) if *level > 0);
                        let child =
                            parent
                                .zip(binder.program.names.get(name))
                                .and_then(|(parent, member)| {
                                    binder.imported_child(
                                        NodeRef {
                                            file: id,
                                            node: *node,
                                        },
                                        &parent,
                                        member,
                                        relative,
                                    )
                                });
                        match child {
                            Some(child) => {
                                children.push(fact.block);
                                module_target(&binder, &child)
                            }
                            None => ModuleTarget::Unsupported,
                        }
                    }
                    (_, Some(module)) => module_target(&binder, module),
                    (_, None) => ModuleTarget::Dynamic,
                };
                if target == ModuleTarget::Ambiguous {
                    ambiguous.push(AmbiguousImport {
                        path: file.path.clone(),
                        specifier: fact.specifier.clone().unwrap_or_default(),
                    });
                }
                targets.push(target);
            }
            // Imports the payload doesn't describe (a corrupt payload) stay unsupported.
            targets.resize(facts.graph.imports().len(), ModuleTarget::Unsupported);
            let ranges = children
                .iter()
                .filter_map(|&block| facts.graph.blocks().get(block as usize).map(|block| block.range));
            census.add(file.path.clone(), &census_data, ranges);
            dependencies.push(targets);
            kept.push(children);
        }
        let classes = binder.class_shapes();
        let exports = (0..files.files().len()).map(|_| OnceLock::new()).collect();
        Self {
            binder,
            inputs,
            sites: OnceLock::new(),
            scripts: OnceLock::new(),
            source_candidates: OnceLock::new(),
            dependencies,
            kept,
            classes,
            exports,
            census,
            ambiguous,
        }
    }

    /// What the source detectors read.
    pub(crate) fn source(&self) -> Source<'_, 'a> {
        Source {
            binder: &self.binder,
            sites: self.sites.get_or_init(|| Sites::new(&self.binder)),
        }
    }

    /// The console-script detector's findings.
    pub(crate) fn scripts(&self) -> &Scripts {
        self.scripts
            .get_or_init(|| crate::detect::scripts::scripts(&self.binder, self.inputs))
    }

    /// The source detectors' candidates.
    pub(crate) fn source_candidates(&self) -> &SourceCandidates {
        self.source_candidates
            .get_or_init(|| crate::detect::source_candidates(&self.source(), &self.inputs.settings))
    }

    fn callee(&self, call: CallRef) -> Option<ExprRef> {
        let callee = self.binder.program.facts.call(call)?.callee?;
        self.binder.value_expr(ValueRef {
            file: call.file,
            value: callee,
        })
    }
}

impl Linked for PythonLinked<'_> {
    fn language(&self) -> LanguageId {
        Language::Python
    }

    /// A call whose callee is a proven, concrete class constructs it.
    fn invocation(&self, call: CallRef) -> Option<Invocation> {
        let callee = self.callee(call)?;
        self.binder.construction(callee).then_some(Invocation {
            edge: EdgeKind::Constructs,
            context: CallContext::Construct,
        })
    }

    fn callee_evidence(&self, call: CallRef) -> Evidence {
        self.callee(call)
            .map_or_else(Evidence::default, |callee| self.binder.evidence(callee))
    }

    fn value_evidence(&self, value: ValueRef) -> Evidence {
        self.binder.value_evidence(value)
    }

    fn annotation_evidence(&self, declaration: DeclRef) -> Option<Evidence> {
        self.binder.annotation(declaration)
    }

    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        self.binder.concrete_target(declaration, context)
    }

    fn parameter(&self, declaration: DeclRef) -> Option<ParameterFact> {
        self.binder.parameter(declaration)
    }

    fn binding_stability(&self, declaration: DeclRef) -> Stability {
        self.binder.stability(declaration)
    }

    fn accessor(&self, declaration: DeclRef) -> bool {
        self.binder.is_accessor(declaration)
    }

    fn member(&self, declaration: DeclRef) -> MemberFact<'_> {
        self.binder.member_fact(declaration)
    }

    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.binder.overload_of(signature, implementation)
    }

    fn origin_declaration(&self, declaration: DeclRef) -> OriginDeclaration {
        self.binder.origin(declaration)
    }

    fn locator_top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        self.binder.top_level(file, name)
    }

    fn locator_concrete(&self, declaration: DeclRef) -> Option<BlockRef> {
        self.binder.locator_block(declaration)
    }

    fn locator_members(&self, owner: DeclRef, name: &str) -> LocatorMembers {
        self.binder.members_named(owner, name)
    }

    fn locator_overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.binder.overload_of(signature, implementation)
    }

    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        self.dependencies.get(file.index()).map_or(&[], Vec::as_slice)
    }

    fn keeps_import(&self, block: BlockRef) -> bool {
        self.kept
            .get(block.file.index())
            .is_some_and(|kept| kept.binary_search(&block.local).is_ok())
    }

    fn module_exports(&self, file: FileId) -> &[Export] {
        match self.exports.get(file.index()) {
            Some(cell) => cell.get_or_init(|| self.binder.exports(file)),
            None => &[],
        }
    }

    fn classes(&self) -> &[ClassShape] {
        &self.classes
    }

    /// Python module specifiers are packages unless relative.
    fn is_package_specifier(&self, _file: FileId, specifier: &str) -> bool {
        !specifier.starts_with('.')
    }

    fn label_for_specifier(&self, _file: FileId, specifier: &str) -> String {
        module_label(specifier)
    }

    fn label_for_location(&self, _file: FileId, location: &ExternalLocation) -> String {
        match location {
            ExternalLocation::Library => "python:builtins".to_owned(),
            ExternalLocation::Package(name) => module_label(name),
            ExternalLocation::File(path) => path.to_string(),
        }
    }

    fn global_label(&self, _file: FileId) -> String {
        "python:builtins".to_owned()
    }

    /// The `pyproject.toml` files the console-script detector reads (when it is enabled), even
    /// those that declare no scripts.
    fn inputs_read(&self) -> Vec<RepoPath> {
        self.scripts().inputs.clone()
    }

    fn detectors(&self) -> Vec<Box<dyn EntranceDetector + '_>> {
        crate::detect::detectors(self)
    }

    fn census(&self) -> Option<&dyn Census> {
        Some(&self.census)
    }

    /// Imports several source roots could supply, and `pyproject.toml` files the console-script
    /// detector can't read.
    fn problems(&self) -> Vec<LanguageProblem> {
        let mut problems: Vec<LanguageProblem> = self
            .ambiguous
            .iter()
            .map(|import| LanguageProblem {
                path: import.path.clone(),
                message: format!(
                    "import `{}` is ambiguous: more than one source root (or a module and a package) supplies it",
                    import.specifier
                )
                .into(),
                incomplete: false,
            })
            .collect();
        problems.extend(self.scripts().problems.iter().cloned());
        problems.sort();
        problems.dedup();
        problems
    }
}
