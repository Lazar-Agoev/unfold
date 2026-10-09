//! Block names and value labels, from syntax only (never from resolution).
//!
//! A declaration is named by its identifier, private name, or decoded string or numeric literal;
//! a computed name gives no name. Constructors are named `constructor`. An anonymous function or
//! class takes the name of what it initializes (a variable, parameter, binding default, class
//! field, or object property), the identifier an `=`, `&&=`, `||=`, or `??=` assigns it to, or
//! `default` under `export default`.

use unfold_core::vocab::BlockKind;
use unfold_engine::edges::text;

use crate::syntax::{Cat, Flags, Label, NONE, Syntax};

/// The text of a node that can name a declaration.
pub(crate) fn declared_name<'a>(syntax: &Syntax<'a>, node: u32) -> Option<&'a str> {
    let found = syntax.node(node);
    (matches!(found.cat, Cat::Identifier | Cat::PrivateIdentifier) || found.cat.is_name_literal())
        .then_some(found.text)
}

/// A block's name, or `None` for an unnamed block.
pub(crate) fn block_name<'a>(syntax: &Syntax<'a>, node: u32, kind: BlockKind) -> Option<&'a str> {
    let name_child = syntax.child(node, Label::Name);
    let name = match kind {
        BlockKind::Constructor => Some("constructor"),
        BlockKind::Variable => name_child
            .filter(|&child| syntax.cat(child) == Cat::Identifier)
            .and_then(|child| declared_name(syntax, child)),
        BlockKind::Function | BlockKind::Class => name_child
            .and_then(|child| declared_name(syntax, child))
            .filter(|name| !name.is_empty())
            .or_else(|| inferred_name(syntax, node)),
        BlockKind::Method | BlockKind::Getter | BlockKind::Setter => {
            name_child.and_then(|child| declared_name(syntax, child))
        }
        _ => None,
    };
    name.filter(|name| !name.is_empty())
}

/// The name an anonymous function or class takes from its context.
fn inferred_name<'a>(syntax: &Syntax<'a>, node: u32) -> Option<&'a str> {
    if syntax.node(node).flags.has(Flags::DEFAULT) {
        return Some("default");
    }
    let mut value = node;
    loop {
        let found = syntax.node(value);
        if found.parent == NONE
            || found.label != Label::Expression
            || !syntax.cat(found.parent).is_transparent()
        {
            break;
        }
        value = found.parent;
    }
    let found = syntax.node(value);
    if found.parent == NONE {
        return None;
    }
    let parent = syntax.node(found.parent);
    match parent.cat {
        Cat::ExportAssignment if parent.flags.has(Flags::DEFAULT) => Some("default"),
        Cat::VariableDeclaration
        | Cat::Parameter
        | Cat::BindingElement
        | Cat::Property
        | Cat::PropertyAssignment
            if found.label == Label::Initializer =>
        {
            syntax
                .child(found.parent, Label::Name)
                .and_then(|name| declared_name(syntax, name))
        }
        Cat::Assignment
            if found.label == Label::Right && matches!(parent.text, "=" | "&&=" | "||=" | "??=") =>
        {
            syntax
                .child(found.parent, Label::Left)
                .filter(|&left| syntax.cat(left) == Cat::Identifier)
                .and_then(|left| declared_name(syntax, left))
        }
        _ => None,
    }
}

/// The short written label of an expression (see [`unfold_engine::edges::text`]), computed from
/// the already-known label of the expression it depends on (an access's receiver, a call's
/// callee, a construction's target, or a template's tag).
pub(crate) fn label_from(syntax: &Syntax<'_>, source: &str, node: u32, dependency: Option<&str>) -> String {
    let unwrapped = syntax.unwrap(node);
    let found = syntax.node(unwrapped);
    let inner = dependency.unwrap_or(text::OTHER);
    match found.cat {
        Cat::Identifier | Cat::PrivateIdentifier => found.text.to_owned(),
        Cat::This => "this".to_owned(),
        Cat::Super => "super".to_owned(),
        Cat::PropertyAccess => {
            let name = syntax
                .child(unwrapped, Label::Name)
                .and_then(|name| declared_name(syntax, name))
                .unwrap_or_default();
            text::member(inner, name)
        }
        Cat::ElementAccess => {
            let key = syntax
                .child(unwrapped, Label::ArgumentExpression)
                .filter(|&key| syntax.cat(key).is_name_literal());
            match key {
                Some(key) => {
                    let written = syntax.range(key);
                    let spelling = source
                        .get(written.start as usize..written.end as usize)
                        .unwrap_or("…");
                    text::literal_key(inner, spelling)
                }
                None => text::computed_key(inner),
            }
        }
        Cat::Call => text::call_result(inner),
        // A dynamic import used as a value has no callee expression.
        Cat::ImportCall => text::call_result(text::OTHER),
        Cat::New | Cat::TaggedTemplate => inner.to_owned(),
        _ => text::OTHER.to_owned(),
    }
}
