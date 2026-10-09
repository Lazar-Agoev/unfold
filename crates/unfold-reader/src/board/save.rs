//! Authoring and saving: writing descriptions and labels signed against verified source, and
//! preparing a board for writing (signing what is unsigned, saving guards, and refusing new
//! problems).
//!
//! A signature is only ever computed from bytes that verified against the facts' hash, so text
//! can never be signed against source the graph has not seen.

use super::BoardError;
use super::check::{Health, HealthKind, Target, current_guard, group_problem, guard_problem, health};
use super::graph::BoardGraph;
use super::model::{Author, Board, Description, Signature};

/// The current signature of a card's reference, a group's span, or a labeled block.
pub(crate) fn current_signature(
    board: &Board,
    graph: &BoardGraph,
    target: &Target,
) -> Result<Signature, BoardError> {
    let unverified = |what: &str| BoardError::Unverified {
        what: what.to_owned(),
    };
    let refuse = |message: &str| BoardError::Problem {
        target: target.clone(),
        message: message.to_owned(),
    };
    match target {
        Target::Card(id) => {
            let card = board.card(id).ok_or_else(|| refuse("unknown card"))?;
            let key = card.reference.key().as_str();
            if let Some(problem) = guard_problem(board, graph, key) {
                return Err(refuse(problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
        Target::Group(id) => {
            let group = board.group(id).ok_or_else(|| refuse("unknown group"))?;
            if let Some(problem) = group_problem(board, graph, group) {
                return Err(refuse(problem));
            }
            if let Some(problem) = group
                .members
                .iter()
                .find_map(|key| guard_problem(board, graph, key.as_str()))
            {
                return Err(refuse(problem));
            }
            let (Some(first), Some(last)) = (group.members.first(), group.members.last()) else {
                return Err(refuse("has no members"));
            };
            graph
                .span_signature(first.as_str(), last.as_str())
                .ok_or_else(|| unverified(id))
        }
        Target::Label(key) => {
            if let Some(problem) = guard_problem(board, graph, key) {
                return Err(refuse(problem));
            }
            graph.signature(key).ok_or_else(|| unverified(key))
        }
    }
}

fn authored(text: &str, author: &Author, signature: Signature) -> Option<Description> {
    (!text.trim().is_empty()).then(|| Description {
        text: text.to_owned(),
        author: author.clone(),
        signature: Some(signature),
    })
}

/// Describes card `id`; blank text removes the description.
pub fn describe_card(
    board: &mut Board,
    graph: &BoardGraph,
    id: &str,
    text: &str,
    author: &Author,
) -> Result<(), BoardError> {
    let description = if text.trim().is_empty() {
        None
    } else {
        authored(
            text,
            author,
            current_signature(board, graph, &Target::Card(id.to_owned()))?,
        )
    };
    let card = board
        .cards
        .iter_mut()
        .find(|card| card.id == id)
        .ok_or_else(|| BoardError::Problem {
            target: Target::Card(id.to_owned()),
            message: "unknown card".to_owned(),
        })?;
    card.description = description;
    Ok(())
}

/// Describes group `id`; blank text removes the description.
pub fn describe_group(
    board: &mut Board,
    graph: &BoardGraph,
    id: &str,
    text: &str,
    author: &Author,
) -> Result<(), BoardError> {
    let description = if text.trim().is_empty() {
        None
    } else {
        authored(
            text,
            author,
            current_signature(board, graph, &Target::Group(id.to_owned()))?,
        )
    };
    let group = board
        .groups
        .iter_mut()
        .find(|group| group.id == id)
        .ok_or_else(|| BoardError::Problem {
            target: Target::Group(id.to_owned()),
            message: "unknown group".to_owned(),
        })?;
    group.description = description;
    Ok(())
}

/// Labels the Flow node of block `key`, which must lie inside one of the board's block cards
/// (not be the card itself); blank text removes the label.
pub fn set_label(
    board: &mut Board,
    graph: &BoardGraph,
    key: &str,
    text: &str,
    author: &Author,
) -> Result<(), BoardError> {
    let target = Target::Label(key.to_owned());
    let refuse = |message: &str| BoardError::Problem {
        target: target.clone(),
        message: message.to_owned(),
    };
    let block = graph.block(key).ok_or_else(|| refuse("unknown block"))?;
    let contained = board.cards.iter().any(|card| {
        card.reference
            .block()
            .and_then(|card_key| graph.block(card_key.as_str()))
            .is_some_and(|card| block.within(&card) && block.local() != card.local())
    });
    if !contained {
        return Err(refuse("no block card on this board contains it"));
    }
    if text.trim().is_empty() {
        board.labels.remove(key);
        return Ok(());
    }
    let signature = current_signature(board, graph, &target)?;
    if let Some(label) = authored(text, author, signature) {
        board.labels.insert(key.to_owned(), label);
    }
    Ok(())
}

/// Signs every unsigned description and label from verified source, keeps the guards of
/// references still on the board (adding fresh ones for new references), and drops the rest.
pub fn sign(board: &mut Board, graph: &BoardGraph) -> Result<(), BoardError> {
    let mut signatures: Vec<(Target, Signature)> = Vec::new();
    let unsigned = board
        .cards
        .iter()
        .filter(|card| {
            card.description
                .as_ref()
                .is_some_and(|item| item.signature.is_none())
        })
        .map(|card| Target::Card(card.id.clone()))
        .chain(
            board
                .groups
                .iter()
                .filter(|group| {
                    group
                        .description
                        .as_ref()
                        .is_some_and(|item| item.signature.is_none())
                })
                .map(|group| Target::Group(group.id.clone())),
        )
        .chain(
            board
                .labels
                .iter()
                .filter(|(_, label)| label.signature.is_none())
                .map(|(key, _)| Target::Label(key.clone())),
        );
    for target in unsigned {
        let signature = current_signature(board, graph, &target)?;
        signatures.push((target, signature));
    }
    for (target, signature) in signatures {
        let description = match &target {
            Target::Card(id) => board
                .cards
                .iter_mut()
                .find(|card| &card.id == id)
                .and_then(|card| card.description.as_mut()),
            Target::Group(id) => board
                .groups
                .iter_mut()
                .find(|group| &group.id == id)
                .and_then(|group| group.description.as_mut()),
            Target::Label(key) => board.labels.get_mut(key),
        };
        if let Some(description) = description {
            description.signature = Some(signature);
        }
    }
    let references: Vec<String> = board.references().into_iter().map(str::to_owned).collect();
    let kept: rustc_hash::FxHashSet<&str> = references.iter().map(String::as_str).collect();
    board.guards.retain(|key, _| kept.contains(key.as_str()));
    for key in references {
        if !board.guards.contains_key(&key)
            && let Some(guard) = current_guard(graph, &key)
        {
            board.guards.insert(key, guard);
        }
    }
    Ok(())
}

/// Prepares `board` for writing over `previous` (the version being replaced, if any): guards
/// the new version dropped come back from `previous`, unsigned text is signed, guards are
/// saved, and the result is refused when it introduces a missing reference, a broken group,
/// or a reference held for review that `previous` did not already have (with the same
/// reference). Problems carried over unchanged are kept, so a board with a placeholder can
/// still be moved, retitled, or described elsewhere.
pub fn prepare_save(
    mut board: Board,
    previous: Option<(&Board, &Health)>,
    graph: &BoardGraph,
) -> Result<(Board, Health), BoardError> {
    if let Some((old, _)) = previous {
        for (key, guard) in &old.guards {
            board.guards.entry(key.clone()).or_insert_with(|| guard.clone());
        }
    }
    sign(&mut board, graph)?;
    let health = health(&board, graph);
    for item in health
        .items
        .iter()
        .filter(|item| item.kind != HealthKind::Outdated)
    {
        let carried = previous.is_some_and(|(old, old_health)| {
            let same_problem = old_health.items.iter().any(|known| {
                known.kind == item.kind && known.target == item.target && known.message == item.message
            });
            let unchanged = match &item.target {
                Target::Card(id) => {
                    old.card(id).map(|card| &card.reference) == board.card(id).map(|card| &card.reference)
                }
                Target::Group(id) => match (old.group(id), board.group(id)) {
                    (Some(a), Some(b)) => a.card == b.card && a.members == b.members,
                    _ => false,
                },
                Target::Label(key) => old.labels.get(key) == board.labels.get(key),
            };
            same_problem && unchanged
        });
        if !carried {
            return Err(BoardError::Problem {
                target: item.target.clone(),
                message: item.message.clone(),
            });
        }
    }
    Ok((board, health))
}
