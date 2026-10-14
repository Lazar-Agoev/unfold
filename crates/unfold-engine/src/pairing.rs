//! Pairing a resolved call's arguments with its target's parameters (`docs/graph-model.md`,
//! "Pairing").
//!
//! Pairing is language neutral. The only language question is how many of the target's leading
//! parameters the call binds itself ([`Linked::bound_parameters`](crate::Linked::bound_parameters));
//! everything else follows from the written arguments (expansions and keywords) and the target's
//! parameter records. The rules, applied to the arguments in written order:
//!
//! 1. The bound parameters and every `this` parameter are dropped; the rest are the *takers*.
//! 2. Positional arguments fill the takers that accept positions (`positional`,
//!    `positional_only`) left to right; past them a `rest` taker takes every further positional
//!    argument; without one a further argument is `no_parameter`.
//! 3. Once an argument is spread, it and every later positional argument are `spread`. A Python
//!    `**` expansion is `spread` itself; keyword arguments after it still pair by name.
//! 4. A keyword argument fills the taker of that name that accepts keywords (`positional`,
//!    `keyword_only`) unless a positional argument filled it already, else the `keywords`
//!    taker, else it is `no_parameter`.
//! 5. A class target (no direct constructor) takes nothing only when its language proves it
//!    ([`Linked::class_takes_no_arguments`](crate::Linked::class_takes_no_arguments): no base, no
//!    decorator, and nothing else that supplies a constructor): every argument is then
//!    `no_parameter`, and otherwise `no_signature`.
//!
//! Equal ordinals never prove anything: nothing here guesses past these rules.

use unfold_core::typed::{Pairing, Parameter};
use unfold_core::vocab::{Expansion, NodeKind, PairingReason, ParameterKind};

use crate::facts::ArgumentFact;
use crate::program::{FactsSet, NodeRef};

/// What a resolved call's target offers its arguments.
#[derive(Debug, Clone, Copy)]
pub enum Takers<'a> {
    /// A callable: its parameter records, and how many leading ones the call binds itself
    /// (`None` when that can't be proven).
    Callable {
        /// The target's parameters, in index order.
        parameters: &'a [Parameter],
        /// The leading parameters the call binds without an argument.
        bound: Option<u32>,
    },
    /// A class without a direct constructor, and whether its language proves that
    /// constructing it takes no arguments.
    Class {
        /// Proven to take no arguments: no base, no decorator, no other constructor.
        takes_nothing: bool,
    },
}

impl<'a> Takers<'a> {
    /// What `target` offers, from the program's facts. `bound` is the language's bound count
    /// for a callable; `takes_nothing` is asked only for a class.
    pub fn of(
        program: &'a FactsSet,
        target: NodeRef,
        bound: Option<u32>,
        takes_nothing: impl FnOnce() -> bool,
    ) -> Option<Self> {
        let facts = program.facts(target.file)?;
        match facts.graph.nodes().get(target.local as usize)?.kind() {
            NodeKind::Class => {
                return Some(Self::Class {
                    takes_nothing: takes_nothing(),
                });
            }
            NodeKind::Function => {}
            _ => return None,
        }
        let parameters = &facts.graph.types().parameters;
        let start = parameters.partition_point(|parameter| parameter.node < target.local);
        let end = parameters.partition_point(|parameter| parameter.node <= target.local);
        Some(Self::Callable {
            parameters: &parameters[start..end],
            bound,
        })
    }
}

/// The pairing of each argument, in written order.
pub fn pair(arguments: &[ArgumentFact], takers: Takers<'_>) -> Vec<Pairing> {
    let (parameters, bound) = match takers {
        Takers::Class { takes_nothing } => {
            let reason = if takes_nothing {
                PairingReason::NoParameter
            } else {
                PairingReason::NoSignature
            };
            return vec![Pairing::Unpaired(reason); arguments.len()];
        }
        Takers::Callable { parameters, bound } => (parameters, bound),
    };
    let Some(bound) = bound else {
        return vec![Pairing::Unpaired(PairingReason::Receiver); arguments.len()];
    };
    let takers: Vec<&Parameter> = parameters
        .iter()
        .skip(bound as usize)
        .filter(|parameter| parameter.kind.takes_argument())
        .collect();
    let positional: Vec<&Parameter> = takers
        .iter()
        .copied()
        .filter(|parameter| parameter.kind.takes_position())
        .collect();
    let rest = takers
        .iter()
        .find(|parameter| parameter.kind == ParameterKind::Rest);
    let keywords = takers
        .iter()
        .find(|parameter| parameter.kind == ParameterKind::Keywords);
    let mut filled: Vec<u32> = Vec::new();
    let mut next = positional.iter();
    let mut spread = false;
    let mut pairings = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let pairing = match (argument.expand, &argument.keyword) {
            (Some(Expansion::Spread), _) => {
                spread = true;
                Pairing::Unpaired(PairingReason::Spread)
            }
            (Some(Expansion::Keywords), _) => Pairing::Unpaired(PairingReason::Spread),
            (None, Some(keyword)) => {
                let named = takers.iter().find(|parameter| {
                    parameter.kind.takes_keyword() && parameter.name.as_deref() == Some(&**keyword)
                });
                match (named, keywords) {
                    (Some(named), _) if filled.contains(&named.index) => {
                        Pairing::Unpaired(PairingReason::NoParameter)
                    }
                    (Some(named), _) => {
                        filled.push(named.index);
                        Pairing::Parameter(named.index)
                    }
                    (None, Some(keywords)) => Pairing::Parameter(keywords.index),
                    (None, None) => Pairing::Unpaired(PairingReason::NoParameter),
                }
            }
            (None, None) if spread => Pairing::Unpaired(PairingReason::Spread),
            (None, None) => match (next.next(), rest) {
                (Some(parameter), _) => {
                    filled.push(parameter.index);
                    Pairing::Parameter(parameter.index)
                }
                (None, Some(rest)) => Pairing::Parameter(rest.index),
                (None, None) => Pairing::Unpaired(PairingReason::NoParameter),
            },
        };
        pairings.push(pairing);
    }
    pairings
}

#[cfg(test)]
mod tests {
    use unfold_core::ByteRange;

    use super::*;

    fn parameter(index: u32, kind: ParameterKind, name: &str) -> Parameter {
        Parameter {
            node: 1,
            index,
            range: ByteRange::new(index, index + 1).unwrap(),
            kind,
            name: Some(name.into()),
            optional: false,
            default: false,
            ty: None,
        }
    }

    fn positional(position: u32) -> ArgumentFact {
        ArgumentFact {
            range: ByteRange::new(position, position + 1).unwrap(),
            value: Some(0),
            expand: None,
            keyword: None,
        }
    }

    fn keyword(position: u32, name: &str) -> ArgumentFact {
        ArgumentFact {
            keyword: Some(name.into()),
            ..positional(position)
        }
    }

    fn expanded(position: u32, expand: Expansion) -> ArgumentFact {
        ArgumentFact {
            expand: Some(expand),
            value: None,
            ..positional(position)
        }
    }

    fn callable(parameters: &[Parameter], bound: Option<u32>) -> Takers<'_> {
        Takers::Callable { parameters, bound }
    }

    use Pairing::{Parameter as P, Unpaired as U};
    use PairingReason as R;

    #[test]
    fn positional_arguments_fill_positions_then_the_rest_parameter() {
        let parameters = [
            parameter(0, ParameterKind::This, "this"),
            parameter(1, ParameterKind::Positional, "cart"),
            parameter(2, ParameterKind::Positional, "user"),
            parameter(3, ParameterKind::Rest, "items"),
        ];
        let arguments: Vec<_> = (0..5).map(positional).collect();
        assert_eq!(
            pair(&arguments, callable(&parameters, Some(0))),
            [P(1), P(2), P(3), P(3), P(3)]
        );
        let without_rest = &parameters[..3];
        assert_eq!(
            pair(&arguments[..3], callable(without_rest, Some(0))),
            [P(1), P(2), U(R::NoParameter)]
        );
    }

    #[test]
    fn a_spread_makes_itself_and_later_positions_spread() {
        let parameters = [
            parameter(0, ParameterKind::Positional, "a"),
            parameter(1, ParameterKind::Positional, "b"),
        ];
        let arguments = [positional(0), expanded(1, Expansion::Spread), positional(2)];
        assert_eq!(
            pair(&arguments, callable(&parameters, Some(0))),
            [P(0), U(R::Spread), U(R::Spread)]
        );
    }

    #[test]
    fn python_keywords_pair_by_name_and_fall_back_to_kwargs() {
        let parameters = [
            parameter(0, ParameterKind::Positional, "self"),
            parameter(1, ParameterKind::PositionalOnly, "amount"),
            parameter(2, ParameterKind::Positional, "currency"),
            parameter(3, ParameterKind::Rest, "args"),
            parameter(4, ParameterKind::KeywordOnly, "retry"),
            parameter(5, ParameterKind::Keywords, "options"),
        ];
        let arguments = [
            positional(0),
            keyword(1, "retry"),
            keyword(2, "amount"),
            keyword(3, "trace"),
            expanded(4, Expansion::Keywords),
            keyword(5, "currency"),
        ];
        assert_eq!(
            pair(&arguments, callable(&parameters, Some(1))),
            [P(1), P(4), P(5), P(5), U(R::Spread), P(2)]
        );
        let without_kwargs = &parameters[..5];
        assert_eq!(
            pair(&arguments[..4], callable(without_kwargs, Some(1))),
            [P(1), P(4), U(R::NoParameter), U(R::NoParameter)]
        );
    }

    #[test]
    fn a_keyword_for_a_filled_parameter_is_no_parameter() {
        let parameters = [parameter(0, ParameterKind::Positional, "amount")];
        let arguments = [positional(0), keyword(1, "amount")];
        assert_eq!(
            pair(&arguments, callable(&parameters, Some(0))),
            [P(0), U(R::NoParameter)]
        );
    }

    #[test]
    fn an_unproven_receiver_pairs_nothing() {
        let parameters = [parameter(0, ParameterKind::Positional, "self")];
        assert_eq!(
            pair(&[positional(0)], callable(&parameters, None)),
            [U(R::Receiver)]
        );
    }

    #[test]
    fn a_class_without_a_constructor_takes_nothing() {
        let arguments = [positional(0), keyword(1, "x")];
        assert_eq!(
            pair(&arguments, Takers::Class { takes_nothing: true }),
            [U(R::NoParameter), U(R::NoParameter)]
        );
        // A base, a decorator, or a Python `__new__` may supply a constructor.
        assert_eq!(
            pair(&arguments, Takers::Class { takes_nothing: false }),
            [U(R::NoSignature), U(R::NoSignature)]
        );
    }

    #[test]
    fn defaults_and_optionals_are_never_paired() {
        let mut optional = parameter(1, ParameterKind::Positional, "b");
        optional.optional = true;
        let parameters = [parameter(0, ParameterKind::Positional, "a"), optional];
        assert_eq!(pair(&[positional(0)], callable(&parameters, Some(0))), [P(0)]);
    }
}
