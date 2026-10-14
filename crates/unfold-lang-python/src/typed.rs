//! The typed records lowering writes for one file: a signature for every function (`def` and
//! `lambda`), a parameter for every written parameter, and the written members of every class.
//! The written type names of annotations, bases, and handler clauses come from the binder pass
//! ([`crate::bind`]), which knows their scopes.
//!
//! | Record | From |
//! | --- | --- |
//! | Signature | `def` (async too) and `lambda`: the return annotation after `->` |
//! | Parameter | Ruff's `Parameters` in order: `posonlyargs` → `positional_only`, `args` → `positional`, `vararg` → `rest`, `kwonlyargs` → `keyword_only`, `kwarg` → `keywords`; the whole parameter as written (`*` and `**` included), the identifier, whether a default is written, the annotation |
//! | Member | every annotated assignment to a plain name (not parenthesized) written directly in a class body, kind `property`, whatever the class's decorators or bases; its range is the whole assignment |

use ruff_python_ast::{Expr, Parameters, Stmt};
use ruff_text_size::{Ranged, TextRange};
use unfold_core::typed::{Member, Parameter, Signature, WrittenType};
use unfold_core::vocab::{MemberKind, ParameterKind};

use crate::ast::bytes;
use crate::lower::Lowered;

/// A written type: its text and range.
fn written(text: &str, range: TextRange) -> WrittenType {
    WrittenType {
        text: text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or_default()
            .into(),
        range: bytes(range),
    }
}

/// The signature of the function `node`, with its written return type.
pub(crate) fn signature(lowered: &mut Lowered<'_>, node: u32, returns: Option<&Expr>, text: &str) {
    lowered.signatures.push(Signature {
        node,
        returns: returns.map(|returns| written(text, returns.range())),
    });
}

/// One parameter record per written parameter of the function `node`, in Ruff's order (which
/// is the written order).
pub(crate) fn parameters(lowered: &mut Lowered<'_>, node: u32, parameters: &Parameters, text: &str) {
    let groups = [
        (ParameterKind::PositionalOnly, parameters.posonlyargs.as_slice()),
        (ParameterKind::Positional, parameters.args.as_slice()),
    ];
    let mut found = Vec::with_capacity(parameters.len());
    for (kind, group) in groups {
        found.extend(group.iter().map(|parameter| {
            (
                kind,
                &parameter.parameter,
                parameter.default.is_some(),
                parameter.range,
            )
        }));
    }
    found.extend(
        parameters
            .vararg
            .as_deref()
            .map(|parameter| (ParameterKind::Rest, parameter, false, parameter.range)),
    );
    found.extend(parameters.kwonlyargs.iter().map(|parameter| {
        (
            ParameterKind::KeywordOnly,
            &parameter.parameter,
            parameter.default.is_some(),
            parameter.range,
        )
    }));
    found.extend(
        parameters
            .kwarg
            .as_deref()
            .map(|parameter| (ParameterKind::Keywords, parameter, false, parameter.range)),
    );
    for (index, (kind, parameter, default, range)) in (0..).zip(found) {
        lowered.parameters.push(Parameter {
            node,
            index,
            range: bytes(range),
            kind,
            name: Some(parameter.name.id.as_str().into()),
            optional: false,
            default,
            ty: parameter
                .annotation
                .as_deref()
                .map(|annotation| written(text, annotation.range())),
        });
    }
}

/// The written members of the class `node`: its annotated plain names.
pub(crate) fn members(lowered: &mut Lowered<'_>, node: u32, body: &[Stmt], text: &str) {
    let members = body.iter().filter_map(|member| match member {
        // `simple`: a name not in parentheses, which Python records as an annotation of the
        // class (`(x): int` is not).
        Stmt::AnnAssign(assignment) if assignment.simple => match &*assignment.target {
            Expr::Name(name) => Some((assignment, name)),
            _ => None,
        },
        _ => None,
    });
    for (index, (assignment, name)) in (0..).zip(members) {
        lowered.members.push(Member {
            node,
            index,
            range: bytes(assignment.range),
            kind: MemberKind::Property,
            name: text
                .get(name.range.start().to_usize()..name.range.end().to_usize())
                .map(Into::into),
            optional: false,
            readonly: false,
            default: assignment.value.is_some(),
            ty: Some(written(text, assignment.annotation.range())),
            value: None,
        });
    }
}
