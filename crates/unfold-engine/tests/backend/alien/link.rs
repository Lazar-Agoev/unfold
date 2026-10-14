//! Alien linking: lexical name lookup, member lookup through bases, module resolution, and the
//! evidence options tests use to reach every step of the reason order.
//!
//! `ref` options: `:refuse=wrapper|dynamic|unsupported|not_concrete`, `:ambiguous`,
//! `:overridden`, `:import=not_found|not_indexed|ambiguous`, `:excluded`, `:extfile=PATH`,
//! `:pkg=NAME`, `:nodecl`, `:mixed`, `:aug` (adds an augmentation), `:augext` (only an external
//! augmentation), `:construct` (linking proves the call a construction: its node becomes `new`
//! and it resolves in the construct context). `get` takes `:ext=NAME` for an external member.
//!
//! The standard library (`console`, `print`, `Math`) is `alien:lib`, the language's builtins, by
//! location and as the label of an undeclared root.

use rustc_hash::FxHashMap;
use smallvec::smallvec;
use unfold_core::validate::{CallSite as CensusSite, Census};
use unfold_core::vocab::Language as LanguageId;
use unfold_core::{ByteRange, ModuleLabel, RepoPath};
use unfold_engine::evidence::{
    Candidate, ClassMember, ClassShape, ConcreteTarget, Evidence, ExternalLocation, ImportBinding,
    ImportTarget, MemberFact, ModuleTarget, Origin, OriginDeclaration, Refusal, Stability, ValueContext,
};
use unfold_engine::{CallRef, DeclRef, FactsSet, FileId, ImportRef, Linked, NodeRef, TypeRef, ValueRef};

use super::payload::{Decl, DeclKind, Payload, ScopeKind, ValueInfo};

/// Names that resolve to the alien standard library when nothing declares them.
const GLOBALS: &[&str] = &["console", "print", "Math"];
/// Globals that are the language's own builtins, apart from the rest of its library.
const CORE: &[&str] = &["JSON"];

pub struct AlienLinked<'a> {
    pub program: &'a FactsSet,
    payloads: Vec<Option<Payload>>,
    unindexed: &'a [RepoPath],
    classes: Vec<ClassShape>,
    dependencies: Vec<Vec<ModuleTarget>>,
    census: AlienCensus,
}

enum Module {
    Indexed(FileId),
    Failed,
    Unindexed(String),
    Missing,
    Package(String),
}

impl<'a> AlienLinked<'a> {
    pub fn new(program: &'a FactsSet, unindexed: &'a [RepoPath]) -> Self {
        let payloads: Vec<Option<Payload>> = program
            .files()
            .iter()
            .map(|file| {
                let facts = file.facts.as_ref().filter(|_| super::is_alien(&file.path))?;
                postcard::from_bytes(&facts.payload).ok()
            })
            .collect();
        let mut linked = Self {
            program,
            payloads,
            unindexed,
            classes: Vec::new(),
            dependencies: Vec::new(),
            census: AlienCensus::default(),
        };
        linked.classes = linked.class_shapes();
        linked.dependencies = program
            .iter()
            .map(|(file, _)| linked.file_dependencies(file))
            .collect();
        let census = AlienCensus::new(program, &linked.payloads, |import| linked.keeps_import(import));
        linked.census = census;
        linked
    }

    pub fn payload(&self, file: FileId) -> Option<&Payload> {
        self.payloads.get(file.index())?.as_ref()
    }

    fn decl(&self, decl: DeclRef) -> Option<&Decl> {
        self.payload(decl.file)?.decls.get(decl.index as usize)
    }

    pub fn value_info(&self, value: ValueRef) -> Option<&ValueInfo> {
        self.payload(value.file)?.values.get(value.value as usize)
    }

    fn resolve_module(&self, file: FileId, spec: &str) -> Module {
        if !spec.starts_with("./") && !spec.starts_with("../") {
            return Module::Package(spec.to_owned());
        }
        let mut parts: Vec<&str> = self
            .program
            .file(file)
            .and_then(|entry| entry.path.parent())
            .map_or_else(Vec::new, |parent| parent.split('/').collect());
        for part in spec.split('/') {
            match part {
                "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Module::Missing;
                    }
                }
                part => parts.push(part),
            }
        }
        let base = parts.join("/");
        for extension in super::EXTENSIONS {
            let path = format!("{base}.{extension}");
            match self.program.find(&path) {
                Some(target) if self.program.facts(target).is_some() => return Module::Indexed(target),
                Some(_) => return Module::Failed,
                None if self.unindexed.iter().any(|unindexed| unindexed.as_str() == path) => {
                    return Module::Unindexed(path);
                }
                None => {}
            }
        }
        Module::Missing
    }

    /// Evidence for a name looked up from `scope` in `file`.
    fn lookup(&self, file: FileId, scope: u32, name: &str) -> Evidence {
        let Some(payload) = self.payload(file) else {
            return Evidence::default();
        };
        let mut current = Some(scope);
        while let Some(id) = current {
            let Some(found) = payload.scopes.get(id as usize) else {
                break;
            };
            let decls: Vec<u32> = found
                .decls
                .iter()
                .copied()
                .filter(|&decl| payload.decls[decl as usize].name == name)
                .collect();
            if !decls.is_empty() {
                return self.bound(file, &decls);
            }
            current = found.parent;
        }
        let mut evidence = Evidence {
            root_is_indexed: Some(false),
            ..Evidence::default()
        };
        if GLOBALS.contains(&name) {
            evidence
                .candidates
                .push(Candidate::external(ExternalLocation::Library));
        }
        if CORE.contains(&name) {
            evidence
                .candidates
                .push(Candidate::external(ExternalLocation::Builtin));
        }
        evidence
    }

    fn bound(&self, file: FileId, decls: &[u32]) -> Evidence {
        let payload = self.payload(file).expect("bound names come from a payload");
        let import = decls
            .iter()
            .find_map(|&decl| match &payload.decls[decl as usize].kind {
                DeclKind::Use { spec, export } => Some((spec, export)),
                _ => None,
            });
        let mut evidence = Evidence {
            root_is_indexed: Some(true),
            ..Evidence::default()
        };
        let Some((spec, export)) = import else {
            evidence.candidates = decls
                .iter()
                .map(|&index| Candidate::indexed(DeclRef { file, index }))
                .collect();
            return evidence;
        };
        let (target, location) = match self.resolve_module(file, spec) {
            Module::Indexed(target) => {
                evidence.candidates = self
                    .top_level(target, export)
                    .into_iter()
                    .map(Candidate::indexed)
                    .collect();
                (ImportTarget::Indexed, None)
            }
            Module::Failed | Module::Unindexed(_) => (ImportTarget::NotIndexed, None),
            Module::Missing => (ImportTarget::NotFound, None),
            Module::Package(name) => {
                let location = ExternalLocation::Package(name.into());
                evidence.candidates.push(Candidate::external(location.clone()));
                (ImportTarget::External, Some(location))
            }
        };
        evidence.import_binding = Some(ImportBinding {
            specifier: spec.as_str().into(),
            imported: export.as_str().into(),
            target,
            location,
        });
        evidence
    }

    fn top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        let Some(payload) = self.payload(file) else {
            return Vec::new();
        };
        payload.scopes[0]
            .decls
            .iter()
            .copied()
            .filter(|&decl| payload.decls[decl as usize].name == name)
            .map(|index| DeclRef { file, index })
            .collect()
    }

    /// The single class a name denotes, through imports.
    fn class_named(&self, file: FileId, scope: u32, name: &str) -> Option<DeclRef> {
        let evidence = self.lookup(file, scope, name);
        let [candidate] = evidence.candidates.as_slice() else {
            return None;
        };
        let decl = candidate.declaration?;
        matches!(self.decl(decl)?.kind, DeclKind::Class { .. }).then_some(decl)
    }

    fn class_shapes(&self) -> Vec<ClassShape> {
        let mut shapes = Vec::new();
        for (file, _) in self.program.iter() {
            let Some(payload) = self.payload(file) else {
                continue;
            };
            for (index, decl) in payload.decls.iter().enumerate() {
                let DeclKind::Class { bases, members, .. } = &decl.kind else {
                    continue;
                };
                shapes.push(ClassShape {
                    class: DeclRef {
                        file,
                        index: index as u32,
                    },
                    bases: bases
                        .iter()
                        .filter_map(|base| self.class_named(file, decl.scope, base))
                        .collect(),
                    members: members
                        .iter()
                        .map(|&member| ClassMember {
                            declaration: DeclRef { file, index: member },
                            is_static: matches!(
                                payload.decls[member as usize].kind,
                                DeclKind::Method { is_static: true, .. }
                            ),
                        })
                        .collect(),
                });
            }
        }
        shapes
    }

    fn bases(&self, class: DeclRef) -> Vec<DeclRef> {
        self.classes
            .iter()
            .find(|shape| shape.class == class)
            .map(|shape| shape.bases.clone())
            .unwrap_or_default()
    }

    /// Members named `name` of `class`, or of the first base (depth first) that has some.
    fn members(&self, class: DeclRef, name: &str) -> Vec<Candidate> {
        let mut pending = vec![class];
        let mut visited = Vec::new();
        while let Some(current) = pending.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.push(current);
            if let Some(Decl {
                kind: DeclKind::Class { members, .. },
                ..
            }) = self.decl(current)
            {
                let found: Vec<Candidate> = members
                    .iter()
                    .filter(|&&member| {
                        self.decl(DeclRef {
                            file: current.file,
                            index: member,
                        })
                        .is_some_and(|decl| decl.name == name)
                    })
                    .map(|&member| {
                        Candidate::indexed(DeclRef {
                            file: current.file,
                            index: member,
                        })
                    })
                    .collect();
                if !found.is_empty() {
                    return found;
                }
            }
            pending.extend(self.bases(current).into_iter().rev());
        }
        Vec::new()
    }

    /// The class `this` denotes from `scope`, or `None` when `this` is unbound.
    fn this_class(&self, file: FileId, scope: u32) -> Option<DeclRef> {
        let payload = self.payload(file)?;
        let mut current = Some(scope);
        while let Some(id) = current {
            let found = payload.scopes.get(id as usize)?;
            match found.kind {
                ScopeKind::Function { arrow: true, .. } => current = found.parent,
                ScopeKind::Function {
                    class: Some(class), ..
                } => return Some(DeclRef { file, index: class }),
                _ => return None,
            }
        }
        None
    }

    fn member_evidence(&self, file: FileId, receiver: u32, name: &str, external: Option<&str>) -> Evidence {
        let mut evidence = Evidence::default();
        // The access root's import binding and declaration status describe the whole chain.
        let mut root = receiver;
        while let Some(ValueInfo::Get { receiver, .. } | ValueInfo::Key { receiver, .. }) =
            self.value_info(ValueRef { file, value: root })
        {
            root = *receiver;
        }
        if let Some(ValueInfo::Ref {
            name: root_name,
            scope,
            ..
        }) = self.value_info(ValueRef { file, value: root })
        {
            let bound = self.lookup(file, *scope, root_name);
            evidence.root_is_indexed = bound.root_is_indexed;
            match bound.import_binding {
                Some(binding) => {
                    if let (ImportTarget::External, Some(location)) = (binding.target, &binding.location) {
                        evidence.candidates.push(Candidate::external(location.clone()));
                    }
                    evidence.import_binding = Some(binding);
                }
                // Members of an undeclared global come from the same library.
                None if !bound.candidates.is_empty()
                    && bound
                        .candidates
                        .iter()
                        .all(|candidate| candidate.origin == Origin::External) =>
                {
                    evidence.candidates = bound.candidates;
                }
                None => {}
            }
        }
        match self.value_info(ValueRef {
            file,
            value: receiver,
        }) {
            Some(ValueInfo::This { scope }) => match self.this_class(file, *scope) {
                Some(class) => {
                    evidence.receiver_class = Some(class);
                    evidence.candidates = self.members(class, name).into();
                }
                None => evidence.unbound_this = true,
            },
            Some(ValueInfo::Base { scope }) => {
                if let Some(base) = self
                    .this_class(file, *scope)
                    .and_then(|class| self.bases(class).first().copied())
                {
                    evidence.super_access = true;
                    evidence.receiver_class = Some(base);
                    evidence.candidates = self.members(base, name).into();
                }
            }
            Some(ValueInfo::Ref {
                name: receiver_name,
                scope,
                ..
            }) if evidence
                .import_binding
                .as_ref()
                .is_none_or(|binding| binding.target == ImportTarget::Indexed) =>
            {
                let bound = self.lookup(file, *scope, receiver_name);
                if let [candidate] = bound.candidates.as_slice()
                    && let Some(decl) = candidate.declaration
                    && let Some(found) = self.decl(decl)
                {
                    match &found.kind {
                        DeclKind::Class { .. } => {
                            evidence.static_access = true;
                            evidence.candidates = self.members(decl, name).into();
                        }
                        DeclKind::Variable { ty: Some(ty), .. } | DeclKind::Param { ty: Some(ty), .. } => {
                            if let Some(class) = self.class_named(decl.file, found.scope, ty) {
                                evidence.receiver_class = Some(class);
                                evidence.candidates = self.members(class, name).into();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if let Some(external) = external {
            evidence.candidates = smallvec![Candidate::external(ExternalLocation::Package(external.into()))];
        }
        evidence
    }

    fn apply_options(evidence: &mut Evidence, options: &[(String, Option<String>)]) {
        for (key, value) in options {
            let value = value.as_deref().unwrap_or("");
            match key.as_str() {
                "refuse" => {
                    evidence.refusal = Some(match value {
                        "wrapper" => Refusal::Wrapper,
                        "dynamic" => Refusal::Dynamic,
                        "unsupported" => Refusal::Unsupported,
                        _ => Refusal::NotConcrete,
                    });
                }
                "ambiguous" => evidence.ambiguous = true,
                "overridden" => evidence.overridden = true,
                "import" => {
                    evidence.import_binding = Some(ImportBinding {
                        specifier: "./elsewhere".into(),
                        imported: "default".into(),
                        target: match value {
                            "not_found" => ImportTarget::NotFound,
                            "not_indexed" => ImportTarget::NotIndexed,
                            _ => ImportTarget::Ambiguous,
                        },
                        location: None,
                    });
                }
                "excluded" => {
                    evidence.candidates = smallvec![Candidate {
                        origin: Origin::Excluded,
                        declaration: None,
                        augmentation: false,
                        location: None,
                    }];
                }
                "extfile" => {
                    evidence.candidates =
                        smallvec![Candidate::external(ExternalLocation::File(value.into()))];
                }
                "pkg" => {
                    evidence.candidates =
                        smallvec![Candidate::external(ExternalLocation::Package(value.into()))];
                }
                "nodecl" => {
                    evidence.candidates = smallvec![Candidate {
                        origin: Origin::Indexed,
                        declaration: None,
                        augmentation: false,
                        location: None,
                    }];
                }
                "mixed" => evidence
                    .candidates
                    .push(Candidate::external(ExternalLocation::Package("mixed".into()))),
                "aug" => evidence.candidates.push(Candidate {
                    origin: Origin::Indexed,
                    declaration: None,
                    augmentation: true,
                    location: None,
                }),
                "augext" => {
                    let mut candidate = Candidate::external(ExternalLocation::Library);
                    candidate.augmentation = true;
                    evidence.candidates = smallvec![candidate];
                }
                _ => {}
            }
        }
    }

    fn file_dependencies(&self, file: FileId) -> Vec<ModuleTarget> {
        self.program
            .imports(file)
            .iter()
            .map(|import| match import.specifier.as_deref() {
                None => ModuleTarget::Dynamic,
                Some(spec) => match self.resolve_module(file, spec) {
                    Module::Indexed(target) => ModuleTarget::Indexed(target),
                    Module::Failed => ModuleTarget::NotIndexed,
                    Module::Unindexed(path) => ModuleTarget::External(ExternalLocation::File(path.into())),
                    Module::Missing => ModuleTarget::NotFound,
                    Module::Package(name) => ModuleTarget::External(ExternalLocation::Package(name.into())),
                },
            })
            .collect()
    }

    fn node(decl: DeclRef, local: u32) -> NodeRef {
        NodeRef {
            file: decl.file,
            local,
        }
    }
}

impl Linked for AlienLinked<'_> {
    fn language(&self) -> LanguageId {
        LanguageId::TypeScript
    }

    fn constructs(&self, call: CallRef) -> bool {
        let Some(fact) = self.program.call(call) else {
            return false;
        };
        let callee = ValueRef {
            file: call.file,
            value: fact.callee,
        };
        matches!(self.value_info(callee), Some(ValueInfo::Ref { options, .. })
            if options.iter().any(|(key, _)| key == "construct"))
    }

    fn callee_evidence(&self, call: CallRef) -> Evidence {
        match self.program.call(call) {
            Some(fact) => self.value_evidence(ValueRef {
                file: call.file,
                value: fact.callee,
            }),
            None => Evidence::default(),
        }
    }

    fn value_evidence(&self, value: ValueRef) -> Evidence {
        match self.value_info(value) {
            Some(ValueInfo::Ref { name, scope, options }) => {
                let mut evidence = self.lookup(value.file, *scope, name);
                Self::apply_options(&mut evidence, options);
                evidence
            }
            Some(ValueInfo::Get {
                receiver,
                name,
                external,
            }) => self.member_evidence(value.file, *receiver, name, external.as_deref()),
            Some(ValueInfo::Key { receiver, name }) => {
                self.member_evidence(value.file, *receiver, name, None)
            }
            Some(ValueInfo::This { scope }) => Evidence {
                unbound_this: self.this_class(value.file, *scope).is_none(),
                ..Evidence::default()
            },
            _ => Evidence::default(),
        }
    }

    fn class_takes_no_arguments(&self, class: NodeRef) -> bool {
        let Some(payload) = self.payload(class.file) else {
            return false;
        };
        payload.decls.iter().any(|decl| {
            matches!(&decl.kind, DeclKind::Class { node, bases, decorated, .. }
                if *node == class.local && bases.is_empty() && !decorated)
        })
    }

    fn type_evidence(&self, reference: TypeRef) -> Evidence {
        let Some((scope, name)) = self
            .payload(reference.file)
            .and_then(|payload| payload.types.get(reference.index as usize))
        else {
            return Evidence::default();
        };
        self.lookup(reference.file, *scope, name)
    }

    fn annotation_evidence(&self, declaration: DeclRef) -> Option<Evidence> {
        let decl = self.decl(declaration)?;
        match &decl.kind {
            DeclKind::Variable { ty: Some(ty), .. } | DeclKind::Param { ty: Some(ty), .. } => {
                Some(self.lookup(declaration.file, decl.scope, ty))
            }
            _ => None,
        }
    }

    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        let concrete = |local| {
            Some(ConcreteTarget {
                target: Self::node(declaration, local),
                implementation: declaration,
            })
        };
        let decl = self.decl(declaration)?;
        match (&decl.kind, context) {
            (DeclKind::Class { node, ctor, .. }, ValueContext::Construct) => concrete(ctor.unwrap_or(*node)),
            (DeclKind::Class { node, .. }, ValueContext::Reference | ValueContext::Type) => concrete(*node),
            (_, ValueContext::Type) | (DeclKind::Class { .. }, ValueContext::Call) => None,
            (
                DeclKind::Function { node } | DeclKind::Method { node, .. } | DeclKind::Ctor { node, .. },
                _,
            ) => concrete(*node),
            (
                DeclKind::Variable {
                    mutable: false,
                    init: Some(init),
                    ..
                }
                | DeclKind::Field { init: Some(init), .. },
                _,
            ) => match self.value_info(ValueRef {
                file: declaration.file,
                value: *init,
            })? {
                ValueInfo::Inline { node } => concrete(*node),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_parameter(&self, declaration: DeclRef) -> bool {
        matches!(
            self.decl(declaration).map(|decl| &decl.kind),
            Some(DeclKind::Param { .. })
        )
    }

    fn binding_stability(&self, declaration: DeclRef) -> Stability {
        match self.decl(declaration).map(|decl| &decl.kind) {
            Some(DeclKind::Variable { mutable: true, .. }) => Stability::Rebindable,
            Some(DeclKind::Variable { mutable: false, .. }) => Stability::Stable,
            _ => Stability::Unknown,
        }
    }

    fn accessor(&self, declaration: DeclRef) -> bool {
        matches!(
            self.decl(declaration).map(|decl| &decl.kind),
            Some(DeclKind::Getter { .. })
        )
    }

    fn member(&self, declaration: DeclRef) -> MemberFact<'_> {
        let Some(decl) = self.decl(declaration) else {
            return MemberFact::default();
        };
        let owner = |owner: u32| {
            Some(DeclRef {
                file: declaration.file,
                index: owner,
            })
        };
        let name = Some(decl.name.as_str());
        match decl.kind {
            DeclKind::Method {
                owner: class,
                is_static,
                is_private,
                ..
            } => MemberFact {
                owner: owner(class),
                name,
                is_static,
                is_private,
                check_override: true,
                can_override: true,
            },
            DeclKind::Field { owner: class, .. } => MemberFact {
                owner: owner(class),
                name,
                check_override: true,
                can_override: true,
                ..MemberFact::default()
            },
            DeclKind::Getter { owner: class, .. } => MemberFact {
                owner: owner(class),
                name,
                can_override: true,
                ..MemberFact::default()
            },
            DeclKind::Ctor { owner: class, .. } => MemberFact {
                owner: owner(class),
                name,
                ..MemberFact::default()
            },
            _ => MemberFact::default(),
        }
    }

    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        match (self.decl(signature), self.decl(implementation)) {
            (Some(sig), Some(body)) => {
                matches!(sig.kind, DeclKind::Signature)
                    && matches!(body.kind, DeclKind::Function { .. })
                    && signature.file == implementation.file
                    && sig.scope == body.scope
                    && sig.name == body.name
            }
            _ => false,
        }
    }

    fn origin_declaration(&self, declaration: DeclRef) -> OriginDeclaration {
        let Some(decl) = self.decl(declaration) else {
            return OriginDeclaration::default();
        };
        let factory_callee = match decl.kind {
            DeclKind::Variable {
                mutable: false,
                init: Some(init),
                ..
            } => {
                match self.value_info(ValueRef {
                    file: declaration.file,
                    value: init,
                }) {
                    Some(ValueInfo::Call { callee: Some(callee) }) => Some(ValueRef {
                        file: declaration.file,
                        value: *callee,
                    }),
                    _ => None,
                }
            }
            _ => None,
        };
        OriginDeclaration {
            factory_callee,
            annotation: matches!(
                decl.kind,
                DeclKind::Variable { .. } | DeclKind::Param { .. } | DeclKind::Field { .. }
            ),
        }
    }

    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        self.dependencies.get(file.index()).map_or(&[], Vec::as_slice)
    }

    fn classes(&self) -> &[ClassShape] {
        &self.classes
    }

    fn is_package_specifier(&self, _file: FileId, specifier: &str) -> bool {
        !specifier.starts_with('.') && !specifier.starts_with('/')
    }

    fn label_for_specifier(&self, _file: FileId, specifier: &str) -> String {
        let mut parts = specifier.split('/');
        match (parts.next(), parts.next()) {
            (Some(scope), Some(name)) if scope.starts_with('@') => format!("{scope}/{name}"),
            (Some(first), _) => first.to_owned(),
            _ => specifier.to_owned(),
        }
    }

    fn label_for_location(&self, file: FileId, location: &ExternalLocation) -> String {
        match location {
            ExternalLocation::Library => "alien:lib".to_owned(),
            ExternalLocation::Builtin => "alien:core".to_owned(),
            ExternalLocation::Package(name) => self.label_for_specifier(file, name),
            ExternalLocation::File(path) => path.to_string(),
        }
    }

    fn global_label(&self, _file: FileId) -> String {
        "alien:lib".to_owned()
    }

    fn is_builtin(&self, label: &ModuleLabel) -> bool {
        matches!(label.as_str(), "alien:lib" | "alien:core")
    }

    /// A `:when-indexed` import stays when its specifier names an indexed file.
    fn keeps_import(&self, import: ImportRef) -> bool {
        self.program
            .import(import)
            .filter(|fact| fact.conditional)
            .and_then(|fact| fact.specifier.as_deref())
            .is_some_and(|spec| matches!(self.resolve_module(import.file, spec), Module::Indexed(_)))
    }

    fn census(&self) -> Option<&dyn Census> {
        Some(&self.census)
    }
}

/// One file's token boundaries, call sites, and module dependencies.
type CensusFile = (Vec<u32>, Vec<CensusSite>, Vec<ByteRange>);

/// The alien census, from the payload's token boundaries and the parser's own call sites and
/// dependencies (not from the lowering).
#[derive(Debug, Default)]
pub struct AlienCensus {
    files: FxHashMap<RepoPath, CensusFile>,
}

impl AlienCensus {
    /// The census of every alien file; the dependencies of conditional imports `keeps` rejects
    /// are left out, as their links are.
    fn new(program: &FactsSet, payloads: &[Option<Payload>], keeps: impl Fn(ImportRef) -> bool) -> Self {
        let mut files = FxHashMap::default();
        for ((id, file), payload) in program.iter().zip(payloads) {
            let (Some(payload), Some(facts)) = (payload, file.facts.as_deref()) else {
                continue;
            };
            let dropped: Vec<ByteRange> = facts
                .graph
                .imports()
                .iter()
                .enumerate()
                .filter(|(index, fact)| {
                    let import = ImportRef {
                        file: id,
                        import: *index as u32,
                    };
                    fact.conditional && !keeps(import)
                })
                .map(|(_, fact)| fact.range)
                .collect();
            let dependencies = payload
                .dependencies
                .iter()
                .copied()
                .filter(|range| !dropped.contains(range))
                .collect();
            let sites = payload
                .call_sites
                .iter()
                .map(|site| CensusSite {
                    range: site.range,
                    form: site.form,
                })
                .collect();
            files.insert(
                file.path.clone(),
                (payload.boundaries.clone(), sites, dependencies),
            );
        }
        Self { files }
    }
}

impl Census for AlienCensus {
    fn language(&self) -> LanguageId {
        LanguageId::TypeScript
    }

    fn is_token_range(&self, path: &RepoPath, range: ByteRange) -> bool {
        self.files.get(path).is_some_and(|(boundaries, _, _)| {
            boundaries.binary_search(&range.start).is_ok() && boundaries.binary_search(&range.end).is_ok()
        })
    }

    fn call_sites(&self, path: &RepoPath) -> Option<&[CensusSite]> {
        self.files.get(path).map(|(_, sites, _)| sites.as_slice())
    }

    fn module_dependencies(&self, path: &RepoPath) -> Option<&[ByteRange]> {
        self.files
            .get(path)
            .map(|(_, _, dependencies)| dependencies.as_slice())
    }
}
