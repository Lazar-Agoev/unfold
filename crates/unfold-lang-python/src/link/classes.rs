//! Classes: bases, C3 linearization, concreteness (a proven metaclass, no expanded or keyword
//! headers, concrete bases), member lookup through linearizations and instance fields, `super()`
//! agreement across every linearization that can reach it, and virtual dispatch.
//!
//! A base may also be a builtin exception class ([`crate::stdlib::BUILTIN_EXCEPTIONS`]): it
//! stands in the linearization as a node of no file ([`NodeRef::builtin_exception`]), with its
//! own chain up to `BaseException`, and is concrete. A linearization is kept only when every
//! builtin class follows every user class, so a member the user's classes define is found before
//! anything the builtins could define; a lookup that reaches a builtin finds nothing.

use std::hash::Hash;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use super::binder::{Binder, ClassTables, Ctx, Guard};
use super::memo::Mro;
use super::program::{ExprRef, NodeRef, Sym};
use super::value::Value;
use crate::bind::ir::{ClassFact, ExprKind, NodeKind};
use crate::stdlib::{BUILTIN_EXCEPTIONS, builtin_exception};

impl Binder<'_> {
    pub(crate) fn class_fact(&self, class: NodeRef) -> Option<&ClassFact> {
        match &self.program.node(class)?.kind {
            NodeKind::Class(fact) => Some(fact),
            _ => None,
        }
    }

    /// The binding of `member` in a class's own body.
    pub(crate) fn class_binding(&self, class: NodeRef, member: Sym) -> Option<u32> {
        let body = self.class_fact(class)?.body;
        self.program.binding_in(class.file, body, member)
    }

    /// Whether a class's members named `member` (or all of them) were invalidated.
    pub(crate) fn invalid_member(&self, class: NodeRef, member: Sym) -> bool {
        self.invalid_members
            .get(&class)
            .is_some_and(|invalid| invalid.contains(&member) || invalid.contains(&self.program.known.star))
    }

    /// A class's bases, when every positional base is a proven class (`object` is skipped) or a
    /// builtin exception class (its stand-in node). An expanded base (`*bases`) leaves the bases
    /// unknown.
    pub(crate) fn base_classes(&self, class: NodeRef, ctx: &mut Ctx) -> Option<Vec<NodeRef>> {
        let fact = self.class_fact(class)?;
        let mut bases = Vec::new();
        for argument in &fact.arguments {
            if argument.keyword.is_some() {
                continue;
            }
            if argument.expansion {
                return None;
            }
            let value = self.resolve(
                ExprRef {
                    file: class.file,
                    expr: argument.value,
                },
                ctx,
            );
            if value
                .external
                .as_ref()
                .is_some_and(|external| external.is("builtins", "object"))
            {
                continue;
            }
            let written = ExprRef {
                file: class.file,
                expr: argument.value,
            };
            if let Some(builtin) = self.builtin_exception_base(written, &value) {
                bases.push(builtin);
                continue;
            }
            match value.class {
                Some(base)
                    if !value.instance
                        && !value.ambiguous
                        && value.refusal.is_none()
                        && self.class_access_proven(&value, ctx) =>
                {
                    bases.push(base);
                }
                _ => return None,
            }
        }
        Some(bases)
    }

    /// The builtin exception class a base expression names: its proven external identity
    /// `builtins.<name>`, or a plain name that nothing binds where it is written (no binding,
    /// no unknown star import), as the builtins table names it.
    fn builtin_exception_base(&self, base: ExprRef, value: &Value) -> Option<NodeRef> {
        let name = match &value.external {
            Some(external) if &*external.module == "builtins" => {
                if value.ambiguous || value.refusal.is_some() || value.instance {
                    return None;
                }
                &*external.member
            }
            Some(_) => return None,
            None => {
                let fact = self.program.expr(base)?;
                let &ExprKind::Name(name) = &fact.kind else {
                    return None;
                };
                let sym = self.program.sym(base.file, name);
                if self.program.lookup(base.file, fact.scope, sym).is_some()
                    || self.program.unknown_star(base.file, fact.scope)
                {
                    return None;
                }
                self.program.text(sym)
            }
        };
        builtin_exception(name).map(|index| NodeRef::builtin_exception(index as u32))
    }

    /// The linearization of a builtin exception class's stand-in: its chain of bases up to
    /// `BaseException` (`object` is skipped, as for user classes).
    fn builtin_mro(builtin: NodeRef) -> Option<Arc<[NodeRef]>> {
        let mut chain = Vec::new();
        let mut current = Some(builtin.node as usize);
        while let Some(index) = current {
            let &(_, base) = BUILTIN_EXCEPTIONS.get(index)?;
            chain.push(NodeRef::builtin_exception(index as u32));
            if chain.len() > BUILTIN_EXCEPTIONS.len() {
                return None;
            }
            current = match base {
                "" => None,
                base => Some(builtin_exception(base)?),
            };
        }
        Some(Arc::from(chain))
    }

    /// Whether constructing `class` without a direct constructor is proven to take no
    /// arguments: no base but `object`, no keyword (a `metaclass=` could supply its own call),
    /// no decorator (which could replace the class), and no `__new__` in its body.
    pub(crate) fn takes_no_arguments(&self, class: NodeRef) -> bool {
        let Some(fact) = self.class_fact(class) else {
            return false;
        };
        if !fact.decorators.is_empty() || self.class_binding(class, self.program.known.new).is_some() {
            return false;
        }
        let mut ctx = Ctx::new();
        fact.arguments.iter().all(|argument| {
            argument.keyword.is_none()
                && !argument.expansion
                && self
                    .identity(
                        ExprRef {
                            file: class.file,
                            expr: argument.value,
                        },
                        &mut ctx,
                    )
                    .as_deref()
                    == Some("builtins.object")
        })
    }

    /// The positional, unexpanded bases that are proven classes, whatever the others are.
    fn known_bases(&self, class: NodeRef, ctx: &mut Ctx) -> Vec<NodeRef> {
        let Some(fact) = self.class_fact(class) else {
            return Vec::new();
        };
        fact.arguments
            .iter()
            .filter(|argument| argument.keyword.is_none() && !argument.expansion)
            .filter_map(|argument| {
                let value = self.resolve(
                    ExprRef {
                        file: class.file,
                        expr: argument.value,
                    },
                    ctx,
                );
                value
                    .class
                    .filter(|_| !value.instance && !value.ambiguous && value.refusal.is_none())
            })
            .collect()
    }

    /// Every class's descendants through known bases.
    pub(crate) fn class_tables(&self) -> ClassTables {
        let classes = &self.program.classes;
        let index = classes
            .iter()
            .enumerate()
            .map(|(position, &class)| (class, position as u32))
            .collect::<FxHashMap<_, _>>();
        let bases: Vec<Vec<u32>> = classes
            .iter()
            .map(|&class| {
                self.known_bases(class, &mut Ctx::new())
                    .into_iter()
                    .filter_map(|base| index.get(&base).copied())
                    .collect()
            })
            .collect();
        let mut descendants: Vec<Vec<u32>> = vec![Vec::new(); classes.len()];
        // `visited[class] == start` marks the classes reached from `start`, so no reset is
        // needed between starts.
        let mut visited = vec![u32::MAX; classes.len()];
        let mut pending = Vec::new();
        for start in 0..classes.len() as u32 {
            pending.clear();
            pending.push(start);
            while let Some(class) = pending.pop() {
                if std::mem::replace(&mut visited[class as usize], start) == start {
                    continue;
                }
                descendants[class as usize].push(start);
                pending.extend(bases[class as usize].iter().copied());
            }
        }
        ClassTables { index, descendants }
    }

    /// The classes that descend from `owner` (itself included), in program order.
    pub(crate) fn descendants(&self, owner: NodeRef) -> impl Iterator<Item = NodeRef> + '_ {
        self.class_index(owner)
            .and_then(|index| self.classes.descendants.get(index as usize))
            .into_iter()
            .flatten()
            .filter_map(|&index| self.program.classes.get(index as usize).copied())
    }

    /// The C3 linearization of a class, when its bases are known and consistent. Computed
    /// iteratively (bases first), so a deep hierarchy can't exhaust the stack; a class whose
    /// bases lead back to it has none.
    pub(crate) fn mro(&self, class: NodeRef, ctx: &mut Ctx) -> Mro {
        if let Some(found) = self.memo_mro(class) {
            return found;
        }
        if ctx.guarded(Guard::Mro(class)) {
            return None;
        }
        let (results, independent) = ctx.with(Guard::Mro(class), |ctx| self.linearize(class, ctx));
        let result = results.get(&class).cloned().flatten();
        if independent {
            for (found, mro) in results {
                self.keep_mro(found, mro);
            }
        }
        result
    }

    /// The linearizations of `root` and of every ancestor not yet known.
    fn linearize(&self, root: NodeRef, ctx: &mut Ctx) -> FxHashMap<NodeRef, Mro> {
        let mut results: FxHashMap<NodeRef, Mro> = FxHashMap::default();
        let mut path: FxHashSet<NodeRef> = FxHashSet::default();
        let mut frames: Vec<(NodeRef, Option<Vec<NodeRef>>)> = vec![(root, None)];
        let known = |results: &FxHashMap<NodeRef, Mro>, class: NodeRef| {
            if class.is_builtin() {
                return Some(Self::builtin_mro(class));
            }
            results.get(&class).cloned().or_else(|| self.memo_mro(class))
        };
        while let Some((class, bases)) = frames.last_mut() {
            let class = *class;
            if bases.is_none() {
                let found = if class == root {
                    self.base_classes(class, ctx)
                } else {
                    ctx.with(Guard::Mro(class), |ctx| self.base_classes(class, ctx)).0
                };
                match found {
                    Some(found) => {
                        *bases = Some(found);
                        path.insert(class);
                    }
                    None => {
                        results.insert(class, None);
                        frames.pop();
                        continue;
                    }
                }
            }
            let bases = bases.clone().unwrap_or_default();
            let mut next = None;
            let mut cyclic = false;
            for &base in &bases {
                if known(&results, base).is_some() {
                    continue;
                }
                if path.contains(&base) || ctx.guarded(Guard::Mro(base)) {
                    cyclic = true;
                } else {
                    next = Some(base);
                }
                break;
            }
            if let Some(base) = next.filter(|_| !cyclic) {
                frames.push((base, None));
                continue;
            }
            let lists: Option<Vec<Arc<[NodeRef]>>> = if cyclic {
                None
            } else {
                bases
                    .iter()
                    .map(|&base| known(&results, base).flatten())
                    .collect()
            };
            let mro = lists
                .and_then(|lists| merge(class, lists, bases))
                .filter(|mro| builtins_last(mro))
                .map(Arc::from);
            results.insert(class, mro);
            path.remove(&class);
            frames.pop();
        }
        results
    }

    /// Whether a class has a proven, concrete shape: not wrapped by a decorator, no expanded or
    /// keyword header other than a `metaclass` proven to be `type`, and concrete bases.
    /// Computed iteratively, bases first; a class whose bases lead back to it is not concrete.
    pub(crate) fn class_concrete(&self, class: NodeRef, ctx: &mut Ctx) -> bool {
        if let Some(found) = self.memo_concrete(class) {
            return found;
        }
        if ctx.guarded(Guard::Concrete(class)) {
            return false;
        }
        let (results, independent) = ctx.with(Guard::Concrete(class), |ctx| self.concreteness(class, ctx));
        let result = results.get(&class).copied().unwrap_or(false);
        if independent {
            for (found, concrete) in results {
                self.keep_concrete(found, concrete);
            }
        }
        result
    }

    fn concreteness(&self, root: NodeRef, ctx: &mut Ctx) -> FxHashMap<NodeRef, bool> {
        let mut results: FxHashMap<NodeRef, bool> = FxHashMap::default();
        let mut path: FxHashSet<NodeRef> = FxHashSet::default();
        let mut frames: Vec<(NodeRef, Option<Vec<NodeRef>>)> = vec![(root, None)];
        let known = |results: &FxHashMap<NodeRef, bool>, class: NodeRef| {
            if class.is_builtin() {
                return Some(true);
            }
            results.get(&class).copied().or_else(|| self.memo_concrete(class))
        };
        while let Some((class, bases)) = frames.last_mut() {
            let class = *class;
            if bases.is_none() {
                let found = if class == root {
                    self.own_shape(class, ctx)
                } else {
                    ctx.with(Guard::Concrete(class), |ctx| self.own_shape(class, ctx))
                        .0
                };
                match found {
                    Some(found) => {
                        *bases = Some(found);
                        path.insert(class);
                    }
                    None => {
                        results.insert(class, false);
                        frames.pop();
                        continue;
                    }
                }
            }
            let bases = bases.clone().unwrap_or_default();
            let mut verdict = Some(true);
            for &base in &bases {
                match known(&results, base) {
                    Some(true) => continue,
                    Some(false) => verdict = Some(false),
                    None if path.contains(&base) || ctx.guarded(Guard::Concrete(base)) => {
                        verdict = Some(false);
                    }
                    None => {
                        frames.push((base, None));
                        verdict = None;
                    }
                }
                break;
            }
            if let Some(concrete) = verdict {
                results.insert(class, concrete);
                path.remove(&class);
                frames.pop();
            }
        }
        results
    }

    /// A class's own part of concreteness: not wrapped, a plain header, a proven `type`
    /// metaclass if any, and known bases (returned).
    fn own_shape(&self, class: NodeRef, ctx: &mut Ctx) -> Option<Vec<NodeRef>> {
        if self.wrapper(class, ctx) {
            return None;
        }
        let fact = self.class_fact(class)?;
        let metaclass =
            |keyword: u32| self.program.text(self.program.sym(class.file, keyword)) == "metaclass";
        if fact
            .arguments
            .iter()
            .any(|argument| argument.expansion || argument.keyword.is_some_and(|keyword| !metaclass(keyword)))
        {
            return None;
        }
        let meta = fact
            .arguments
            .iter()
            .find(|argument| argument.keyword.is_some_and(metaclass));
        if let Some(meta) = meta {
            let identity = self.identity(
                ExprRef {
                    file: class.file,
                    expr: meta.value,
                },
                ctx,
            );
            if identity.as_deref() != Some("builtins.type") {
                return None;
            }
        }
        self.base_classes(class, ctx)
    }

    /// Whether a class attribute access is proven: a written class attribute is looked up by
    /// its metaclass, so the class must be concrete; a `cls` parameter can name any descendant,
    /// so they must all be. Instance and `super` lookups have their own evidence.
    pub(crate) fn class_access_proven(&self, value: &Value, ctx: &mut Ctx) -> bool {
        let Some(owner) = value.receiver_class else {
            return true;
        };
        if value.super_access || !value.static_access && !value.class_parameter_access {
            return true;
        }
        if ctx.guarded(Guard::Access(owner)) {
            return false;
        }
        ctx.with(Guard::Access(owner), |ctx| {
            self.class_concrete(owner, ctx)
                && (!value.class_parameter_access
                    || self
                        .descendants(owner)
                        .all(|candidate| self.class_concrete(candidate, ctx)))
        })
        .0
    }
}

/// Whether every builtin exception stand-in of a linearization follows every user class, so a
/// lookup meets the user's definitions first.
fn builtins_last(mro: &[NodeRef]) -> bool {
    let first = mro.iter().position(|node| node.is_builtin()).unwrap_or(mro.len());
    mro[first..].iter().all(|node| node.is_builtin())
}

/// The C3 merge of the bases' linearizations and the bases themselves (of user classes, or of
/// the standard library's stub classes by name). A candidate is the first head that appears in
/// no list's tail; counting tail occurrences keeps the merge linear.
pub(crate) fn merge<T: Copy + Eq + Hash>(class: T, mros: Vec<Arc<[T]>>, bases: Vec<T>) -> Option<Vec<T>> {
    let mut lists: Vec<Vec<T>> = mros.iter().map(|mro| mro.to_vec()).collect();
    lists.push(bases);
    let mut tails: FxHashMap<T, u32> = FxHashMap::default();
    for list in &lists {
        for &item in list.iter().skip(1) {
            *tails.entry(item).or_default() += 1;
        }
    }
    let mut heads = vec![0usize; lists.len()];
    let mut result = vec![class];
    loop {
        let mut open = false;
        let mut chosen = None;
        for (list, &head) in lists.iter().zip(&heads) {
            let Some(&candidate) = list.get(head) else {
                continue;
            };
            open = true;
            if tails.get(&candidate).copied().unwrap_or(0) == 0 {
                chosen = Some(candidate);
                break;
            }
        }
        if !open {
            return Some(result);
        }
        let candidate = chosen?;
        result.push(candidate);
        for (list, head) in lists.iter().zip(heads.iter_mut()) {
            if list.get(*head) == Some(&candidate) {
                *head += 1;
                if let Some(next) = list.get(*head)
                    && let Some(count) = tails.get_mut(next)
                {
                    *count = count.saturating_sub(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use unfold_engine::FileId;

    use super::*;

    fn class(node: u32) -> NodeRef {
        NodeRef {
            file: FileId(0),
            node,
        }
    }

    #[test]
    fn c3_merges_diamonds_and_rejects_inconsistent_orders() {
        let (object, a, b, c, d) = (class(0), class(1), class(2), class(3), class(4));
        let mro_a: Arc<[NodeRef]> = Arc::from([a, object]);
        let mro_b: Arc<[NodeRef]> = Arc::from([b, a, object]);
        let mro_c: Arc<[NodeRef]> = Arc::from([c, a, object]);
        assert_eq!(
            merge(d, vec![mro_b.clone(), mro_c], vec![b, c]),
            Some(vec![d, b, c, a, object])
        );
        // `class D(A, B)` where B already precedes A: no consistent order.
        assert_eq!(merge(d, vec![mro_a, mro_b], vec![a, b]), None);
        assert_eq!(merge(d, Vec::new(), Vec::new()), Some(vec![d]));
    }
}
