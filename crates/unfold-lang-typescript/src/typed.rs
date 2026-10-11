//! Typed records from the normalized syntax and the selected blocks (`docs/languages/typescript.md`,
//! "Typed records"): a signature per function-like block, a parameter per parameter slot, the
//! written members of type declarations and classes, and a result per call and construction.
//! Everything is read from what is written; nothing is inferred.
//!
//! It also lists the written types whose names become type links ([`Records::linked`]): the
//! return and parameter types recorded here, class property types, variable annotations, a type
//! alias's aliased type, and an interface's body. Binding walks exactly those types for names.
//!
//! | Result | When the call, after parentheses, `await`, `!`, `as`, `satisfies`, and `<T>`, is |
//! | --- | --- |
//! | `variable` | a variable declaration's initializer (the variable block is the target) |
//! | `return` | a `return`'s expression, or an arrow function's expression body |
//! | `argument` | an argument of a call or `new` (the outer block and the argument's index) |
//! | `discard` | an expression statement's expression |
//! | `other` | anything else (a receiver, an operand, a spread, an assignment, a `throw`, …) |

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::Block;
use unfold_core::text::ByteRange;
use unfold_core::typed::{CallResult, Member, Parameter, Signature, WrittenType};
use unfold_core::vocab::{BlockKind, ParameterKind, ResultBinding};

use crate::syntax::{Cat, Flags, Label, NONE, Syntax};

/// The typed records of one file, and the written types whose names are linked.
#[derive(Debug, Default)]
pub(crate) struct Records {
    /// One per function-like block.
    pub(crate) signatures: Vec<Signature>,
    /// One per parameter slot.
    pub(crate) parameters: Vec<Parameter>,
    /// The written members of interfaces, type aliases, enums, and classes.
    pub(crate) members: Vec<Member>,
    /// One per call and construction block.
    pub(crate) results: Vec<CallResult>,
    /// The ranges of the written types whose type names get links: recorded return,
    /// parameter, and class property types, variable annotations, aliased types, and interface
    /// bodies.
    pub(crate) linked: FxHashSet<ByteRange>,
}

/// The typed records of a file whose syntax selected `blocks` (`block_of` maps a node to its
/// block's local ID, or [`NONE`]).
pub(crate) fn records(syntax: &Syntax<'_>, source: &str, block_of: &[u32], blocks: &[Block]) -> Records {
    let mut records = Records::default();
    let written = |range: ByteRange| -> Option<WrittenType> {
        let text = source.get(range.start as usize..range.end as usize)?;
        (!text.is_empty()).then(|| WrittenType {
            text: text.into(),
            range,
        })
    };
    for node in 0..syntax.len() as u32 {
        let Some(&local) = block_of.get(node as usize).filter(|&&local| local != NONE) else {
            continue;
        };
        let Some(kind) = blocks.get(local as usize).map(|block| block.kind) else {
            continue;
        };
        match kind {
            kind if kind.is_function_like() => {
                records.callable(syntax, node, local, &written);
            }
            BlockKind::Call | BlockKind::New => records.results.push(result(syntax, block_of, node, local)),
            BlockKind::Variable if syntax.cat(node) == Cat::VariableDeclaration => {
                if let Some(ty) = syntax.child(node, Label::Type) {
                    records.linked.insert(syntax.range(ty));
                }
            }
            BlockKind::TypeAlias => {
                if let Some(ty) = syntax.child(node, Label::Type) {
                    records.linked.insert(syntax.range(ty));
                }
            }
            BlockKind::Interface => {
                if let Some(body) = syntax.child(node, Label::Body) {
                    records.linked.insert(syntax.range(body));
                }
            }
            _ => {}
        }
    }
    records.members(syntax, block_of, blocks, source, &written);
    records
}

impl Records {
    /// The signature and parameters of function-like block `local`, converted from `node`.
    fn callable(
        &mut self,
        syntax: &Syntax<'_>,
        node: u32,
        local: u32,
        written: &impl Fn(ByteRange) -> Option<WrittenType>,
    ) {
        let mut returns = None;
        let mut index = 0;
        for child in syntax.children(node) {
            let found = syntax.node(child);
            match found.label {
                Label::Type => returns = written(found.range),
                Label::Parameters => {
                    let parameter = parameter(syntax, child, local, index, written);
                    if let Some(ty) = &parameter.ty {
                        self.linked.insert(ty.range);
                    }
                    self.parameters.push(parameter);
                    index += 1;
                }
                _ => {}
            }
        }
        if let Some(returns) = &returns {
            self.linked.insert(returns.range);
        }
        self.signatures.push(Signature {
            block: local,
            returns,
        });
    }

    /// The members of every selected type declaration and class, indexed per block in source
    /// order. A class's property types are linked.
    fn members(
        &mut self,
        syntax: &Syntax<'_>,
        block_of: &[u32],
        blocks: &[Block],
        source: &str,
        written: &impl Fn(ByteRange) -> Option<WrittenType>,
    ) {
        let text = |range: ByteRange| -> Option<Box<str>> {
            source
                .get(range.start as usize..range.end as usize)
                .filter(|text| !text.is_empty())
                .map(Box::from)
        };
        let mut next: FxHashMap<u32, u32> = FxHashMap::default();
        for draft in syntax.members() {
            let Some(&block) = block_of.get(draft.owner as usize).filter(|&&block| block != NONE) else {
                continue;
            };
            let Some(kind) = blocks.get(block as usize).map(|found| found.kind) else {
                continue;
            };
            if !kind.is_type_declaration() {
                continue;
            }
            let ty = draft.ty.and_then(written);
            if kind == BlockKind::Class
                && let Some(ty) = &ty
            {
                self.linked.insert(ty.range);
            }
            let index = next.entry(block).or_insert(0);
            self.members.push(Member {
                block,
                index: *index,
                kind: draft.kind,
                name: draft.name.and_then(text),
                optional: draft.optional,
                readonly: draft.readonly,
                default: draft.default,
                ty,
                value: draft.value.and_then(text),
            });
            *index += 1;
        }
    }
}

/// Parameter `index` of function-like block `local`, from its parameter node.
fn parameter(
    syntax: &Syntax<'_>,
    node: u32,
    local: u32,
    index: u32,
    written: &impl Fn(ByteRange) -> Option<WrittenType>,
) -> Parameter {
    let found = syntax.node(node);
    let name = syntax
        .child(node, Label::Name)
        .filter(|&name| syntax.cat(name) == Cat::Identifier)
        .map(|name| syntax.node(name).text);
    let rest = found.flags.has(Flags::REST);
    // `this` is a reserved word, so only an explicit `this` parameter is spelled that way.
    let kind = if rest {
        ParameterKind::Rest
    } else if index == 0 && name == Some("this") {
        ParameterKind::This
    } else {
        ParameterKind::Positional
    };
    Parameter {
        block: local,
        index,
        kind,
        name: name.filter(|name| !name.is_empty()).map(Box::from),
        optional: found.flags.has(Flags::OPTIONAL) && !rest,
        default: syntax.child(node, Label::Initializer).is_some(),
        ty: syntax
            .child(node, Label::Type)
            .and_then(|ty| written(syntax.range(ty))),
    }
}

/// Where the value of call or construction `node` (block `local`) lands.
fn result(syntax: &Syntax<'_>, block_of: &[u32], node: u32, local: u32) -> CallResult {
    let mut current = node;
    let mut awaited = false;
    loop {
        let found = syntax.node(current);
        if found.parent == NONE || found.label != Label::Expression {
            break;
        }
        let parent = syntax.cat(found.parent);
        if !parent.is_transparent() && parent != Cat::Await {
            break;
        }
        awaited |= parent == Cat::Await;
        current = found.parent;
    }
    let found = syntax.node(current);
    let block = |node: u32| {
        block_of
            .get(node as usize)
            .copied()
            .filter(|&block| block != NONE)
    };
    let mut result = CallResult {
        call: local,
        binding: ResultBinding::Other,
        awaited,
        target: None,
        position: None,
        // Set at link time, when the call's target is known.
        bound: None,
    };
    if found.parent == NONE {
        return result;
    }
    match (syntax.cat(found.parent), found.label) {
        (Cat::VariableDeclaration, Label::Initializer) => {
            if let Some(variable) = block(found.parent) {
                result.binding = ResultBinding::Variable;
                result.target = Some(variable);
            }
        }
        // An arrow function's expression body is its returned value; a block body is a node of
        // its own.
        (Cat::Return, Label::Expression) | (Cat::ArrowFunction, Label::Body) => {
            result.binding = ResultBinding::Return;
        }
        (Cat::Call | Cat::New, Label::Arguments) => {
            let position = syntax
                .children(found.parent)
                .filter(|&child| syntax.node(child).label == Label::Arguments)
                .position(|child| child == current);
            if let (Some(outer), Some(position)) = (block(found.parent), position) {
                result.binding = ResultBinding::Argument;
                result.target = Some(outer);
                result.position = Some(position as u32);
            }
        }
        (Cat::ExpressionStatement, Label::Expression) => result.binding = ResultBinding::Discard,
        _ => {}
    }
    result
}
