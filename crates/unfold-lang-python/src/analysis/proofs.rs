//! The structural proofs: which identities a resolved per-file census proves, and the function
//! forms, erased imports, and dynamic imports that follow from them.

use ruff_python_ast::{Expr, ExprCall, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::Form;

use super::scope::{Census, Lookup, Origin, Write};

/// The decisions about structure that depend on proven identities.
#[derive(Debug, Default)]
pub(crate) struct Proofs {
    /// The form of every function defined directly in a class body, by the `def`'s start (a
    /// decorated definition starts at its first `@`). Functions not listed are `function`s.
    pub(super) kinds: FxHashMap<TextSize, Form>,
    /// The starts of `import` and `from … import` statements a proven `TYPE_CHECKING` guard
    /// erases.
    pub(super) erased: FxHashSet<TextSize>,
    /// Calls proven to be `importlib.import_module` with a literal module name, by range, with
    /// the decoded name and the literal's range.
    pub(super) dynamic: FxHashMap<TextRange, (Box<str>, TextRange)>,
    /// The specifier of every runtime module dependency, sorted: each `import` item's module,
    /// each `from … import` statement's module path, and each proven dynamic import's literal.
    /// Imports a proven `TYPE_CHECKING` guard erases are not dependencies.
    pub(super) dependencies: Vec<TextRange>,
}

impl Proofs {
    /// The form of a `def`.
    pub(crate) fn function_form(&self, function: &StmtFunctionDef) -> Form {
        self.kinds
            .get(&function.range.start())
            .copied()
            .unwrap_or(Form::Function)
    }

    /// Whether an import statement starting at `start` is erased.
    pub(crate) fn is_erased(&self, start: TextSize) -> bool {
        self.erased.contains(&start)
    }

    /// The literal module name of a call proven to be a dynamic import, with the literal's
    /// range.
    pub(crate) fn dynamic_import(&self, call: &ExprCall) -> Option<(&str, TextRange)> {
        self.dynamic
            .get(&call.range())
            .map(|(name, literal)| (&**name, *literal))
    }

    /// The ranges of every runtime module dependency, sorted, for the census.
    pub(crate) fn dependencies(&self) -> &[TextRange] {
        &self.dependencies
    }
}

/// The form of a `def` written directly in a class body.
pub(super) fn member_form(function: &StmtFunctionDef, scope: u32, identities: &Identities<'_, '_>) -> Form {
    let decorators = &function.decorator_list;
    let setter = decorators.iter().any(|decorator| {
        let Expr::Attribute(access) = &decorator.expression else {
            return false;
        };
        let Expr::Name(property) = &*access.value else {
            return false;
        };
        if access.attr.id.as_str() != "setter" {
            return false;
        }
        let Lookup::Bound(binding) = identities.census.lookup(scope, &property.id) else {
            return false;
        };
        binding.writes.iter().any(|write| match write {
            Write::Def(other) => other.decorator_list.iter().any(|item| {
                !std::ptr::eq(item, decorator)
                    && identities.is(callee(&item.expression), scope, "builtins.property")
            }),
            _ => false,
        })
    });
    if setter {
        return Form::Setter;
    }
    let getter = decorators
        .iter()
        .any(|decorator| identities.is(callee(&decorator.expression), scope, "builtins.property"));
    if getter {
        Form::Getter
    } else if function.name.id.as_str() == "__init__" {
        Form::Constructor
    } else {
        Form::Method
    }
}

/// A decorator's callee: `@d(...)` is decorated by `d`'s result, identified by `d`.
fn callee(expr: &Expr) -> &Expr {
    match expr {
        Expr::Call(call) => &call.func,
        _ => expr,
    }
}

/// Proven identities over a resolved census.
pub(super) struct Identities<'c, 'a> {
    census: &'c Census<'a>,
    /// A proven builtin `exec` or `eval` can rebind anything in the module, so nothing is proven.
    poisoned: bool,
    /// Imported identities this file assigns or deletes (`typing.TYPE_CHECKING = True`).
    mutated: FxHashSet<String>,
}

impl<'c, 'a> Identities<'c, 'a> {
    pub(super) fn new(
        census: &'c Census<'a>,
        evaluations: &[(&Expr, u32)],
        mutations: &[(u32, &str, &str)],
    ) -> Self {
        let mut identities = Self {
            census,
            poisoned: false,
            mutated: FxHashSet::default(),
        };
        identities.poisoned = evaluations.iter().any(|&(callee, scope)| {
            matches!(
                identities.raw(callee, scope).as_deref(),
                Some("builtins.exec" | "builtins.eval")
            )
        });
        for &(scope, base, attribute) in mutations {
            if let Some(module) = identities.module(scope, base) {
                identities.mutated.insert(format!("{module}.{attribute}"));
            }
        }
        identities
    }

    /// Whether `expr`, evaluated in `scope`, is proven to be `identity`.
    pub(super) fn is(&self, expr: &Expr, scope: u32, identity: &str) -> bool {
        !self.poisoned
            && !self.mutated.contains(identity)
            && self.raw(expr, scope).is_some_and(|found| found == identity)
    }

    /// The identity of a name or a module attribute, before invalidation.
    fn raw(&self, expr: &Expr, scope: u32) -> Option<String> {
        match expr {
            Expr::Name(name) => match self.census.lookup(scope, &name.id) {
                Lookup::Unbound => Some(format!("builtins.{}", name.id)),
                Lookup::Bound(binding) => match binding.writes.as_slice() {
                    [Write::Import(Origin::Module(module))] => Some((*module).to_owned()),
                    [Write::Import(Origin::Member(module, member))] => Some(format!("{module}.{member}")),
                    _ => None,
                },
            },
            Expr::Attribute(access) => match &*access.value {
                Expr::Name(base) => self
                    .module(scope, &base.id)
                    .map(|module| format!("{module}.{}", access.attr.id)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The module a name is proven to be bound to by a single `import`.
    fn module(&self, scope: u32, name: &str) -> Option<&'a str> {
        match self.census.lookup(scope, name) {
            Lookup::Bound(binding) => match binding.writes.as_slice() {
                [Write::Import(Origin::Module(module))] => Some(module),
                _ => None,
            },
            Lookup::Unbound => None,
        }
    }
}
