//! The override census: which instance members a class's indexed descendants can override.
//!
//! Built once per link from the language's class shapes. For every class it records the names
//! of the overriding members declared anywhere among its proper descendants (through every
//! declared base, so multiple inheritance participates), plus whether any of them has an unknown
//! computed name, which could override anything.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::evidence::Evidence;
use crate::language::Linked;
use crate::program::DeclRef;

#[derive(Debug, Default)]
struct Overriders {
    /// Some descendant declares an overriding member whose name isn't known.
    unnamed: bool,
    /// Names of overriding members declared by descendants.
    names: FxHashSet<Box<str>>,
}

/// Overriding members per class, and every class member with its static flag.
#[derive(Debug, Default)]
pub(crate) struct OverrideCensus {
    /// `(owner, member)` for every listed class member, mapped to whether it is static.
    members: FxHashMap<(DeclRef, DeclRef), bool>,
    overriders: FxHashMap<DeclRef, Overriders>,
}

impl OverrideCensus {
    pub(crate) fn new(linked: &dyn Linked) -> Self {
        let classes = linked.classes();
        let by_class: FxHashMap<DeclRef, usize> = classes
            .iter()
            .enumerate()
            .map(|(index, shape)| (shape.class, index))
            .collect();
        let mut census = Self::default();
        let mut pending = Vec::new();
        let mut visited = FxHashSet::default();
        for shape in classes {
            let mut overriding: Vec<Option<&str>> = Vec::new();
            for member in &shape.members {
                census
                    .members
                    .insert((shape.class, member.declaration), member.is_static);
                if member.is_static {
                    continue;
                }
                let facts = linked.member(member.declaration);
                if facts.can_override {
                    overriding.push(facts.name);
                }
            }
            if overriding.is_empty() {
                continue;
            }
            // Every proper ancestor reachable through declared bases gains these overriders.
            pending.clear();
            pending.extend(shape.bases.iter().rev().copied());
            visited.clear();
            while let Some(base) = pending.pop() {
                if !visited.insert(base) {
                    continue;
                }
                if base != shape.class {
                    let entry = census.overriders.entry(base).or_default();
                    for name in &overriding {
                        match name {
                            Some(name) => {
                                entry.names.insert((*name).into());
                            }
                            None => entry.unnamed = true,
                        }
                    }
                }
                if let Some(&index) = by_class.get(&base) {
                    pending.extend(classes[index].bases.iter().rev().copied());
                }
            }
        }
        census
    }

    /// Whether a call through `evidence` to member `declaration` can reach a different
    /// implementation in an indexed descendant of the receiver's class.
    pub(crate) fn is_overridden(
        &self,
        linked: &dyn Linked,
        declaration: DeclRef,
        evidence: &Evidence,
    ) -> bool {
        let facts = linked.member(declaration);
        if evidence.super_access || evidence.static_access || facts.is_static {
            return false;
        }
        let Some(owner) = facts.owner else {
            return false;
        };
        // Only a listed, non-static member of its owner takes part.
        if self.members.get(&(owner, declaration)) != Some(&false) || facts.is_private {
            return false;
        }
        let receiver = evidence.receiver_class.unwrap_or(owner);
        let Some(overriders) = self.overriders.get(&receiver) else {
            return false;
        };
        match facts.name {
            None => overriders.unnamed || !overriders.names.is_empty(),
            Some(name) => overriders.unnamed || overriders.names.contains(name),
        }
    }
}
