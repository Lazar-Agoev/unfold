//! The binder pass over annotations: every type name a recorded annotation writes, with the
//! expression the type meaning resolves at link time ([`crate::link`]).
//!
//! The recorded annotations are parameter and return annotations and the annotation of every
//! annotated assignment (`x: T`, `x: T = v`, class members included). Each is evaluated where
//! Python evaluates it: a `def`'s annotations in its type-parameter scope (or the enclosing
//! scope), an annotated assignment's in its own scope (a class member's in the class body).
//!
//! | Written | Type names |
//! | --- | --- |
//! | `Cart` | `Cart` |
//! | `models.Cart`, `typing.Optional` | the whole chain, one qualified name (its root must be a name) |
//! | `dict[str, Cart]`, `Optional[list[Cart]]` | the subscript's value and every element of its slice, nested |
//! | `Cart \| None` | both sides of `\|` |
//! | `Callable[[Cart], Receipt]`, `tuple[*Ts]` | the items of lists and tuples, and starred values |
//! | `"Cart"`, `"models.Cart"` | a string whose whole content is a dotted name: the name inside the quotes |
//! | `Literal["a", B]` (or `x.Literal[...]`) | `Literal` only: its arguments are values |
//! | `Annotated[Cart, meta]` (or `x.Annotated[...]`) | `Annotated` and its first argument: the rest are values |
//! | `None`, literals, calls, any other string, any other expression | nothing |
//!
//! A name bound to a PEP 695 type parameter of an enclosing `def`, `class`, or `type` statement
//! (`def first[T](items: list[T]) -> T`) is not recorded; that is decided lexically once the
//! file's scopes are complete ([`Builder::type_parameter`]).

use std::borrow::Cow;

use ruff_python_ast::{Expr, ExprStringLiteral, Operator, StringFlags};
use ruff_text_size::{Ranged, TextRange};
use unicode_normalization::UnicodeNormalization;

use super::build::Builder;
use super::ir::{ExprId, ExprKind, ScopeId, ScopeKind};

/// One type name before the type-parameter filter.
pub(super) struct RawTypeName<'a> {
    /// The written name's range: the name, the whole chain, or a string's content.
    pub(super) range: TextRange,
    /// Whether it is a dotted name (`a.B`).
    pub(super) qualified: bool,
    /// The name or member chain to resolve.
    pub(super) expr: ExprId,
    /// The scope the annotation is evaluated in.
    pub(super) scope: ScopeId,
    /// The chain's root name (NFKC), which decides whether it names a type parameter.
    pub(super) root: Cow<'a, str>,
}

/// Python's keywords, which no dotted name in a string annotation can contain.
const KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
    "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield",
];

impl<'a> Builder<'a> {
    /// Records the type names of the annotation `annotation`, evaluated in `scope`. Iterative,
    /// so a deeply nested annotation cannot exhaust the stack.
    pub(super) fn annotation(&mut self, annotation: &'a Expr, scope: ScopeId) {
        let mut pending = vec![annotation];
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Name(name) => self.type_name(expr, Cow::Borrowed(name.id.as_str()), scope),
                Expr::Attribute(_) => {
                    if let Some(root) = chain_root(expr) {
                        self.type_name(expr, Cow::Borrowed(root), scope);
                    }
                }
                Expr::Subscript(subscript) => {
                    let elements = match &*subscript.slice {
                        Expr::Tuple(tuple) => tuple.elts.as_slice(),
                        slice => std::slice::from_ref(slice),
                    };
                    let types = match spelled(&subscript.value) {
                        Some("Literal") => 0,
                        Some("Annotated") => 1,
                        _ => elements.len(),
                    };
                    pending.extend(elements.iter().take(types).rev());
                    pending.push(&subscript.value);
                }
                Expr::BinOp(union) if union.op == Operator::BitOr => {
                    pending.push(&union.right);
                    pending.push(&union.left);
                }
                Expr::Tuple(tuple) => pending.extend(tuple.elts.iter().rev()),
                Expr::List(list) => pending.extend(list.elts.iter().rev()),
                Expr::Starred(starred) => pending.push(&starred.value),
                Expr::StringLiteral(literal) => self.string_annotation(literal, scope),
                _ => {}
            }
        }
    }

    /// A name or member chain written in an annotation.
    fn type_name(&mut self, expr: &'a Expr, root: Cow<'a, str>, scope: ScopeId) {
        let id = self.reserve(expr, scope);
        self.types.push(RawTypeName {
            range: expr.range(),
            qualified: matches!(expr, Expr::Attribute(_)),
            expr: id,
            scope,
            root,
        });
    }

    /// A string annotation whose whole content is a dotted name, written without escapes: the
    /// name inside the quotes, with a synthesized name or member chain to resolve. Any other
    /// string records nothing.
    fn string_annotation(&mut self, literal: &'a ExprStringLiteral, scope: ScopeId) {
        let [part] = literal.value.as_slice() else {
            return;
        };
        let start = part.range.start().checked_add(part.flags.opener_len());
        let end = part.range.end().checked_sub(part.flags.closer_len());
        let (Some(start), Some(end)) = (start, end) else {
            return;
        };
        if start >= end {
            return;
        }
        let range = TextRange::new(start, end);
        let written = self
            .inputs
            .text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or_default();
        if written != &*part.value || !is_dotted_name(written) {
            return;
        }
        let mut parts = written.split('.').map(normalized);
        let Some(root) = parts.next() else {
            return;
        };
        let name = self.interner.name_text(root.clone());
        let mut expr = self.interner.synthetic(ExprKind::Name(name), scope);
        for part in parts {
            let name = self.interner.name_text(part);
            expr = self
                .interner
                .synthetic(ExprKind::Member { base: expr, name }, scope);
        }
        self.types.push(RawTypeName {
            range,
            qualified: written.contains('.'),
            expr,
            scope,
            root,
        });
    }

    /// Whether `name`, looked up from `scope` by Python's rules, names a PEP 695 type parameter:
    /// the nearest scope that writes it is a type-parameter scope. A class body is visible only
    /// from itself or through type-parameter scopes, and `global` sends the lookup to the
    /// module. Call once every write of the file is known.
    pub(super) fn type_parameter(&self, scope: ScopeId, name: &str) -> bool {
        let mut direct = true;
        let mut current = Some(scope);
        while let Some(index) = current {
            let Some(found) = self.scopes.get(index as usize) else {
                return false;
            };
            if found.globals.contains(&name) {
                return false;
            }
            let visible = index == scope || direct || found.kind != ScopeKind::Class;
            if visible && found.lexical.contains(name) {
                return found.kind == ScopeKind::Type;
            }
            direct &= found.kind == ScopeKind::Type;
            current = found.parent;
        }
        false
    }
}

/// The root name of a member chain whose every receiver is a member or a name.
fn chain_root(expr: &Expr) -> Option<&str> {
    let mut current = expr;
    loop {
        match current {
            Expr::Attribute(access) => current = &access.value,
            Expr::Name(name) => return Some(name.id.as_str()),
            _ => return None,
        }
    }
}

/// The last name a subscript's value is spelled with: `Literal` for `Literal` and
/// `typing.Literal`.
fn spelled(value: &Expr) -> Option<&str> {
    match value {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(access) => Some(access.attr.id.as_str()),
        _ => None,
    }
}

/// Whether `text` is identifiers joined by dots, with nothing else (no spaces, no keywords).
fn is_dotted_name(text: &str) -> bool {
    text.split('.').all(|part| {
        let mut characters = part.chars();
        characters
            .next()
            .is_some_and(|first| first == '_' || unicode_id_start::is_id_start(first))
            && characters.all(|next| next == '_' || unicode_id_start::is_id_continue(next))
            && !KEYWORDS.contains(&part)
    })
}

/// An identifier as Python reads it (NFKC), borrowed when it is ASCII.
fn normalized(part: &str) -> Cow<'_, str> {
    if part.is_ascii() {
        Cow::Borrowed(part)
    } else {
        Cow::Owned(part.nfkc().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_names_are_identifiers_joined_by_dots() {
        for text in ["Cart", "models.Cart", "_private.Ünïcode", "a.b.c"] {
            assert!(is_dotted_name(text), "{text}");
        }
        for text in [
            "",
            "not a name",
            "a..b",
            ".a",
            "a.",
            "None",
            "a.class",
            "1a",
            "a b",
            "list[int]",
        ] {
            assert!(!is_dotted_name(text), "{text}");
        }
    }
}
