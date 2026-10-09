//! The argument syntax entrance detectors read ([`Site`]), recorded during extraction because
//! linking never sees a syntax tree.
//!
//! Only call sites a detector may recognize are considered: every `new`, every call of a name
//! (a `node-cron` `schedule` import may have any local name), and every call of a member named
//! like an Express registration (`get`, `post`, …, `use`) or `schedule`. For those, the first two
//! arguments keep their decoded string literal, a route's handler arrays keep their flattened
//! elements (each with a value of its own), and a construction's second argument keeps its
//! written text when it is itself a `new` (BullMQ's `new URL(…)` processor). A site with nothing
//! beyond its values is not recorded, so ordinary code adds nothing to the facts.

use crate::payload::{ArgumentSyntax, Element, Site};
use crate::syntax::{Cat, Label, NONE, Syntax};
use crate::values::Table;

/// Express route methods: a call of a member with one of these names may register a route.
const ROUTE_METHODS: &[&str] = &["get", "head", "post", "put", "patch", "delete", "options", "all"];

/// What a call's callee says about the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Callee {
    /// A member named like a route registration: handler arrays matter.
    Route,
    /// Any other callee a detector may recognize.
    Other,
}

/// The sites of one file, in block order.
pub(crate) fn collect(
    syntax: &Syntax<'_>,
    source: &str,
    block_of: &[u32],
    table: &mut Table<'_, '_>,
) -> Vec<Site> {
    let mut sites = Vec::new();
    for node in 0..syntax.len() as u32 {
        let block = block_of[node as usize];
        if block == NONE {
            continue;
        }
        let cat = syntax.cat(node);
        let callee = match cat {
            Cat::New => Callee::Other,
            Cat::Call => match recognizable(syntax, node) {
                Some(callee) => callee,
                None => continue,
            },
            _ => continue,
        };
        let arguments: Vec<u32> = syntax
            .children(node)
            .filter(|&child| syntax.node(child).label == Label::Arguments)
            .collect();
        let mut recorded = Vec::with_capacity(arguments.len());
        for (position, &argument) in arguments.iter().enumerate() {
            let unwrapped = syntax.unwrap(argument);
            let found = syntax.node(unwrapped);
            let syntax_of = match found.cat {
                Cat::StringLiteral | Cat::NoSubstitutionTemplate if position < 2 => {
                    ArgumentSyntax::Literal(found.text.into())
                }
                Cat::Array if callee == Callee::Route && position > 0 => {
                    ArgumentSyntax::Array(flatten(syntax, unwrapped, table))
                }
                Cat::New if cat == Cat::New && position == 1 => {
                    let written = syntax.range(argument);
                    let text = source
                        .get(written.start as usize..written.end as usize)
                        .unwrap_or_default();
                    ArgumentSyntax::Construction(text.into())
                }
                _ => ArgumentSyntax::Plain,
            };
            recorded.push(syntax_of);
        }
        if recorded.iter().any(|argument| *argument != ArgumentSyntax::Plain) {
            sites.push(Site {
                block,
                arguments: recorded,
            });
        }
    }
    sites
}

/// Whether a call's callee is one a detector may recognize: a name, or a member named like a
/// registration.
fn recognizable(syntax: &Syntax<'_>, call: u32) -> Option<Callee> {
    let callee = syntax.unwrap(syntax.child(call, Label::Expression)?);
    match syntax.cat(callee) {
        Cat::Identifier => Some(Callee::Other),
        Cat::PropertyAccess => {
            let name = syntax
                .child(callee, Label::Name)
                .map(|name| syntax.node(name).text)?;
            if ROUTE_METHODS.contains(&name) {
                Some(Callee::Route)
            } else if matches!(name, "use" | "schedule") {
                Some(Callee::Other)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The elements of an array literal with nested array literals (through transparent wrappers)
/// flattened in written order, iteratively so deep nesting can't overflow the stack.
fn flatten(syntax: &Syntax<'_>, array: u32, table: &mut Table<'_, '_>) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut pending: Vec<u32> = syntax.children(array).collect();
    pending.reverse();
    while let Some(element) = pending.pop() {
        let unwrapped = syntax.unwrap(element);
        match syntax.cat(unwrapped) {
            Cat::Array => {
                let start = pending.len();
                pending.extend(syntax.children(unwrapped));
                pending[start..].reverse();
            }
            Cat::Hole => elements.push(Element::Hole),
            Cat::Spread => elements.push(Element::Spread(table.value(element))),
            _ => elements.push(Element::Value(table.value(element))),
        }
    }
    elements
}
