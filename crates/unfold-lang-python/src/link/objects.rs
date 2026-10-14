//! The module objects a value may be (the proof of concept's `objects`): followed through
//! aliases, assignments, imports, tuple and dict displays, subscripts, and module attributes,
//! with the special attributes (`__dict__`, `__path__`, `__spec__`) that redirect a module's
//! namespace or loading, and `globals()`, `locals()`, and `vars()`.

use super::binder::{Binder, Ctx, Seen};
use super::children::{ModuleObject, Raw, special};
use super::modules::{Module, ModuleValue, dirname};
use super::program::{ExprRef, NodeRef, Sym};
use crate::bind::ir::{ExprKind, NodeKind, ScopeKind};

impl Binder<'_> {
    /// The module that holds a scope, as an object (a package's folder included).
    pub(crate) fn owning_module(&self, file: unfold_engine::FileId) -> Option<Module> {
        let path = self.program.facts.file(file)?.path.as_str();
        let directories = if path.ends_with("/__init__.py") {
            vec![dirname(path).into()]
        } else {
            Vec::new()
        };
        Some(std::sync::Arc::new(ModuleValue {
            specifier: "".into(),
            paths: vec![path.into()],
            directories,
            external: None,
            missing: false,
            ambiguous: false,
        }))
    }

    /// The writes a name or import declaration denotes.
    pub(crate) fn raw_writes(&self, raw: Raw) -> Vec<u32> {
        let binding = match raw {
            Raw::Node(node) => self.program.binding_of(node),
            Raw::Expr(expr) => match self.program.expr(expr) {
                Some(fact) => match fact.kind {
                    ExprKind::Name(name) => {
                        self.program
                            .lookup(expr.file, fact.scope, self.program.sym(expr.file, name))
                    }
                    _ => None,
                },
                None => None,
            },
        };
        binding.map_or_else(Vec::new, |binding| self.program.binding(binding).writes.clone())
    }

    /// The declarations of `member` at the top level of each file of a module.
    pub(crate) fn module_declarations(&self, module: &ModuleValue, member: Sym) -> Vec<NodeRef> {
        module
            .paths
            .iter()
            .filter_map(|path| self.modules.indexed(path))
            .filter_map(|file| self.program.module_binding(file, member))
            .flat_map(|binding| {
                self.program
                    .binding(binding)
                    .writes
                    .iter()
                    .map(|&write| self.program.write(write).node)
            })
            .collect()
    }

    pub(crate) fn resolve_raw(&self, raw: Raw) -> super::value::Value {
        match raw {
            Raw::Expr(expr) => self.resolve(expr, &mut Ctx::new()),
            Raw::Node(node) => self.resolve_node(node),
        }
    }

    /// The module objects (and special attributes of them) a value may be.
    pub(crate) fn objects(&self, raw: Raw, seen: &mut Seen) -> Vec<ModuleObject> {
        if seen.has(raw.key()) || seen.full() {
            return Vec::new();
        }
        seen.add(raw.key());
        let mut result = Vec::new();
        let value = self.resolve_raw(raw);
        if let Some(module) = value.module {
            result.push(ModuleObject {
                module,
                attribute: None,
            });
        }
        for &declaration in &value.declarations {
            if Raw::Node(declaration) != raw {
                result.extend(self.objects(Raw::Node(declaration), seen));
            }
        }
        match raw {
            Raw::Expr(expr) => self.expr_objects(expr, seen, &mut result),
            Raw::Node(node) => match self.program.node(node).map(|fact| &fact.kind) {
                Some(NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    result.extend(self.objects(
                        Raw::Expr(ExprRef {
                            file: node.file,
                            expr: *value,
                        }),
                        seen,
                    ));
                }
                Some(NodeKind::Import) => self.write_objects(raw, seen, &mut result),
                _ => {}
            },
        }
        result
    }

    pub(crate) fn expr_objects(&self, expr: ExprRef, seen: &mut Seen, result: &mut Vec<ModuleObject>) {
        let Some(fact) = self.program.expr(expr) else {
            return;
        };
        let file = expr.file;
        let local = |id: u32| ExprRef { file, expr: id };
        match &fact.kind {
            &ExprKind::Name(name) => {
                let sym = self.program.sym(file, name);
                let text = self.program.text(sym);
                if matches!(text, "__path__" | "__spec__")
                    && self.program.lookup(file, fact.scope, sym).is_none()
                    && let Some(module) = self.owning_module(file)
                    && (text == "__spec__" || !module.directories.is_empty())
                {
                    result.push(ModuleObject {
                        module,
                        attribute: special(text),
                    });
                }
                self.write_objects(Raw::Expr(expr), seen, result);
            }
            &ExprKind::Member { base, name } => {
                let member = self.program.sym(file, name);
                for origin in self.objects(Raw::Expr(local(base)), seen) {
                    if origin.attribute.is_some() {
                        result.push(origin);
                    } else if let Some(attribute) = special(self.program.text(member)) {
                        result.push(ModuleObject {
                            module: origin.module,
                            attribute: Some(attribute),
                        });
                    } else {
                        for declaration in self.module_declarations(&origin.module, member) {
                            result.extend(self.objects(Raw::Node(declaration), seen));
                        }
                    }
                }
            }
            &ExprKind::Index { base, key } => {
                let key = key.map(|key| self.program.text(self.program.sym(file, key)));
                for origin in self.objects(Raw::Expr(local(base)), seen) {
                    let redirected =
                        origin.attribute == Some("__dict__") && matches!(key, Some("__path__" | "__spec__"));
                    result.push(if redirected {
                        ModuleObject {
                            module: origin.module,
                            attribute: key.and_then(special),
                        }
                    } else {
                        origin
                    });
                }
            }
            &ExprKind::Chain(value) => result.extend(self.objects(Raw::Expr(local(value)), seen)),
            ExprKind::Sequence { items, .. } | ExprKind::Dict(items) => {
                for &item in items {
                    result.extend(self.objects(Raw::Expr(local(item)), seen));
                }
            }
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let callee = local(*callee);
                let mut identity = self.identity(callee, &mut Ctx::new());
                if identity.is_none()
                    && let Some(callee_fact) = self.program.expr(callee)
                    && let ExprKind::Name(name) = callee_fact.kind
                {
                    let sym = self.program.sym(file, name);
                    let text = self.program.text(sym);
                    if matches!(text, "globals" | "locals" | "vars")
                        && self.program.lookup(file, callee_fact.scope, sym).is_none()
                        && !self.program.unknown_star(file, callee_fact.scope)
                        && !self.program.unsupported(file)
                    {
                        identity = Some(format!("builtins.{text}"));
                    }
                }
                let module_scope = self
                    .program
                    .scope(file, fact.scope)
                    .is_some_and(|scope| scope.kind == ScopeKind::Module);
                match identity.as_deref() {
                    Some("builtins.globals") => self.push_dictionary(file, result),
                    Some("builtins.locals") if module_scope => self.push_dictionary(file, result),
                    Some("builtins.vars") => {
                        if let Some(first) = arguments.first() {
                            for origin in self.objects(Raw::Expr(local(first.value)), seen) {
                                result.push(ModuleObject {
                                    module: origin.module,
                                    attribute: Some("__dict__"),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub(crate) fn push_dictionary(&self, file: unfold_engine::FileId, result: &mut Vec<ModuleObject>) {
        if let Some(module) = self.owning_module(file) {
            result.push(ModuleObject {
                module,
                attribute: Some("__dict__"),
            });
        }
    }

    /// The objects a name or import's writes may bind.
    pub(crate) fn write_objects(&self, raw: Raw, seen: &mut Seen, result: &mut Vec<ModuleObject>) {
        for write in self.raw_writes(raw) {
            let write = self.program.write(write);
            if let Some(value) = write.value {
                result.extend(self.objects(Raw::Expr(value), seen));
            }
            let Some(imported) = &write.import else {
                continue;
            };
            match imported.member {
                None => result.push(ModuleObject {
                    module: imported.module.clone(),
                    attribute: None,
                }),
                Some(member) => match special(self.program.text(member)) {
                    Some(attribute) => result.push(ModuleObject {
                        module: imported.module.clone(),
                        attribute: Some(attribute),
                    }),
                    None => {
                        for declaration in self.module_declarations(&imported.module, member) {
                            result.extend(self.objects(Raw::Node(declaration), seen));
                        }
                    }
                },
            }
        }
    }
}
