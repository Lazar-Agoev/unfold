//! Named child imports (`from pkg import child`) and the second mutation pass that guards them
//! (the proof of concept's `collectChildImportMutations` and `importedChild`).
//!
//! Python first asks the package for an existing attribute; only an attribute proven absent
//! makes the import machinery load the child module. So a child import is an `imports` link of its
//! own only when the package is an indexed package or namespace whose `__init__.py` binds
//! nothing of that name and defines no hook that could (`__getattr__`, `__path__`, `__name__`,
//! `__class__`, `__dict__`), holds no unknown star import or `exec`, and no code anywhere may
//! write the attribute: directly, through `setattr`, through the module's `__dict__`, through
//! `globals()`/`locals()`/`vars()`, through `__path__` and `__spec__` (which redirect loading),
//! or through mutating methods on any of these. A relative child import additionally needs
//! the importer's package metadata (`__package__`, `__spec__`, `__name__`, `__path__`)
//! unchanged.
//!
//! The pass follows values to the module objects they may be ([`Binder::objects`]) and records
//! negative facts separately from the first pass, so they only ever withhold child imports.

use rustc_hash::FxHashSet;

use super::binder::{Binder, Ctx, Seen, SeenKey};
use super::modules::Module;
use super::mutations::module_keys;
use super::program::{ExprRef, NodeRef, Sym};
use super::value::dotted;
use crate::bind::ir::{Effect, ExprKind, MODULE_SCOPE, NodeKind};

/// Methods that mutate a dict or list in place.
const MUTATORS: &[&str] = &[
    "append",
    "extend",
    "insert",
    "remove",
    "pop",
    "popitem",
    "clear",
    "sort",
    "reverse",
    "update",
    "setdefault",
    "__setitem__",
    "__delitem__",
    "__ior__",
    "__iadd__",
    "__imul__",
];

/// Module attributes whose writes redirect loading.
const METADATA: &[&str] = &["__package__", "__spec__", "__name__", "__path__"];

/// Package attributes that could supply any name.
const HOOKS: &[&str] = &["__getattr__", "__path__", "__name__", "__class__", "__dict__"];

/// A module object a value may be, or one of its special attributes.
#[derive(Debug, Clone)]
pub(crate) struct ModuleObject {
    pub(crate) module: Module,
    pub(crate) attribute: Option<&'static str>,
}

/// An expression or a declaration, as the pass follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Raw {
    Expr(ExprRef),
    Node(NodeRef),
}

impl Raw {
    pub(crate) fn key(self) -> SeenKey {
        match self {
            Self::Expr(expr) => SeenKey::Expr(expr),
            Self::Node(node) => SeenKey::Node(node),
        }
    }
}

pub(crate) fn special(name: &str) -> Option<&'static str> {
    ["__dict__", "__path__", "__spec__"]
        .into_iter()
        .find(|&found| found == name)
}

impl Binder<'_> {
    pub(crate) fn collect_child_mutations(&mut self) {
        let files = self.program.files.len();
        for index in 0..files {
            let file = unfold_engine::FileId(index as u32);
            let effects = match self.program.file(file) {
                Some(bind) => bind.effects.clone(),
                None => continue,
            };
            for effect in effects {
                let mut invalidations = Vec::new();
                match effect {
                    Effect::Target {
                        target, augmented, ..
                    } => {
                        let target = ExprRef { file, expr: target };
                        self.target_invalidations(target, &mut invalidations);
                        if augmented {
                            for origin in self.objects(Raw::Expr(target), &mut Seen::default()) {
                                invalidations.push((origin, self.program.known.star));
                            }
                        }
                    }
                    Effect::Call(call) => {
                        self.call_invalidations(ExprRef { file, expr: call }, &mut invalidations);
                    }
                }
                for (origin, member) in invalidations {
                    self.invalidate_child(&origin, member);
                }
            }
        }
    }

    fn invalidate_child(&mut self, origin: &ModuleObject, member: Sym) {
        let key = match origin.attribute {
            Some(attribute) if attribute != "__dict__" => self.program.names.intern(attribute),
            _ => member,
        };
        for path in module_keys(&origin.module) {
            self.invalid_children.entry(path).or_default().insert(key);
        }
    }

    /// A written attribute or item: every module object its receiver may be loses that name.
    fn target_invalidations(&self, target: ExprRef, out: &mut Vec<(ModuleObject, Sym)>) {
        let mut pending = vec![target];
        while let Some(expr) = pending.pop() {
            let file = expr.file;
            match self.program.expr(expr).map(|fact| &fact.kind) {
                Some(&ExprKind::Member { base, name }) => {
                    let member = self.program.sym(file, name);
                    for origin in self.objects(Raw::Expr(ExprRef { file, expr: base }), &mut Seen::default())
                    {
                        out.push((origin, member));
                    }
                }
                Some(&ExprKind::Index { base, key }) => {
                    let member = key.map_or(self.program.known.star, |key| self.program.sym(file, key));
                    for origin in self.objects(Raw::Expr(ExprRef { file, expr: base }), &mut Seen::default())
                    {
                        out.push((origin, member));
                    }
                }
                Some(ExprKind::Sequence(items)) => {
                    pending.extend(items.iter().map(|&item| ExprRef { file, expr: item }));
                }
                _ => {}
            }
        }
    }

    fn call_invalidations(&self, call: ExprRef, out: &mut Vec<(ModuleObject, Sym)>) {
        let Some(ExprKind::Call {
            callee, arguments, ..
        }) = self.program.expr(call).map(|fact| &fact.kind)
        else {
            return;
        };
        let file = call.file;
        let callee = ExprRef { file, expr: *callee };
        let star = self.program.known.star;
        self.mutation_call(Raw::Expr(callee), &mut Seen::default(), out);
        let possible = self.identities(Raw::Expr(callee), &mut Seen::default());
        if possible.contains("builtins.setattr") || possible.contains("builtins.delattr") {
            let member = self.literal_argument(file, arguments, 1).unwrap_or(star);
            if arguments.iter().any(|argument| argument.expansion) {
                for argument in arguments {
                    for origin in self.objects(
                        Raw::Expr(ExprRef {
                            file,
                            expr: argument.value,
                        }),
                        &mut Seen::default(),
                    ) {
                        out.push((origin, star));
                    }
                }
            } else if let Some(first) = arguments.first() {
                for origin in self.objects(
                    Raw::Expr(ExprRef {
                        file,
                        expr: first.value,
                    }),
                    &mut Seen::default(),
                ) {
                    out.push((origin, member));
                }
            }
        }
        let mutates = possible.iter().any(|identity| {
            identity
                .strip_prefix("builtins.dict.")
                .or_else(|| identity.strip_prefix("builtins.list."))
                .is_some_and(|method| MUTATORS.contains(&method))
        });
        if mutates && let Some(first) = arguments.first() {
            for origin in self.objects(
                Raw::Expr(ExprRef {
                    file,
                    expr: first.value,
                }),
                &mut Seen::default(),
            ) {
                out.push((origin, star));
            }
        }
    }

    /// A call that may mutate a module object in place (`pkg.__path__.insert`, an alias of a
    /// mutating method, ...).
    fn mutation_call(&self, raw: Raw, seen: &mut Seen, out: &mut Vec<(ModuleObject, Sym)>) {
        if seen.has(raw.key()) || seen.full() {
            return;
        }
        seen.add(raw.key());
        let star = self.program.known.star;
        match raw {
            Raw::Expr(expr) => {
                let file = expr.file;
                let Some(fact) = self.program.expr(expr) else {
                    return;
                };
                match fact.kind {
                    ExprKind::Member { base, name } => {
                        let member = self.program.sym(file, name);
                        let base = Raw::Expr(ExprRef { file, expr: base });
                        for declaration in self.resolve_raw(raw).declarations {
                            self.mutation_call(Raw::Node(declaration), seen, out);
                        }
                        for origin in self.objects(base, &mut Seen::default()) {
                            if origin.attribute.is_none() {
                                for declaration in self.module_declarations(&origin.module, member) {
                                    self.mutation_call(Raw::Node(declaration), seen, out);
                                }
                            }
                        }
                        if MUTATORS.contains(&self.program.text(member)) {
                            for origin in self.objects(base, &mut Seen::default()) {
                                out.push((origin, star));
                            }
                        }
                    }
                    ExprKind::Chain(value) => {
                        self.mutation_call(Raw::Expr(ExprRef { file, expr: value }), seen, out);
                    }
                    ExprKind::Name(_) => self.write_mutations(raw, seen, out),
                    _ => {}
                }
            }
            Raw::Node(node) => match self.program.node(node).map(|fact| &fact.kind) {
                Some(NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    self.mutation_call(
                        Raw::Expr(ExprRef {
                            file: node.file,
                            expr: *value,
                        }),
                        seen,
                        out,
                    );
                }
                Some(NodeKind::Import) => self.write_mutations(raw, seen, out),
                _ => {}
            },
        }
    }

    fn write_mutations(&self, raw: Raw, seen: &mut Seen, out: &mut Vec<(ModuleObject, Sym)>) {
        for write in self.raw_writes(raw) {
            let write = self.program.write(write);
            if let Some(value) = write.value {
                self.mutation_call(Raw::Expr(value), seen, out);
            }
            if let Some(imported) = &write.import
                && let Some(member) = imported.member
            {
                for declaration in self.module_declarations(&imported.module, member) {
                    self.mutation_call(Raw::Node(declaration), seen, out);
                }
            }
        }
    }

    /// Every identity a callee may have, through aliases, assignments, and imports.
    fn identities(&self, raw: Raw, seen: &mut Seen) -> FxHashSet<String> {
        let mut result = FxHashSet::default();
        if seen.has(raw.key()) || seen.full() {
            return result;
        }
        seen.add(raw.key());
        if let Raw::Expr(expr) = raw
            && let Some(direct) = self.identity(expr, &mut Ctx::new())
        {
            result.insert(direct);
        }
        match raw {
            Raw::Expr(expr) => {
                let file = expr.file;
                match self.program.expr(expr).map(|fact| &fact.kind) {
                    Some(&ExprKind::Member { base, name }) => {
                        let member = self.program.text(self.program.sym(file, name));
                        for found in self.identities(Raw::Expr(ExprRef { file, expr: base }), seen) {
                            let base = found.strip_suffix(".*").unwrap_or(&found);
                            result.insert(dotted(base, member));
                        }
                        for declaration in self.resolve_raw(raw).declarations {
                            result.extend(self.identities(Raw::Node(declaration), seen));
                        }
                    }
                    Some(&ExprKind::Chain(value)) => {
                        result.extend(self.identities(Raw::Expr(ExprRef { file, expr: value }), seen));
                    }
                    _ => {}
                }
            }
            Raw::Node(node) => {
                if let Some(NodeKind::Assignment {
                    value: Some(value), ..
                }) = self.program.node(node).map(|fact| &fact.kind)
                {
                    result.extend(self.identities(
                        Raw::Expr(ExprRef {
                            file: node.file,
                            expr: *value,
                        }),
                        seen,
                    ));
                }
            }
        }
        for write in self.raw_writes(raw) {
            let write = self.program.write(write);
            if let Some(value) = write.value {
                result.extend(self.identities(Raw::Expr(value), seen));
            }
            let Some(imported) = &write.import else {
                continue;
            };
            if imported.module.external.is_some() {
                let member = imported.member.map_or("*", |member| self.program.text(member));
                result.insert(dotted(&imported.module.specifier, member));
            } else if let Some(member) = imported.member {
                for declaration in self.module_declarations(&imported.module, member) {
                    result.extend(self.identities(Raw::Node(declaration), seen));
                }
            }
        }
        result
    }

    /// The module a named child import loads, when the name is proven absent from its package
    /// (see the module docs). `node` is the name's declaration; `from` is the statement's module.
    pub(crate) fn imported_child(
        &self,
        node: NodeRef,
        module: &Module,
        member: Sym,
        relative: bool,
    ) -> Option<Module> {
        if module.external.is_some() || module.missing || module.ambiguous || module.directories.is_empty() {
            return None;
        }
        let program = &self.program;
        let scope = program.node(node)?.scope;
        if program.unsupported(node.file) || program.own_unknown_star(node.file, scope) {
            return None;
        }
        let named = |name: &str| program.names.get(name);
        let has =
            |file, name: &str| named(name).is_some_and(|sym| program.module_binding(file, sym).is_some());
        let invalid = |key: &str, names: &[Sym]| {
            [self.invalid_modules.get(key), self.invalid_children.get(key)]
                .into_iter()
                .flatten()
                .any(|invalid| names.iter().any(|name| invalid.contains(name)))
        };
        if relative {
            let importer = program.facts.file(node.file)?.path.as_str();
            let mut names: Vec<Sym> = METADATA.iter().filter_map(|name| named(name)).collect();
            names.push(program.known.star);
            if METADATA.iter().any(|name| has(node.file, name)) || invalid(importer, &names) {
                return None;
            }
        }
        for path in &module.paths {
            let file = self.modules.indexed(path)?;
            if !path.ends_with("/__init__.py") {
                return None;
            }
            let binding = program.module_binding(file, member);
            let own_import = binding.is_some_and(|binding| {
                file == node.file
                    && program
                        .binding(binding)
                        .writes
                        .iter()
                        .all(|&write| program.write(write).node == node)
            });
            if binding.is_some() && !own_import
                || HOOKS.iter().any(|name| has(file, name))
                || program.own_unknown_star(file, MODULE_SCOPE)
                || program.unsupported(file)
            {
                return None;
            }
        }
        let mut names = vec![member, program.known.star];
        names.extend(HOOKS.iter().filter_map(|name| named(name)));
        if module_keys(module).iter().any(|key| invalid(key, &names)) {
            return None;
        }
        self.modules.member(module, program.text(member))
    }
}
