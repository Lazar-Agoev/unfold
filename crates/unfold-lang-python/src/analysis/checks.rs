//! Compile-time errors the proof of concept's grammar (Pyright's parser) reported but Ruff's
//! parser leaves to later passes. CPython rejects all of them when it compiles a module, so a
//! file that has one fails, as it did in the proof of concept:
//!
//! - `return` outside a function; `break` or `continue` outside a loop;
//! - `return`, `break`, or `continue` leaving an `except*` handler;
//! - with grammar 3.14, `return`, `break`, or `continue` leaving a `finally` block (PEP 765; the
//!   proof of concept's grammar rejected these, though CPython 3.14 only warns);
//! - a duplicate parameter name;
//! - a bare `except:` that is not the last handler;
//! - in `match`: an irrefutable case before the last one, an irrefutable alternative before
//!   the last one in an or-pattern, a name bound twice in one pattern, alternatives that bind
//!   different names, and two starred names in one sequence pattern.

use ruff_python_ast::{ExceptHandler, MatchCase, Parameters, Pattern};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashSet;
use unfold_engine::SourceFailure;

use crate::parse::failure;

/// The enclosing constructs a statement is checked against.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Flags {
    /// Inside a function body.
    pub(crate) function: bool,
    /// Inside a loop body (not its `else`).
    pub(crate) in_loop: bool,
    /// Inside an `except*` handler, not inside a loop or function nested in it.
    pub(crate) except_group: bool,
    /// Inside a `finally` block of the current function.
    pub(crate) finally_block: bool,
    /// Inside a `finally` block whose nearest enclosing loop is outside it.
    pub(crate) finally_loop: bool,
}

impl Flags {
    /// The flags inside a function body.
    pub(crate) const FUNCTION: Self = Self {
        function: true,
        in_loop: false,
        except_group: false,
        finally_block: false,
        finally_loop: false,
    };

    /// The flags inside a loop body.
    pub(crate) fn enter_loop(self) -> Self {
        Self {
            in_loop: true,
            except_group: false,
            finally_loop: false,
            ..self
        }
    }

    /// The flags inside a `finally` block.
    pub(crate) fn enter_finally(self) -> Self {
        Self {
            finally_block: true,
            finally_loop: self.in_loop,
            ..self
        }
    }
}

/// Which control-flow transfer is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transfer {
    Return,
    Break,
    Continue,
}

/// Checks a `return`, `break`, or `continue` at `at` against its enclosing constructs.
pub(crate) fn transfer(
    flags: Flags,
    transfer: Transfer,
    at: TextSize,
    pep765: bool,
) -> Result<(), SourceFailure> {
    let word = match transfer {
        Transfer::Return => "return",
        Transfer::Break => "break",
        Transfer::Continue => "continue",
    };
    let message = match transfer {
        Transfer::Return if !flags.function => Some(format!("'{word}' outside function")),
        Transfer::Break | Transfer::Continue if !flags.in_loop => Some(format!("'{word}' outside loop")),
        _ if flags.except_group => Some(format!("'{word}' cannot appear in an except* block")),
        Transfer::Return if pep765 && flags.finally_block => {
            Some(format!("'{word}' cannot exit a 'finally' block"))
        }
        Transfer::Break | Transfer::Continue if pep765 && flags.finally_loop => {
            Some(format!("'{word}' cannot exit a 'finally' block"))
        }
        _ => None,
    };
    message.map_or(Ok(()), |message| Err(failure(message, Some(at))))
}

/// Fails on a parameter name written twice.
pub(crate) fn parameters(parameters: &Parameters) -> Result<(), SourceFailure> {
    let mut seen = FxHashSet::default();
    for parameter in parameters {
        let name = parameter.name();
        if !seen.insert(name.id.as_str()) {
            return Err(failure(
                format!("duplicate argument '{}' in function definition", name.id),
                Some(name.range.start()),
            ));
        }
    }
    Ok(())
}

/// Fails on a bare `except:` before the last handler.
pub(crate) fn handlers(handlers: &[ExceptHandler]) -> Result<(), SourceFailure> {
    let last = handlers.len().saturating_sub(1);
    for (index, ExceptHandler::ExceptHandler(handler)) in handlers.iter().enumerate() {
        if handler.type_.is_none() && index < last {
            return Err(failure(
                "default 'except:' must be last",
                Some(handler.range.start()),
            ));
        }
    }
    Ok(())
}

/// Checks a `match` statement's cases and patterns. Returns the deepest pattern nesting seen.
pub(crate) fn cases(cases: &[MatchCase]) -> Result<usize, SourceFailure> {
    let last = cases.len().saturating_sub(1);
    let mut depth = 0;
    for (index, case) in cases.iter().enumerate() {
        if index < last && case.guard.is_none() && irrefutable(&case.pattern) {
            return Err(failure(
                "an irrefutable pattern is allowed only in the last case",
                Some(case.pattern.start()),
            ));
        }
        depth = depth.max(pattern(&case.pattern)?);
    }
    Ok(depth)
}

/// Whether a pattern always matches: a capture or wildcard, possibly named or alternated.
fn irrefutable(pattern: &Pattern) -> bool {
    let mut stack = vec![pattern];
    while let Some(pattern) = stack.pop() {
        match pattern {
            Pattern::MatchAs(node) => match &node.pattern {
                None => return true,
                Some(inner) => stack.push(inner),
            },
            Pattern::MatchOr(node) => stack.extend(&node.patterns),
            _ => {}
        }
    }
    false
}

/// The direct sub-patterns of a pattern.
fn children(pattern: &Pattern) -> Vec<&Pattern> {
    match pattern {
        Pattern::MatchSequence(node) => node.patterns.iter().collect(),
        Pattern::MatchMapping(node) => node.patterns.iter().collect(),
        Pattern::MatchClass(node) => node
            .arguments
            .patterns
            .iter()
            .chain(node.arguments.keywords.iter().map(|keyword| &keyword.pattern))
            .collect(),
        Pattern::MatchAs(node) => node.pattern.as_deref().into_iter().collect(),
        Pattern::MatchOr(node) => node.patterns.iter().collect(),
        Pattern::MatchValue(_) | Pattern::MatchSingleton(_) | Pattern::MatchStar(_) => Vec::new(),
    }
}

/// The name a pattern node itself binds, if any.
fn own_name(pattern: &Pattern) -> Option<(&str, TextRange)> {
    let identifier = match pattern {
        Pattern::MatchAs(node) => node.name.as_ref(),
        Pattern::MatchStar(node) => node.name.as_ref(),
        Pattern::MatchMapping(node) => node.rest.as_ref(),
        _ => None,
    }?;
    Some((identifier.id.as_str(), identifier.range))
}

/// The names a pattern binds, in source order. An or-pattern binds what its first alternative
/// binds (alternatives are checked to agree separately).
fn bound_names(pattern: &Pattern) -> Vec<(&str, TextRange)> {
    let mut names = Vec::new();
    let mut stack = vec![pattern];
    while let Some(pattern) = stack.pop() {
        names.extend(own_name(pattern));
        match pattern {
            Pattern::MatchOr(node) => stack.extend(node.patterns.first()),
            _ => stack.extend(children(pattern).into_iter().rev()),
        }
    }
    names.sort_by_key(|(_, range)| range.start());
    names
}

/// Fails on a name bound twice in `names`.
fn distinct(names: &[(&str, TextRange)]) -> Result<(), SourceFailure> {
    let mut seen = FxHashSet::default();
    for (name, range) in names {
        if !seen.insert(*name) {
            return Err(failure(
                format!("multiple assignments to name '{name}' in pattern"),
                Some(range.start()),
            ));
        }
    }
    Ok(())
}

/// Checks one case pattern and returns its nesting depth.
fn pattern(root: &Pattern) -> Result<usize, SourceFailure> {
    distinct(&bound_names(root))?;
    let mut depth = 0;
    let mut stack = vec![(root, 1usize)];
    while let Some((pattern, level)) = stack.pop() {
        depth = depth.max(level);
        match pattern {
            Pattern::MatchSequence(node) => {
                let mut stars = node
                    .patterns
                    .iter()
                    .filter(|p| matches!(p, Pattern::MatchStar(_)));
                if let (Some(_), Some(second)) = (stars.next(), stars.next()) {
                    return Err(failure(
                        "multiple starred names in sequence pattern",
                        Some(second.start()),
                    ));
                }
            }
            Pattern::MatchOr(node) => {
                let last = node.patterns.len().saturating_sub(1);
                let mut expected: Option<FxHashSet<&str>> = None;
                for (index, alternative) in node.patterns.iter().enumerate() {
                    if index < last && irrefutable(alternative) {
                        return Err(failure(
                            "an irrefutable alternative makes the remaining patterns unreachable",
                            Some(alternative.start()),
                        ));
                    }
                    let names = bound_names(alternative);
                    distinct(&names)?;
                    let names: FxHashSet<&str> = names.into_iter().map(|(name, _)| name).collect();
                    match &expected {
                        None => expected = Some(names),
                        Some(first) if *first != names => {
                            return Err(failure(
                                "alternative patterns bind different names",
                                Some(alternative.start()),
                            ));
                        }
                        Some(_) => {}
                    }
                }
            }
            _ => {}
        }
        stack.extend(children(pattern).into_iter().map(|child| (child, level + 1)));
    }
    Ok(depth)
}
