//! Resolving manual locators against the current declarations and exports.
//!
//! Locators select explicit declarations or branches, never inferred runtime receivers, so their
//! rules deliberately differ from the value policy:
//!
//! - `path` names the file root (a startup's module initialization).
//! - `path:line` names the outermost permitted block starting on that line.
//! - `path#name` names an export of that name, or else top-level declarations of that name.
//! - `path#Owner.member` names a direct member of one class or constant object literal.
//!
//! A path outside the program is `not_indexed` when the file exists and `not_found` otherwise.

use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use unfold_core::vocab::{DetectorId, Reason, TargetRole};

use super::TargetResolver;
use crate::error::BackendError;
use crate::evidence::{Candidate, Origin};
use crate::labels::location_label;
use crate::locator::{Locator, Selector};
use crate::program::{BlockRef, DeclRef, FactsSet, FileId};
use crate::resolve::{Outcome, Resolver};

/// Resolves `locator` for a target with `role`, using the resolver of the language that owns
/// the locator's file.
pub fn resolve_locator(
    program: &FactsSet,
    resolvers: &[&Resolver<'_>],
    targets: &dyn TargetResolver,
    locator: &Locator,
    role: TargetRole,
) -> Result<Outcome, BackendError> {
    let file = program.find(locator.path().as_str());
    let Some(file) = file.filter(|&file| program.facts(file).is_some()) else {
        let exists = file.is_some() || targets.is_unindexed_file(locator.path());
        return Ok(failure(if exists {
            Reason::NotIndexed
        } else {
            Reason::NotFound
        }));
    };
    let root = BlockRef { file, local: 0 };
    let (owner, member) = match locator.selector() {
        Selector::File => return Ok(resolved(program, root, role)),
        Selector::Line(line) => return Ok(at_line(program, file, *line, role)),
        Selector::Name(name) => (&**name, None),
        Selector::Member { owner, member } => (&**owner, Some(&**member)),
    };
    let Some(resolver) = resolvers.iter().find(|resolver| resolver.owns(file)) else {
        return Err(BackendError::NoLanguage {
            path: locator.path().clone(),
        });
    };
    let linked = resolver.linked();
    let candidates: Vec<Candidate> = match linked
        .module_exports(file)
        .iter()
        .find(|export| &*export.name == owner)
    {
        Some(export) => export.candidates.clone(),
        None => linked
            .locator_top_level(file, owner)
            .into_iter()
            .map(Candidate::indexed)
            .collect(),
    };
    if candidates.is_empty() {
        return Ok(failure(Reason::NotFound));
    }
    let candidates = match member {
        None => candidates,
        Some(member) => match members(resolver, &candidates, member) {
            Ok(members) => members,
            Err(outcome) => return Ok(outcome),
        },
    };
    declarations(resolver, file, locator.text(), &candidates, role)
}

fn failure(reason: Reason) -> Outcome {
    Outcome::unresolved(reason)
}

/// A block permitted for a manual target with `role`, or `unsupported`.
fn resolved(program: &FactsSet, block: BlockRef, role: TargetRole) -> Outcome {
    let permitted = program
        .block(block)
        .is_some_and(|found| role.target_kinds(DetectorId::Manual).contains(&found.kind));
    if permitted {
        Outcome::Resolved(block)
    } else {
        failure(Reason::Unsupported)
    }
}

/// The outermost permitted block starting on `line`.
fn at_line(program: &FactsSet, file: FileId, line: u64, role: TargetRole) -> Outcome {
    let blocks = program.facts(file).map_or(&[][..], |facts| facts.graph.blocks());
    let kinds = role.target_kinds(DetectorId::Manual);
    let on_line: Vec<u32> = blocks
        .iter()
        .filter(|block| u64::from(block.lines.start) == line)
        .map(|block| block.local_id)
        .collect();
    let allowed: FxHashSet<u32> = on_line
        .iter()
        .copied()
        .filter(|&local| kinds.contains(&blocks[local as usize].kind))
        .collect();
    let mut outermost = on_line.iter().copied().filter(|local| {
        allowed.contains(local) && {
            let mut parent = blocks[*local as usize].parent;
            loop {
                match parent {
                    Some(id) if allowed.contains(&id) => break false,
                    Some(id) => parent = blocks.get(id as usize).and_then(|block| block.parent),
                    None => break true,
                }
            }
        }
    });
    match (outermost.next(), outermost.next()) {
        (Some(_), Some(_)) => failure(Reason::Ambiguous),
        (Some(local), None) => resolved(program, BlockRef { file, local }, role),
        (None, _) if on_line.is_empty() => failure(Reason::NotFound),
        (None, _) => failure(Reason::Unsupported),
    }
}

/// The members named `member` of the single declaration `candidates` name.
fn members(
    resolver: &Resolver<'_>,
    candidates: &[Candidate],
    member: &str,
) -> Result<Vec<Candidate>, Outcome> {
    let effective: SmallVec<[&Candidate; 2]> = candidates
        .iter()
        .filter(|candidate| !candidate.augmentation)
        .collect();
    if effective
        .iter()
        .any(|candidate| candidate.is_unindexed_repository())
    {
        return Err(failure(Reason::NotIndexed));
    }
    let mut declarations: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for candidate in &effective {
        match candidate
            .declaration
            .filter(|_| candidate.origin == Origin::Indexed)
        {
            Some(declaration) if !declarations.contains(&declaration) => declarations.push(declaration),
            Some(_) => {}
            None => return Err(failure(Reason::Unsupported)),
        }
    }
    let owner = match declarations.as_slice() {
        [] => return Err(failure(Reason::NotFound)),
        [owner] => *owner,
        _ => return Err(failure(Reason::Ambiguous)),
    };
    let found = resolver.linked().locator_members(owner, member);
    if !found.supported {
        return Err(failure(Reason::Unsupported));
    }
    Ok(found.declarations.into_iter().map(Candidate::indexed).collect())
}

/// One permitted block for a set of candidate declarations.
fn declarations(
    resolver: &Resolver<'_>,
    file: FileId,
    text: &str,
    candidates: &[Candidate],
    role: TargetRole,
) -> Result<Outcome, BackendError> {
    let program = resolver.program();
    let linked = resolver.linked();
    if candidates.is_empty() {
        return Ok(failure(Reason::NotFound));
    }
    let values: SmallVec<[&Candidate; 2]> = candidates
        .iter()
        .filter(|candidate| !candidate.augmentation)
        .collect();
    let effective: SmallVec<[&Candidate; 2]> = if values.is_empty() {
        candidates.iter().collect()
    } else {
        values.clone()
    };
    if effective
        .iter()
        .any(|candidate| candidate.is_unindexed_repository())
    {
        return Ok(failure(Reason::NotIndexed));
    }
    if effective
        .iter()
        .all(|candidate| candidate.origin == Origin::External)
    {
        if role == TargetRole::Startup {
            return Ok(failure(Reason::Unsupported));
        }
        let label = location_label(program, linked, file, text, effective.iter().copied())?;
        return Ok(Outcome::External(label));
    }
    if effective
        .iter()
        .any(|candidate| candidate.origin != Origin::Indexed)
    {
        return Ok(failure(Reason::Ambiguous));
    }
    let mut declarations: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for candidate in &values {
        match candidate.declaration {
            Some(declaration) if !declarations.contains(&declaration) => declarations.push(declaration),
            Some(_) => {}
            None => return Ok(failure(Reason::Unsupported)),
        }
    }
    let mut targets: SmallVec<[(BlockRef, DeclRef); 2]> = SmallVec::new();
    let mut unsuitable: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for &declaration in &declarations {
        match linked
            .locator_concrete(declaration)
            .filter(|&block| program.block(block).is_some())
        {
            Some(block) => match targets.iter_mut().find(|(found, _)| *found == block) {
                Some(entry) => entry.1 = declaration,
                None => targets.push((block, declaration)),
            },
            None => unsuitable.push(declaration),
        }
    }
    let (block, implementation) = match targets.as_slice() {
        [target] => *target,
        [] if declarations.len() > 1 => return Ok(failure(Reason::Ambiguous)),
        [] => return Ok(failure(Reason::Unsupported)),
        _ => return Ok(failure(Reason::Ambiguous)),
    };
    if unsuitable
        .iter()
        .any(|&declaration| !linked.locator_overload_of(declaration, implementation))
    {
        return Ok(failure(Reason::Ambiguous));
    }
    Ok(resolved(program, block, role))
}
