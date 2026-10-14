//! The first mutation pass (the proof of concept's `collectMutations`): attribute writes and
//! reflection that change what classes and modules provide.
//!
//! In program order, for every attribute target and every call:
//!
//! - `module.attr = ...` (from any file) invalidates `attr` of every module file and namespace
//!   folder the receiver may be, and makes those modules' bindings of `attr` mutated;
//! - `self.attr = ...` in a method, where `self` is the method's first parameter and an
//!   instance of its class, records an instance field (with its annotation, its value, and
//!   whether the write is in `__init__`);
//! - any other write to a class's attribute invalidates that member (and mutates its binding);
//! - a proven `exec` or `eval` makes every identity in its module `unsupported`;
//! - a proven `setattr`/`delattr` invalidates the literal member it names (or every member).
//!
//! Each step resolves values with the state the earlier steps left, as the proof of concept did.

use rustc_hash::FxHashMap;

use super::binder::{Binder, Ctx, Field};
use super::modules::ModuleValue;
use super::program::{ExprRef, NodeRef, Sym};
use crate::bind::ir::{Argument, Effect, ExprKind, MODULE_SCOPE, NodeKind, TargetKind};

/// The keys a module's invalid attributes are recorded under.
pub(crate) fn module_keys(module: &ModuleValue) -> Vec<Box<str>> {
    if module.external.is_some() {
        return vec![format!("external:{}", module.specifier).into()];
    }
    module
        .paths
        .iter()
        .cloned()
        .chain(
            module
                .directories
                .iter()
                .map(|directory| format!("namespace:{directory}").into()),
        )
        .collect()
}

impl Binder<'_> {
    pub(crate) fn collect_mutations(&mut self) {
        self.initial_fields();
        let files = self.program.files.len();
        for index in 0..files {
            let file = unfold_engine::FileId(index as u32);
            let effects = match self.program.file(file) {
                Some(bind) => bind.effects.clone(),
                None => continue,
            };
            for effect in effects {
                match effect {
                    Effect::Target {
                        target,
                        kind,
                        value,
                        annotation,
                        function,
                        ..
                    } => {
                        let context = TargetWrite {
                            kind,
                            value: value.map(|expr| ExprRef { file, expr }),
                            annotation: annotation.map(|expr| ExprRef { file, expr }),
                            function: function.map(|node| NodeRef { file, node }),
                        };
                        for member in self.member_targets(ExprRef { file, expr: target }) {
                            self.member_write(member, &context);
                        }
                    }
                    Effect::Call(call) => self.reflective_call(ExprRef { file, expr: call }),
                }
            }
        }
    }

    /// Fields declared by annotations in class bodies.
    fn initial_fields(&mut self) {
        let mut bodies: FxHashMap<(unfold_engine::FileId, u32), NodeRef> = FxHashMap::default();
        for &class in &self.program.classes {
            if let Some(fact) = self.class_fact(class) {
                bodies.insert((class.file, fact.body), class);
            }
        }
        for binding in &self.program.bindings {
            let Some(&class) = bodies.get(&(binding.file, binding.scope)) else {
                continue;
            };
            let writes: Vec<_> = binding
                .writes
                .iter()
                .map(|&write| self.program.write(write))
                .collect();
            let annotations: Vec<ExprRef> = writes.iter().filter_map(|write| write.annotation).collect();
            if annotations.is_empty() {
                continue;
            }
            let field = Field {
                annotations,
                writes: writes
                    .iter()
                    .filter_map(|write| write.value.map(|value| (Some(value), false)))
                    .collect(),
            };
            self.fields.entry(class).or_default().insert(binding.name, field);
        }
    }

    /// The attribute expressions a target writes, through tuples and lists.
    pub(crate) fn member_targets(&self, target: ExprRef) -> Vec<ExprRef> {
        let mut found = Vec::new();
        let mut pending = vec![target];
        while let Some(expr) = pending.pop() {
            match self.program.expr(expr).map(|fact| &fact.kind) {
                Some(ExprKind::Member { .. }) => found.push(expr),
                Some(ExprKind::Sequence { items, .. }) => {
                    pending.extend(items.iter().rev().map(|&item| ExprRef {
                        file: expr.file,
                        expr: item,
                    }));
                }
                _ => {}
            }
        }
        found
    }

    fn member_write(&mut self, target: ExprRef, context: &TargetWrite) {
        let Some(&ExprKind::Member { base, name }) = self.program.expr(target).map(|fact| &fact.kind) else {
            return;
        };
        let file = target.file;
        let key = self.program.sym(file, name);
        let base = ExprRef { file, expr: base };
        let mut ctx = Ctx::new();
        let receiver = self.resolve(base, &mut ctx);
        if let Some(module) = &receiver.module {
            let module = module.clone();
            self.invalidate_module(&module, key);
            return;
        }
        let Some(class) = receiver.class else {
            return;
        };
        let method = context
            .function
            .and_then(|function| match &self.program.node(function)?.kind {
                NodeKind::Function(fact) => Some(fact),
                _ => None,
            });
        let base_name = match self.program.expr(base).map(|fact| &fact.kind) {
            Some(&ExprKind::Name(name)) => Some(self.program.sym(file, name)),
            _ => None,
        };
        let own = receiver.instance
            && base_name.is_some()
            && method
                .and_then(|method| method.first_parameter)
                .map(|first| self.program.sym(file, first))
                == base_name;
        if own {
            let constructor = method
                .and_then(|method| method.name)
                .is_some_and(|name| self.program.sym(file, name) == self.program.known.init);
            let field = self.fields.entry(class).or_default().entry(key).or_default();
            match context.kind {
                TargetKind::Annotation => field.annotations.extend(context.annotation),
                kind => {
                    let value = context.value.filter(|_| kind == TargetKind::Assignment);
                    field.writes.push((value, constructor));
                    if kind == TargetKind::Assignment {
                        field.annotations.extend(context.annotation);
                    }
                }
            }
        } else {
            // A new epoch only when the state answers depend on changes.
            let mut changed = self.invalid_members.entry(class).or_default().insert(key);
            if let Some(binding) = self.class_binding(class, key) {
                changed |= !std::mem::replace(&mut self.program.bindings[binding as usize].mutated, true);
            }
            if changed {
                self.changed();
            }
        }
    }

    /// Invalidates `key` (or every attribute, for `*`) of the modules a value may be.
    pub(crate) fn invalidate_module(&mut self, module: &ModuleValue, key: Sym) {
        let star = self.program.known.star;
        let mut changed = false;
        for path in module_keys(module) {
            let file = self.modules.indexed(&path);
            changed |= self.invalid_modules.entry(path).or_default().insert(key);
            let Some(file) = file else {
                continue;
            };
            let bindings: Vec<u32> = self.program.bindings_in(file, MODULE_SCOPE).to_vec();
            for binding in bindings {
                let found = &mut self.program.bindings[binding as usize];
                if (key == star || found.name == key) && !found.mutated {
                    found.mutated = true;
                    changed = true;
                }
            }
        }
        // A new epoch only when the state answers depend on changes.
        if changed {
            self.changed();
        }
    }

    /// `exec`, `eval`, `setattr`, and `delattr`, by proven identity.
    fn reflective_call(&mut self, call: ExprRef) {
        let Some(ExprKind::Call {
            callee, arguments, ..
        }) = self.program.expr(call).map(|fact| fact.kind.clone())
        else {
            return;
        };
        let file = call.file;
        let mut ctx = Ctx::new();
        let identity = self.identity(ExprRef { file, expr: callee }, &mut ctx);
        match identity.as_deref() {
            Some("builtins.exec" | "builtins.eval") => {
                if let Some(Some(bind)) = self.program.files.get_mut(file.index())
                    && !std::mem::replace(&mut bind.unsupported, true)
                {
                    self.changed();
                }
            }
            Some("builtins.setattr" | "builtins.delattr") => {
                let receiver = arguments
                    .first()
                    .map(|argument| {
                        self.resolve(
                            ExprRef {
                                file,
                                expr: argument.value,
                            },
                            &mut ctx,
                        )
                    })
                    .unwrap_or_default();
                let key = self
                    .literal_argument(file, &arguments, 1)
                    .unwrap_or(self.program.known.star);
                if let Some(module) = &receiver.module {
                    let module = module.clone();
                    self.invalidate_module(&module, key);
                }
                if let Some(class) = receiver.class
                    && self.invalid_members.entry(class).or_default().insert(key)
                {
                    self.changed();
                }
            }
            _ => {}
        }
    }

    /// The string literal written as the `position`th argument, as a symbol.
    pub(crate) fn literal_argument(
        &self,
        file: unfold_engine::FileId,
        arguments: &[Argument],
        position: usize,
    ) -> Option<Sym> {
        let argument = arguments.get(position)?;
        match self
            .program
            .expr(ExprRef {
                file,
                expr: argument.value,
            })?
            .kind
        {
            ExprKind::Str(text) => Some(self.program.sym(file, text)),
            _ => None,
        }
    }
}

/// What writes a target, for [`Binder::member_write`].
struct TargetWrite {
    kind: TargetKind,
    value: Option<ExprRef>,
    annotation: Option<ExprRef>,
    function: Option<NodeRef>,
}
