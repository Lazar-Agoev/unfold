//! Type definitions and their uses.
//!
//! - **A definition** is an `interface`, `type_alias`, `enum`, or `class` block: its name, place,
//!   written members (each with its own type text, or an enum member's value), and how many type
//!   links resolve to it. A type alias with no members (its aliased type isn't an object type)
//!   also shows the aliased type: the text of its `type` slot from the verified source (no
//!   record stores it), with the type links inside it; absent when the file changed.
//! - **Uses** are one page of the type links that resolve to a declaration, read from the
//!   store's reverse index. Each names its owner: the block the link is sourced at when that is a
//!   definition or type declaration (a parameter's type belongs to its function), else that
//!   block's owner (a variable annotation belongs to its function or file). Its line comes from
//!   the verified source, else the block's first line.

use unfold_core::text::short_label;
use unfold_core::vocab::{BlockKind, SlotName};
use unfold_core::{Block, LineIndex, RepoPath, WrittenType};

use super::facts::Facts;
use super::{TypeDefinition, TypeKind, TypeMember, TypeUsePlace, TypeUses};
use crate::error::Result;

impl TypeKind {
    /// The kind of a type declaration or class block, or `None` for any other block.
    pub fn of(kind: BlockKind) -> Option<Self> {
        match kind {
            BlockKind::Interface => Some(Self::Interface),
            BlockKind::TypeAlias => Some(Self::TypeAlias),
            BlockKind::Enum => Some(Self::Enum),
            BlockKind::Class => Some(Self::Class),
            _ => None,
        }
    }
}

/// The definition of the type declaration or class with `key`; `None` when no block has that
/// key or it is no type declaration.
pub fn type_definition(facts: &mut Facts<'_>, key: &str) -> Result<Option<TypeDefinition>> {
    let Some((file, local)) = facts.locate(key)? else {
        return Ok(None);
    };
    let Some((block, kind)) = file
        .block(local)
        .and_then(|block| TypeKind::of(block.kind).map(|kind| (block, kind)))
    else {
        return Ok(None);
    };
    let members: Vec<TypeMember> = file
        .members(local)
        .iter()
        .map(|member| TypeMember {
            name: member.name.as_deref().map(str::to_owned),
            kind: member.kind,
            optional: member.optional,
            readonly: member.readonly,
            default: member.default,
            ty: member.ty.as_ref().map(|ty| file.type_text(ty)),
            value: member.value.as_deref().map(str::to_owned),
        })
        .collect();
    let aliased = match file.slot(local, SlotName::Type, None) {
        Some(slot) if kind == TypeKind::TypeAlias && members.is_empty() => {
            facts.text(&file).and_then(|snapshot| {
                let text = snapshot.text().get(slot.range.as_usize())?;
                Some(file.type_text(&WrittenType {
                    text: short_label(text).into(),
                    range: slot.range,
                }))
            })
        }
        _ => None,
    };
    let uses = facts.source().type_use_count(key)?;
    Ok(Some(TypeDefinition {
        key: block.key.clone(),
        kind,
        name: block
            .name
            .as_deref()
            .map_or_else(|| file.label(local), str::to_owned),
        path: file.path().clone(),
        line: block.lines.start,
        language: file.graph().file.language,
        members,
        aliased,
        uses: u32::try_from(uses).unwrap_or(u32::MAX),
    }))
}

/// Whether a type link sourced at `block` belongs to the block itself (a definition or type
/// declaration) rather than to its owner.
fn owns_annotations(block: &Block) -> bool {
    block.kind.is_function_like() || block.kind.is_type_declaration()
}

/// One page of the places the type declaration or class with `key` is written, skipping
/// `offset` and listing at most `limit`; `None` when no block has that key or it is no type
/// declaration.
pub fn type_uses(facts: &mut Facts<'_>, key: &str, offset: usize, limit: usize) -> Result<Option<TypeUses>> {
    let declared = facts.locate(key)?.is_some_and(|(file, local)| {
        file.block(local)
            .is_some_and(|block| block.kind.is_type_declaration())
    });
    if !declared {
        return Ok(None);
    }
    let source = facts.source();
    let total = source.type_use_count(key)?;
    let mut places = Vec::new();
    // Sites come by path: index each file's verified lines once.
    let mut lines: Option<(RepoPath, Option<LineIndex>)> = None;
    for site in source.type_uses(key, offset, limit)? {
        let Some(file) = facts.file(&site.path)? else {
            continue;
        };
        let Some(block) = file.block(site.block) else {
            continue;
        };
        let owner = if owns_annotations(block) {
            site.block
        } else {
            file.owner(site.block)
        };
        let Some(owner_block) = file.block(owner) else {
            continue;
        };
        if lines.as_ref().is_none_or(|(path, _)| *path != site.path) {
            let index = facts.text(&file).map(|snapshot| snapshot.line_index());
            lines = Some((site.path.clone(), index));
        }
        let line = lines
            .as_ref()
            .and_then(|(_, index)| index.as_ref())
            .map_or(block.lines.start, |index| index.line_of(site.range.start));
        places.push(TypeUsePlace {
            owner: owner_block.key.clone(),
            label: file.label(owner),
            path: site.path,
            line,
        });
    }
    Ok(Some(TypeUses {
        total: u32::try_from(total).unwrap_or(u32::MAX),
        places,
    }))
}
