//! Classes and types, and where they are named.
//!
//! - **A definition** is a class or a type (`interface`, `type_alias`, `enum`): its name, place,
//!   written members (each with its own type text, or an enum member's value), and how many
//!   `types` and `extends` links name it. A type alias with no members (its aliased type isn't an
//!   object type) also shows the aliased type: the text of its `body` from the verified source,
//!   with the type names inside it; absent when the file changed.
//! - **Inherited members** follow the `extends` links that name a base the definition extends
//!   (not `implements`, whose members a class writes itself), depth first in written order: each
//!   base is a group with its members, then the groups of its own bases. Each type is listed once
//!   (a cycle or a diamond stops at a type already listed, the definition itself included), and
//!   bases more than [`MAX_INHERITED_DEPTH`] levels down are left out. A library, builtin, or
//!   unknown base is a group without members, saying where it comes from or why it has no
//!   definition. Members are as written: generic arguments aren't substituted, and a member a type
//!   writes again isn't hidden.
//! - **Uses** are one page of the `types` and `extends` links that reach a class or type, from the
//!   store's reverse index. Each names its owner: the definition the link starts at, or the
//!   definition an `except` part belongs to. Its line comes from the verified source, else the
//!   owner's first line.

use std::path::Path;

use rustc_hash::FxHashSet;
use unfold_core::text::short_label;
use unfold_core::vocab::{Form, LinkKind, NodeKind};
use unfold_core::{Key, LinkTarget, WrittenType};
use unfold_store::{Cursor, Reader};

use super::facts::Facts;
use super::text::{type_text, whole_text};
use super::{InheritedGroup, TypeDefinition, TypeKind, TypeMember, TypeUsePlace, TypeUses, UseKind};
use crate::error::Result;
use crate::tree::FileTree;

/// How many levels of bases a definition's inherited members reach.
pub const MAX_INHERITED_DEPTH: u32 = 8;

impl TypeKind {
    /// The kind of a class or type of `form`; `None` for any other form.
    pub fn of(form: Form) -> Option<Self> {
        match form {
            Form::Interface => Some(Self::Interface),
            Form::TypeAlias => Some(Self::TypeAlias),
            Form::Enum => Some(Self::Enum),
            Form::Class => Some(Self::Class),
            _ => None,
        }
    }
}

/// Whether node `id` of `file` is a class or a type.
fn is_type(file: &FileTree, id: u32) -> bool {
    matches!(file.kind(id), NodeKind::Class | NodeKind::Type)
}

/// The definition of the class or type with `key`; `None` when no class or type has that key.
pub fn type_definition(facts: &mut Facts<'_>, key: &str) -> Result<Option<TypeDefinition>> {
    let Some((file, id)) = facts.locate(key)? else {
        return Ok(None);
    };
    let node = file.node(id);
    let Some(kind) = TypeKind::of(node.form) else {
        return Ok(None);
    };
    let members = members(&file, id);
    let aliased = match node.body {
        Some(body) if kind == TypeKind::TypeAlias && members.is_empty() => {
            facts.text(&file).and_then(|source| {
                let text = source.slice(body).ok()?;
                let written = WrittenType {
                    text: short_label(text).into(),
                    range: body,
                };
                Some(type_text(&written, file.type_links_in(body)))
            })
        }
        _ => None,
    };
    let mut inherited = Vec::new();
    let mut listed: FxHashSet<Key> = FxHashSet::default();
    listed.insert(node.key.clone());
    inherit(facts, &file, id, 1, &mut listed, &mut inherited)?;
    Ok(Some(TypeDefinition {
        key: node.key.clone(),
        kind,
        name: file.label(id),
        path: file.path().clone(),
        line: node.lines.start,
        language: file.language(),
        members,
        inherited,
        aliased,
        uses: facts.source().type_use_count(key)?,
    }))
}

/// The written members of class or type `id` of `file`.
fn members(file: &FileTree, id: u32) -> Vec<TypeMember> {
    file.members(id)
        .iter()
        .map(|member| TypeMember {
            name: member.name.as_deref().map(str::to_owned),
            kind: member.kind,
            optional: member.optional,
            readonly: member.readonly,
            default: member.default,
            ty: member
                .ty
                .as_ref()
                .map(|ty| type_text(ty, file.type_links_in(ty.range))),
            value: member.value.as_deref().map(str::to_owned),
        })
        .collect()
}

/// Adds a group for each base that `id` of `file` extends (at `depth`, 1 for the definition's
/// own bases), each followed by its own bases' groups. `listed` holds the project types already
/// listed.
fn inherit(
    facts: &mut Facts<'_>,
    file: &FileTree,
    id: u32,
    depth: u32,
    listed: &mut FxHashSet<Key>,
    groups: &mut Vec<InheritedGroup>,
) -> Result<()> {
    let bases = file
        .links(id)
        .iter()
        .filter(|link| link.kind == LinkKind::Extends && !link.implements);
    for link in bases {
        let from = whole_text(&link.text, &link.to);
        let LinkTarget::Project(target) = &link.to else {
            if !groups
                .iter()
                .any(|group| group.from == from && group.members.is_empty())
            {
                groups.push(InheritedGroup {
                    from,
                    members: Vec::new(),
                });
            }
            continue;
        };
        if !listed.insert(target.clone()) {
            continue;
        }
        let base = facts
            .locate(target.as_str())?
            .filter(|(base, at)| is_type(base, *at));
        groups.push(InheritedGroup {
            from,
            members: base
                .as_ref()
                .map_or_else(Vec::new, |(base, at)| members(base, *at)),
        });
        if let Some((base, at)) = base
            && depth < MAX_INHERITED_DEPTH
        {
            inherit(facts, &base, at, depth + 1, listed, groups)?;
        }
    }
    Ok(())
}

/// One page of the places the class or type with `key` is named, after `after` (the previous
/// page's `next`), at most `limit`, with sources verified under `root`; `None` when no class or
/// type has that key.
pub fn type_uses(
    reader: &Reader,
    root: &Path,
    key: &str,
    after: Option<&Cursor>,
    limit: usize,
) -> Result<Option<TypeUses>> {
    reader.snapshot(|reader| Ok(uses(reader, root, key, after, limit)))?
}

fn uses(
    reader: &Reader,
    root: &Path,
    key: &str,
    after: Option<&Cursor>,
    limit: usize,
) -> Result<Option<TypeUses>> {
    let mut facts = Facts::new(reader, root);
    if !facts.locate(key)?.is_some_and(|(file, id)| is_type(&file, id)) {
        return Ok(None);
    }
    let page = reader.type_uses(key, after, limit)?;
    let mut places = Vec::with_capacity(page.items.len());
    for incoming in page.items {
        let Some(file) = facts.file(&incoming.path)? else {
            continue;
        };
        let from = incoming.link.from;
        let (owner, kind) = match incoming.link.kind {
            LinkKind::Extends if incoming.link.implements => (Some(from), UseKind::Implements),
            LinkKind::Extends => (Some(from), UseKind::Extends),
            _ if file.kind(from).is_definition() => (Some(from), UseKind::Annotation),
            _ => (file.owner(from), UseKind::Handler),
        };
        let Some(owner) = owner else {
            continue;
        };
        let line = match facts.text(&file) {
            Some(source) => source.lines().line_of(incoming.link.range.start),
            None => file.node(owner).lines.start,
        };
        places.push(TypeUsePlace {
            owner: file.key(owner).clone(),
            label: file.label(owner),
            path: incoming.path,
            line,
            kind,
        });
    }
    Ok(Some(TypeUses {
        total: reader.type_use_count(key)?,
        places,
        next: page.next,
    }))
}
