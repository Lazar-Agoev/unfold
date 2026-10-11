//! The at-rest label of one arrow (`source` occurrence, `seq`).
//!
//! - **A call or construction** (`seq` 0, resolved): one input per argument in written order:
//!   the written type of the parameter it fills, else that parameter's name, else nothing (an
//!   argument that fills no proven parameter, or a pattern parameter: the UI shows `…`). A
//!   Python keyword argument keeps its keyword. The output is the callee's written return type,
//!   or the class's name (a `declared` span) for a construction, or absent.
//! - **A passed callable** (`seq` > 0, a resolved `references` edge): its own signature, one
//!   input per parameter that takes an argument (every kind but TypeScript's `this`) after the
//!   ones the passed value binds itself (the argument record's proven bound count: `obj.method`
//!   binds `self`), and its return type (a class's name for a passed class). When a function's
//!   bound count isn't proven, the label has no inputs and says so (`inputs_unknown`).
//!
//! External and unresolved edges, imports, and unknown occurrences have no label.

use std::rc::Rc;

use unfold_core::vocab::{BlockKind, EdgeKind, Resolution};
use unfold_core::{Edge, Key, Pairing, Parameter};

use super::facts::{Facts, FileFacts};
use super::text::{outcome_span, whole_text};
use super::{LabelInput, LineLabel, TypeText};
use crate::error::Result;

/// A resolved edge's callee, located in its file.
pub(super) struct Callee {
    pub(super) file: Rc<FileFacts>,
    pub(super) local: u32,
    pub(super) key: Key,
}

impl Callee {
    /// The callee of `edge`, when it resolved to a block that exists.
    pub(super) fn of(facts: &mut Facts<'_>, edge: &Edge) -> Result<Option<Self>> {
        let Some(key) = edge
            .target
            .as_ref()
            .filter(|_| edge.resolution == Resolution::Resolved)
        else {
            return Ok(None);
        };
        Ok(facts.locate(key.as_str())?.map(|(file, local)| Self {
            file,
            local,
            key: key.clone(),
        }))
    }

    /// The class an edge of `kind` to this callee constructs: a class target itself, or the
    /// class of a constructor (or of the method a Python construction reaches, `__init__`).
    pub(super) fn constructed_class(&self, kind: EdgeKind) -> Option<u32> {
        class_of(&self.file, self.local, kind)
    }

    /// What a call of kind `kind` gives back: the constructed class's name, else the written
    /// return type.
    pub(super) fn output(&self, kind: EdgeKind) -> Option<TypeText> {
        match self.constructed_class(kind) {
            Some(class) => Some(class_name(&self.file, class)),
            None => self
                .file
                .signature(self.local)
                .and_then(|signature| signature.returns.as_ref())
                .map(|returns| self.file.type_text(returns)),
        }
    }
}

/// The class block `local` of `file` stands for when reached by an edge of `kind`: itself for a
/// class, its parent class for a constructor, or for a method reached by a construction.
pub(super) fn class_of(file: &FileFacts, local: u32, kind: EdgeKind) -> Option<u32> {
    let block = file.block(local)?;
    let parent_class = || {
        file.parent(local).filter(|&parent| {
            file.block(parent)
                .is_some_and(|parent| parent.kind == BlockKind::Class)
        })
    };
    match block.kind {
        BlockKind::Class => Some(local),
        BlockKind::Constructor => parent_class(),
        BlockKind::Method if kind == EdgeKind::Constructs => parent_class(),
        _ => None,
    }
}

/// The name of class `class` as a `declared` type.
pub(super) fn class_name(file: &FileFacts, class: u32) -> TypeText {
    let key = file.block(class).map(|block| block.key.clone());
    whole_text(&file.label(class), |end| {
        outcome_span(0, end, Resolution::Resolved, key.as_ref(), None, None)
    })
}

/// The input a parameter shows: its written type, else its name.
pub(super) fn parameter_input(file: &FileFacts, parameter: &Parameter) -> LabelInput {
    match &parameter.ty {
        Some(ty) => LabelInput {
            ty: Some(file.type_text(ty)),
            ..LabelInput::default()
        },
        None => LabelInput {
            name: parameter.name.as_deref().map(str::to_owned),
            ..LabelInput::default()
        },
    }
}

/// The label of the arrow for edge `seq` of occurrence `source`, or `None` when it has none.
pub fn line_label(facts: &mut Facts<'_>, source: &str, seq: u32) -> Result<Option<LineLabel>> {
    let Some((caller, call)) = facts.locate(source)? else {
        return Ok(None);
    };
    let Some(edge) = caller.edge(call, seq) else {
        return Ok(None);
    };
    let call_edge = seq == 0 && matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs);
    let reference = seq > 0 && edge.kind == EdgeKind::References;
    if !(call_edge || reference) {
        return Ok(None);
    }
    let Some(callee) = Callee::of(facts, edge)? else {
        return Ok(None);
    };
    let mut inputs_unknown = false;
    let inputs = if call_edge {
        caller
            .arguments(call)
            .iter()
            .map(|argument| {
                let parameter = match argument.pairing {
                    Some(Pairing::Parameter(index)) => callee.file.parameter(callee.local, index),
                    _ => None,
                };
                let input = parameter.map_or_else(LabelInput::default, |parameter| {
                    parameter_input(&callee.file, parameter)
                });
                LabelInput {
                    keyword: argument.keyword.as_deref().map(str::to_owned),
                    ..input
                }
            })
            .collect()
    } else {
        let function = callee
            .file
            .block(callee.local)
            .is_some_and(|block| block.kind.is_function_like());
        let bound = edge.position.and_then(|position| {
            caller
                .arguments(call)
                .iter()
                .find(|argument| argument.index == position)?
                .bound
        });
        match (function, bound) {
            (true, None) => {
                inputs_unknown = true;
                Vec::new()
            }
            (_, bound) => callee
                .file
                .parameters(callee.local)
                .iter()
                .filter(|parameter| parameter.kind.takes_argument() && parameter.index >= bound.unwrap_or(0))
                .map(|parameter| parameter_input(&callee.file, parameter))
                .collect(),
        }
    };
    Ok(Some(LineLabel {
        inputs,
        inputs_unknown,
        output: callee.output(edge.kind),
    }))
}
