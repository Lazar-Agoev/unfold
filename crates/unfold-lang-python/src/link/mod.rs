//! Python linking: the whole-program binder (`binder`) and its answers to the shared backend
//! ([`PythonLinked`]), including the type meaning of written type names and the parameters a
//! call binds itself (`types`), and the standard library's written types (`library`).
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
mod library;
mod members;
mod memo;
pub(crate) mod modules;
mod mutations;
mod objects;
pub(crate) mod program;
mod types;
pub(crate) mod value;

use rustc_hash::FxHashMap;
use unfold_core::validate::Census;
use unfold_core::vocab::Language;
use unfold_core::{ModuleLabel, RepoPath};
use unfold_engine::evidence::{
    ClassShape, ConcreteTarget, Evidence, ExternalLocation, MemberFact, ModuleTarget, OriginDeclaration,
    Stability, ValueContext,
};
use unfold_engine::{
    CallRef, DeclRef, FactsSet, FileId, ImportRef, LanguageId, LanguageProblem, Linked, NodeRef as GraphNode,
    ProjectInputs, TypeRef, ValueRef,
};

pub use census::PythonCensus;

use answers::module_target;
use binder::Binder;
use program::{ExprRef, NodeRef};

use crate::bind::ir::NodeKind;
use crate::payload::ImportSyntax;
use crate::stdlib::module_label;

/// The label of Python's own builtins, the only `builtin` outcome.
const BUILTINS: &str = "python:builtins";

/// An import whose module more than one source root could supply.
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
    /// Each file's import outcomes, aligned with its import facts.
    dependencies: Vec<Vec<ModuleTarget>>,
    /// Each file's kept conditional imports, by index in its import facts, sorted.
    kept: Vec<Vec<u32>>,
    classes: Vec<ClassShape>,
    census: PythonCensus,
    ambiguous: Vec<AmbiguousImport>,
    /// The declaration of every lowered `def` and `lambda`, by its node.
    functions: FxHashMap<GraphNode, NodeRef>,
    /// Every lowered `class` declaration, by its node.
    class_nodes: FxHashMap<GraphNode, NodeRef>,
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
            for (index, ((fact, written), module)) in facts
                .graph
                .imports()
                .iter()
                .zip(&syntax)
                .zip(&modules)
                .enumerate()
            {
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
                                children.push(u32::try_from(index).unwrap_or(u32::MAX));
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
            let ranges = children.iter().filter_map(|&index| {
                facts
                    .graph
                    .imports()
                    .get(index as usize)
                    .map(|import| import.range)
            });
            census.add(file.path.clone(), &census_data, ranges);
            dependencies.push(targets);
            kept.push(children);
        }
        let classes = binder.class_shapes();
        let (functions, class_nodes) = definitions(&binder);
        Self {
            binder,
            dependencies,
            kept,
            classes,
            census,
            ambiguous,
            functions,
            class_nodes,
        }
    }

    fn callee(&self, call: CallRef) -> Option<ExprRef> {
        let callee = self.binder.program.facts.call(call)?.callee;
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
    fn constructs(&self, call: CallRef) -> bool {
        self.callee(call)
            .is_some_and(|callee| self.binder.construction(callee))
    }

    fn callee_evidence(&self, call: CallRef) -> Evidence {
        self.callee(call)
            .map_or_else(Evidence::default, |callee| self.binder.evidence(callee))
    }

    fn value_evidence(&self, value: ValueRef) -> Evidence {
        self.binder.value_evidence(value)
    }

    /// The type meaning (see `types`): classes are their types; assignments and other writes
    /// are never followed to their values.
    fn type_evidence(&self, reference: TypeRef) -> Evidence {
        self.binder
            .type_expr(reference)
            .map_or_else(Evidence::default, |expr| self.binder.type_evidence(expr))
    }

    /// Proven from the callee's receiver and the target's decorators (see `types`).
    fn bound_parameters(&self, call: CallRef, target: GraphNode) -> Option<u32> {
        let function = *self.functions.get(&target)?;
        self.binder.bound_parameters(self.callee(call)?, function)
    }

    /// Proven from the passed value's receiver and the target's decorators (see `types`).
    fn reference_bound_parameters(&self, value: ValueRef, target: GraphNode) -> Option<u32> {
        let function = *self.functions.get(&target)?;
        self.binder
            .bound_reference(self.binder.value_expr(value)?, function)
    }

    /// A class takes no arguments when nothing but `object` is its base, it has no keyword
    /// (`metaclass=`) and no decorator, and its body binds no `__new__`.
    fn class_takes_no_arguments(&self, class: GraphNode) -> bool {
        self.class_nodes
            .get(&class)
            .is_some_and(|&node| self.binder.takes_no_arguments(node))
    }

    fn annotation_evidence(&self, declaration: DeclRef) -> Option<Evidence> {
        self.binder.annotation(declaration)
    }

    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        self.binder.concrete_target(declaration, context)
    }

    fn is_parameter(&self, declaration: DeclRef) -> bool {
        self.binder.is_parameter(declaration)
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

    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        self.dependencies.get(file.index()).map_or(&[], Vec::as_slice)
    }

    /// A named child import is kept when its child module is proven (see `children`).
    fn keeps_import(&self, import: ImportRef) -> bool {
        self.kept
            .get(import.file.index())
            .is_some_and(|kept| kept.binary_search(&import.import).is_ok())
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
            ExternalLocation::Library | ExternalLocation::Builtin => BUILTINS.to_owned(),
            ExternalLocation::Package(name) => module_label(name),
            ExternalLocation::File(path) => path.to_string(),
        }
    }

    fn global_label(&self, _file: FileId) -> String {
        BUILTINS.to_owned()
    }

    /// Exactly `python:builtins`.
    fn is_builtin(&self, label: &ModuleLabel) -> bool {
        label.as_str() == BUILTINS
    }

    fn census(&self) -> Option<&dyn Census> {
        Some(&self.census)
    }

    /// Imports several source roots could supply.
    fn problems(&self) -> Vec<LanguageProblem> {
        let mut problems: Vec<LanguageProblem> = self
            .ambiguous
            .iter()
            .map(|import| LanguageProblem {
                path: import.path.clone(),
                message: format!(
                    "import `{}` is ambiguous: more than one source root supplies it",
                    import.specifier
                )
                .into(),
                incomplete: false,
            })
            .collect();
        problems.sort();
        problems.dedup();
        problems
    }
}

/// The declaration of every lowered `def` and `lambda` of the program, and of every lowered
/// `class`, by its node.
fn definitions(binder: &Binder<'_>) -> (FxHashMap<GraphNode, NodeRef>, FxHashMap<GraphNode, NodeRef>) {
    let mut functions = FxHashMap::default();
    let mut classes = FxHashMap::default();
    for (index, file) in binder.program.files.iter().enumerate() {
        let Some(file) = file else {
            continue;
        };
        let id = FileId(u32::try_from(index).unwrap_or(u32::MAX));
        for (node, fact) in file.nodes.iter().enumerate() {
            let (map, local) = match &fact.kind {
                NodeKind::Function(function) => (&mut functions, function.local),
                NodeKind::Class(class) => (&mut classes, class.local),
                _ => continue,
            };
            let Some(local) = local else {
                continue;
            };
            map.entry(GraphNode { file: id, local }).or_insert(NodeRef {
                file: id,
                node: u32::try_from(node).unwrap_or(u32::MAX),
            });
        }
    }
    (functions, classes)
}
