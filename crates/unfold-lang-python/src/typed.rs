//! The typed facts of one file, recorded from the selection walk ([`unfold_engine::select::walk`]):
//! a signature for every `def` and `lambda`, a parameter for every parameter slot, the written
//! members of every class, and where each call's value lands. The written type names of
//! annotations come from the binder pass ([`crate::bind`]), which knows their scopes.
//!
//! | Record | From |
//! | --- | --- |
//! | Signature | `def` (async too) and `lambda`: the return annotation after `->`, as its `return_type` slot |
//! | Parameter | Ruff's `Parameters` in order: `posonlyargs` → `positional_only`, `args` → `positional`, `vararg` → `rest`, `kwonlyargs` → `keyword_only`, `kwarg` → `keywords`; the identifier, whether a default is written, the annotation |
//! | Member | every annotated assignment to a plain name (not parenthesized) written directly in a class body, kind `property`, whatever the class's decorators or bases |
//! | Result | the nearest construct a call's value lands in once parentheses and `await` are unwrapped |
//!
//! Where a call's value lands:
//!
//! | Binding | When the call (unwrapped) is exactly |
//! | --- | --- |
//! | `variable` | the value of an assignment (for `a = b = f()`, the innermost level), of an annotated assignment, or of an assignment expression; the target is that variable block |
//! | `return` | a `return`'s value or a lambda's body |
//! | `argument` | a positional argument or a keyword argument's value of an outer call; the target is the outer call, the position its argument slot |
//! | `discard` | an expression statement's value |
//! | `other` | anything else: an augmented assignment's value, `*f()` and `**f()`, a receiver, an operand, a `raise`, a decorator, a class base, a `type` statement's value, ... |
//!
//! `awaited` records that an `await` applies to the call directly (through nested `await`s,
//! which the binder pass finds), whatever its binding.

use ruff_python_ast::{ArgOrKeyword, Expr, Parameters, Stmt};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::typed::{CallResult, Member, Parameter, Signature, WrittenType};
use unfold_core::vocab::{MemberKind, ParameterKind, ResultBinding};

use crate::ast::arguments;
use crate::mapping::bytes;
use crate::syntax::{Node, Syntax};
use crate::values::address;

/// Where a call's value lands, before it is known to be a call block.
#[derive(Debug, Clone, Copy)]
struct Landing {
    binding: ResultBinding,
    target: Option<u32>,
    position: Option<u32>,
}

/// The typed facts of one file under construction.
pub(crate) struct Typed<'s, 'a> {
    syntax: &'s Syntax<'a>,
    signatures: Vec<Signature>,
    parameters: Vec<Parameter>,
    members: Vec<Member>,
    /// Where the value of each call lands, by the call's address.
    landings: FxHashMap<usize, Landing>,
}

impl<'s, 'a> Typed<'s, 'a> {
    pub(crate) fn new(syntax: &'s Syntax<'a>) -> Self {
        Self {
            syntax,
            signatures: Vec::new(),
            parameters: Vec::new(),
            members: Vec::new(),
            landings: FxHashMap::default(),
        }
    }

    /// Records what the block `local`, selected for `node`, contributes.
    pub(crate) fn selected(&mut self, node: Node<'a>, local: u32) {
        match node {
            Node::Stmt(stmt) => self.statement(stmt, local),
            Node::ExprStatement(statement) => {
                self.land(&statement.value, ResultBinding::Discard, local, None);
            }
            // Only the innermost level of a chain is initialized by the value itself.
            Node::Assign(assign, 0) => self.land(&assign.value, ResultBinding::Variable, local, None),
            Node::Expr(Expr::Lambda(lambda), _) => {
                self.signatures.push(Signature {
                    block: local,
                    returns: None,
                });
                if let Some(parameters) = &lambda.parameters {
                    self.parameters(parameters, local);
                }
                self.land(&lambda.body, ResultBinding::Return, local, None);
            }
            Node::Expr(Expr::Named(named), _) => {
                self.land(&named.value, ResultBinding::Variable, local, None);
            }
            Node::Expr(Expr::Call(call), _) if self.syntax.proofs.dynamic_import(call).is_none() => {
                for (position, argument) in arguments(&call.arguments).enumerate() {
                    let value = match argument {
                        ArgOrKeyword::Arg(Expr::Starred(_)) => continue,
                        ArgOrKeyword::Arg(value) => value,
                        ArgOrKeyword::Keyword(keyword) if keyword.arg.is_some() => &keyword.value,
                        ArgOrKeyword::Keyword(_) => continue,
                    };
                    self.land(value, ResultBinding::Argument, local, Some(position as u32));
                }
            }
            _ => {}
        }
    }

    fn statement(&mut self, stmt: &'a Stmt, local: u32) {
        match stmt {
            Stmt::FunctionDef(function) => {
                let returns = function.returns.as_deref().map(|returns| {
                    let range = self.syntax.range_in(returns, stmt.range());
                    self.written(range)
                });
                self.signatures.push(Signature {
                    block: local,
                    returns,
                });
                self.parameters(&function.parameters, local);
            }
            Stmt::ClassDef(class) => {
                let members = class.body.iter().filter_map(|member| match member {
                    // `simple`: a name not in parentheses, which Python records as an
                    // annotation of the class (`(x): int` is not).
                    Stmt::AnnAssign(assignment) if assignment.simple => match &*assignment.target {
                        Expr::Name(name) => Some((assignment, name)),
                        _ => None,
                    },
                    _ => None,
                });
                for (index, (assignment, name)) in members.enumerate() {
                    let range = self.syntax.range_in(&assignment.annotation, assignment.range);
                    self.members.push(Member {
                        block: local,
                        index: index as u32,
                        kind: MemberKind::Property,
                        name: Some(self.syntax.slice(name.range).into()),
                        optional: false,
                        readonly: false,
                        default: assignment.value.is_some(),
                        ty: Some(self.written(range)),
                        value: None,
                    });
                }
            }
            Stmt::Return(node) => {
                if let Some(value) = &node.value {
                    self.land(value, ResultBinding::Return, local, None);
                }
            }
            Stmt::AnnAssign(node) => {
                if let Some(value) = &node.value {
                    self.land(value, ResultBinding::Variable, local, None);
                }
            }
            _ => {}
        }
    }

    /// One parameter record per parameter slot, in Ruff's order (which is the slots' order).
    fn parameters(&mut self, parameters: &'a Parameters, local: u32) {
        let groups = [
            (ParameterKind::PositionalOnly, parameters.posonlyargs.as_slice()),
            (ParameterKind::Positional, parameters.args.as_slice()),
        ];
        let mut written = Vec::with_capacity(parameters.len());
        for (kind, group) in groups {
            written.extend(group.iter().map(|parameter| {
                (
                    kind,
                    &parameter.parameter,
                    parameter.default.is_some(),
                    parameter.range,
                )
            }));
        }
        written.extend(
            parameters
                .vararg
                .as_deref()
                .map(|parameter| (ParameterKind::Rest, parameter, false, parameter.range)),
        );
        written.extend(parameters.kwonlyargs.iter().map(|parameter| {
            (
                ParameterKind::KeywordOnly,
                &parameter.parameter,
                parameter.default.is_some(),
                parameter.range,
            )
        }));
        written.extend(
            parameters
                .kwarg
                .as_deref()
                .map(|parameter| (ParameterKind::Keywords, parameter, false, parameter.range)),
        );
        for (index, (kind, parameter, default, slot)) in written.into_iter().enumerate() {
            let ty = parameter.annotation.as_deref().map(|annotation| {
                let range = self.syntax.range_in(annotation, slot);
                self.written(range)
            });
            self.parameters.push(Parameter {
                block: local,
                index: index as u32,
                kind,
                name: Some(parameter.name.id.as_str().into()),
                optional: false,
                default,
                ty,
            });
        }
    }

    /// Records where `value` lands, when it is a call once `await`s are unwrapped.
    fn land(&mut self, value: &'a Expr, binding: ResultBinding, target: u32, position: Option<u32>) {
        let mut value = value;
        while let Expr::Await(inner) = value {
            value = &inner.value;
        }
        if let Expr::Call(_) = value {
            let target =
                matches!(binding, ResultBinding::Variable | ResultBinding::Argument).then_some(target);
            self.landings.insert(
                address(value),
                Landing {
                    binding,
                    target,
                    position,
                },
            );
        }
    }

    fn written(&self, range: TextRange) -> WrittenType {
        WrittenType {
            text: self.syntax.slice(range).into(),
            range: bytes(range),
        }
    }

    /// The signatures, parameters, and members, and the result of each call block (`calls`:
    /// each call's address and block), given the calls an `await` applies to.
    pub(crate) fn finish(
        self,
        calls: &[(usize, u32)],
        awaited: &FxHashSet<usize>,
    ) -> (Vec<Signature>, Vec<Parameter>, Vec<Member>, Vec<CallResult>) {
        let results = calls
            .iter()
            .map(|&(call, block)| {
                let landing = self.landings.get(&call).copied().unwrap_or(Landing {
                    binding: ResultBinding::Other,
                    target: None,
                    position: None,
                });
                CallResult {
                    call: block,
                    binding: landing.binding,
                    awaited: awaited.contains(&call),
                    target: landing.target,
                    position: landing.position,
                    // Set at link time, when the call's target is known.
                    bound: None,
                }
            })
            .collect();
        (self.signatures, self.parameters, self.members, results)
    }
}
