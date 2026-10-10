//! `relations` and `callers` (architecture §10.1, decision U-29): what the
//! Selected section shows for one definition (its callers, its outgoing calls grouped on the
//! page / openable / external or unresolved, and the entrances it is reached from), and **Show
//! callers**: the distinct definitions that call it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{EdgeKind, EntranceKind, HttpMethod};
use unfold_core::{Entrance, Key};
use unfold_reader::ReaderIndex;
use unfold_reader::mermaid::entrance_title;
use unfold_reader::relations::{Outgoing, outgoing};
use unfold_store::Reader;

use super::graph::locate;
use super::{count, key_arg};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// The most callers one page holds.
pub const MAX_CALLERS: u32 = 500;
/// Callers per page when the request doesn't say.
const DEFAULT_CALLERS: u32 = 100;
/// The most keys `onPage` may list.
pub const MAX_ON_PAGE: usize = 2_000;
/// Calling definitions `callers` lists when the request doesn't say.
const DEFAULT_CALLING: u32 = 20;
/// The most calling definitions `callers` lists.
pub const MAX_CALLING: u32 = 100;

/// The arguments of `relations`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationsArgs {
    /// The definition's project.
    pub project: ProjectId,
    /// The definition's (or any block's) key.
    pub key: String,
    /// The keys of this project's cards on the page (at most 2,000), which group outgoing calls
    /// as "on the page".
    #[serde(default)]
    #[ts(optional)]
    pub on_page: Option<Vec<String>>,
    /// Where the page of callers starts (`callersNext` of the previous page).
    #[serde(default)]
    #[ts(optional)]
    pub callers_offset: Option<u32>,
    /// How many callers to return (at most 500; 100 when absent).
    #[serde(default)]
    #[ts(optional)]
    pub callers_limit: Option<u32>,
}

/// One resolved edge into the definition, seen from its caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CallerView {
    /// The definition (or file) that owns the calling occurrence.
    pub owner: String,
    /// The owner's qualified name, or its file's path for a file-level call.
    pub owner_name: String,
    /// The occurrence block.
    pub source: String,
    /// The occurrence's file.
    pub path: String,
    /// The occurrence's first line.
    pub line: u32,
    /// The relationship.
    pub kind: EdgeKind,
    /// The argument position of a passed reference.
    pub position: Option<u32>,
}

/// An entrance a definition is reached from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceRef {
    /// The entrance's key.
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub kind: EntranceKind,
    /// Its HTTP method.
    pub method: Option<HttpMethod>,
    /// Its trigger.
    pub trigger: Option<String>,
    /// Its evidence file.
    pub path: String,
}

/// A definition's relations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Relations {
    /// The definition.
    pub key: String,
    /// One page of callers, in graph order.
    pub callers: Vec<CallerView>,
    /// How many callers there are in all.
    pub callers_total: u32,
    /// Where the next page of callers starts, if there is one.
    pub callers_next: Option<u32>,
    /// The edges the definition owns, in source order, grouped.
    pub outgoing: Outgoing,
    /// The qualified names of the resolved outgoing targets, by key.
    pub target_names: BTreeMap<String, String>,
    /// The non-process entrances whose closures contain the definition ("reached at least
    /// from"), in canonical order.
    pub reached_from: Vec<EntranceRef>,
}

/// `relations`: callers (paginated), outgoing calls, and reached-from for a block.
pub fn relations(ctx: &Ctx<'_>, args: RelationsArgs) -> CommandResult<Relations> {
    let key = key_arg(&args.key)?;
    if key.starts_with("entrance:") {
        return Err(CommandError::invalid_args(
            "`relations` takes a block key; an entrance's targets are on its card",
        ));
    }
    let on_page = args.on_page.unwrap_or_default();
    if on_page.len() > MAX_ON_PAGE {
        return Err(CommandError::invalid_args(format!(
            "onPage lists at most {MAX_ON_PAGE} keys"
        )));
    }
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let index = workspace.reader_index()?;
    let (file, local) = locate(&reader, key)?;
    let on_page: BTreeSet<&str> = on_page.iter().map(String::as_str).collect();
    let grouped = outgoing(&file, local, |target| on_page.contains(target)).unwrap_or_default();
    let mut target_names = BTreeMap::new();
    for call in grouped.on_board.iter().chain(&grouped.openable) {
        if let Some(target) = &call.target
            && let Some(definition) = reader.definition(target.as_str())?
        {
            target_names.insert(target.as_str().to_owned(), definition.qualified_name.to_string());
        }
    }
    ctx.check_cancelled()?;
    let all = index.callers(key).unwrap_or_default();
    let offset = args.callers_offset.unwrap_or(0) as usize;
    let limit = args
        .callers_limit
        .unwrap_or(DEFAULT_CALLERS)
        .clamp(1, MAX_CALLERS) as usize;
    let mut callers = Vec::new();
    for caller in all.iter().skip(offset).take(limit) {
        let line = reader
            .block(caller.source.as_str())?
            .map_or(0, |located| located.block.lines.start);
        callers.push(CallerView {
            owner: caller.owner.as_str().to_owned(),
            owner_name: owner_name(&reader, caller.owner.as_str(), caller.path.as_str())?,
            source: caller.source.as_str().to_owned(),
            path: caller.path.as_str().to_owned(),
            line,
            kind: caller.kind,
            position: caller.position,
        });
    }
    let next = offset + callers.len();
    ctx.check_cancelled()?;
    let reached = index.reached_from(key).unwrap_or_default();
    let mut reached_from = Vec::with_capacity(reached.len());
    for entrance_key in &reached {
        if let Some(entrance) = reader.entrance(entrance_key.as_str())? {
            reached_from.push(entrance_ref(&entrance));
        }
    }
    Ok(Relations {
        key: key.to_owned(),
        callers_total: count(all.len()),
        callers_next: (next < all.len()).then(|| count(next)),
        callers,
        outgoing: grouped,
        target_names,
        reached_from,
    })
}

/// The qualified name of a caller's owner: a definition's, or the file's path for a file root.
fn owner_name(reader: &Reader, owner: &str, path: &str) -> CommandResult<String> {
    Ok(match reader.definition(owner)? {
        Some(definition) => definition.qualified_name.to_string(),
        None => path.to_owned(),
    })
}

/// An entrance as "reached from" lists it.
pub(crate) fn entrance_ref(entrance: &Entrance) -> EntranceRef {
    EntranceRef {
        key: entrance.key.as_str().to_owned(),
        title: entrance_title(entrance),
        kind: entrance.kind,
        method: entrance.method,
        trigger: entrance.trigger.as_deref().map(str::to_owned),
        path: entrance.evidence.path.as_str().to_owned(),
    }
}

/// One definition (or file) that calls a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CallingDefinition {
    /// Its key.
    pub key: String,
    /// Its display label (its file's path for a file-level call).
    pub name: String,
    /// Its qualified name (its file's path for a file-level call).
    pub qualified_name: String,
    /// Its file.
    pub path: String,
    /// Its first line.
    pub line: u32,
    /// How many of its occurrences call the block.
    pub sites: u32,
}

/// The distinct definitions that call block `key`, by path and position, with their call sites
/// counted: how many there are, and the first `limit` (only those are looked up in the store).
pub(crate) fn distinct_callers(
    reader: &Reader,
    index: &ReaderIndex,
    key: &str,
    limit: usize,
) -> CommandResult<(u32, Vec<CallingDefinition>)> {
    let mut sites: BTreeMap<Key, (String, u32)> = BTreeMap::new();
    for caller in index.callers(key).unwrap_or_default() {
        sites
            .entry(caller.owner)
            .or_insert_with(|| (caller.path.as_str().to_owned(), 0))
            .1 += 1;
    }
    let graph = index.index();
    let mut owners: Vec<(Key, String, u32)> = sites
        .into_iter()
        .map(|(owner, (path, sites))| (owner, path, sites))
        .collect();
    // Blocks are numbered in preorder, so within a file the local id follows source order.
    owners.sort_by_cached_key(|(owner, path, _)| {
        (
            path.clone(),
            graph.id(owner.as_str()).map_or(u32::MAX, |id| graph.local_id(id)),
        )
    });
    let total = count(owners.len());
    let mut found = Vec::new();
    for (owner, path, sites) in owners.into_iter().take(limit) {
        let line = reader
            .block(owner.as_str())?
            .map_or(1, |located| located.block.lines.start);
        let (name, qualified_name) = match reader.definition(owner.as_str())? {
            Some(definition) => (
                definition.label.to_string(),
                definition.qualified_name.to_string(),
            ),
            None => (path.clone(), path.clone()),
        };
        found.push(CallingDefinition {
            key: owner.as_str().to_owned(),
            name,
            qualified_name,
            path,
            line,
            sites,
        });
    }
    Ok((total, found))
}

/// The arguments of `callers`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallersArgs {
    /// The block's project.
    pub project: ProjectId,
    /// The block's key.
    pub key: String,
    /// How many calling definitions to list (20 when absent, at most 100).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// **Show callers**: the distinct definitions that call a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Callers {
    /// The block's project.
    pub project: ProjectId,
    /// The block's key.
    pub key: String,
    /// How many distinct definitions call it.
    pub total: u32,
    /// The first of them, by path and line.
    pub callers: Vec<CallingDefinition>,
}

/// `callers`: the distinct definitions that call a block, with their call sites counted.
pub fn callers(ctx: &Ctx<'_>, args: CallersArgs) -> CommandResult<Callers> {
    let key = key_arg(&args.key)?;
    let limit = args.limit.unwrap_or(DEFAULT_CALLING).clamp(1, MAX_CALLING) as usize;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let index = workspace.reader_index()?;
    if !index.contains(key) {
        return Err(CommandError::not_found(format!("no block `{key}`")));
    }
    let (total, callers) = distinct_callers(&reader, &index, key, limit)?;
    Ok(Callers {
        project: args.project,
        key: key.to_owned(),
        total,
        callers,
    })
}
