//! `relations` and `callers`: what the Selected section shows for one definition (its callers,
//! a page at a time; the calls it makes; and the entrances it is reached from), and **Show
//! callers**: the distinct definitions that call it. Callers come from the store's reverse
//! index, outgoing calls from the definition's own file, and Reached from from the reader index;
//! each wakes its project.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::LinkKind;
use unfold_core::{Key, RepoPath};
use unfold_reader::catalog;
use unfold_reader::notes::LinkEnd;
use unfold_reader::relations::{self, Caller};
use unfold_store::Reader;

use super::graph::block_tree;
use super::{count, key_arg};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// The most callers one page holds.
pub const MAX_CALLERS: u32 = 500;
/// Callers per page when the request doesn't say.
const DEFAULT_CALLERS: u32 = 100;
/// Calling definitions `callers` lists when the request doesn't say.
const DEFAULT_CALLING: u32 = 20;
/// The most calling definitions `callers` lists.
pub const MAX_CALLING: u32 = 100;
/// The most calls `callers` reads to find the distinct definitions they belong to.
pub const MAX_SCANNED: usize = 20_000;

/// The arguments of `relations`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationsArgs {
    /// The definition's project.
    pub project: ProjectId,
    /// The definition's key, or any block's (its definition's relations).
    pub key: String,
    /// Where the page of callers starts (`callersNext` of the previous page).
    #[serde(default)]
    #[ts(optional)]
    pub callers_offset: Option<u32>,
    /// How many callers to return (at most 500; 100 when absent).
    #[serde(default)]
    #[ts(optional)]
    pub callers_limit: Option<u32>,
}

/// One call of the definition, seen from the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CallerView {
    /// The definition the call belongs to.
    pub owner: Key,
    /// That definition's qualified name.
    pub owner_name: String,
    /// The call.
    pub call: Key,
    /// The call's file.
    pub path: RepoPath,
    /// The call's first line.
    pub line: u32,
    /// `calls`, or `passes` when the call passes the definition as a value.
    pub kind: LinkKind,
}

/// One call the definition makes: its `calls` link, or a `passes` link of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingView {
    /// The call.
    pub call: Key,
    /// The call's first line.
    pub line: u32,
    /// `calls` (what the call runs) or `passes` (a definition it passes as a value).
    pub kind: LinkKind,
    /// How the callee or passed name is written.
    pub text: String,
    /// Where it goes.
    pub to: LinkEnd,
    /// The qualified name of the project definition it reaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_name: Option<String>,
}

/// An entrance a definition is reached from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceRef {
    /// The entrance's key.
    pub key: Key,
    /// Its name.
    pub name: String,
    /// Its app's name.
    pub app: String,
}

/// A definition's relations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Relations {
    /// The definition.
    pub key: Key,
    /// One page of its callers, in path order and then source order.
    pub callers: Vec<CallerView>,
    /// How many callers there are in all.
    pub callers_total: u32,
    /// Where the next page of callers starts, if there is one.
    pub callers_next: Option<u32>,
    /// The calls it makes, in source order.
    pub outgoing: Vec<OutgoingView>,
    /// The entrances whose code reaches it ("reached at least from"), in file order. An app's
    /// own start is left out: it reaches nearly everything.
    pub reached_from: Vec<EntranceRef>,
}

/// `relations`: callers (paginated), outgoing calls, and reached-from for a definition.
pub fn relations(ctx: &Ctx<'_>, args: RelationsArgs) -> CommandResult<Relations> {
    let key = key_arg(&args.key)?;
    let offset = args.callers_offset.unwrap_or(0) as usize;
    let limit = args
        .callers_limit
        .unwrap_or(DEFAULT_CALLERS)
        .clamp(1, MAX_CALLERS) as usize;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let index = workspace.reader_index()?;
    let (tree, block) =
        block_tree(&reader, key)?.ok_or_else(|| CommandError::not_found(format!("no block `{key}`")))?;
    let definition = Some(block)
        .filter(|&id| tree.kind(id).is_definition())
        .or_else(|| tree.owner(block))
        .ok_or_else(|| CommandError::not_found(format!("`{key}` belongs to no definition")))?;
    let definition_key = tree.key(definition).clone();
    let mut outgoing = Vec::new();
    for link in relations::outgoing(&tree, definition) {
        let target_name = match &link.to.key {
            Some(target) => qualified_name(&reader, target.as_str())?,
            None => None,
        };
        outgoing.push(OutgoingView {
            call: link.call,
            line: link.line,
            kind: link.kind,
            text: link.text.to_string(),
            to: link.to,
            target_name,
        });
    }
    ctx.check_cancelled()?;
    // The store's pages run from the start: read through the requested one.
    let page = relations::callers(&reader, &index, definition_key.as_str(), None, offset + limit)?;
    let mut callers = Vec::with_capacity(limit);
    for caller in page.callers.into_iter().skip(offset) {
        callers.push(caller_view(&reader, caller)?);
    }
    let apps = reader.apps()?;
    let reached_from = index
        .reached_from(definition_key.as_str())
        .unwrap_or_default()
        .iter()
        .filter_map(|entrance| catalog::start(&apps, entrance.as_str()))
        .map(|start| EntranceRef {
            key: start.key().clone(),
            name: start.name().to_owned(),
            app: start.app().name.to_string(),
        })
        .collect();
    let next = offset + callers.len();
    let total = usize::try_from(page.total).unwrap_or(usize::MAX);
    Ok(Relations {
        key: definition_key,
        callers_total: count(total),
        callers_next: (next < total).then(|| count(next)),
        callers,
        outgoing,
        reached_from,
    })
}

/// The qualified name of the definition with `key`, if the catalog has it.
fn qualified_name(reader: &Reader, key: &str) -> unfold_store::Result<Option<String>> {
    Ok(reader
        .definition(key)?
        .map(|definition| definition.qualified_name.to_string()))
}

fn caller_view(reader: &Reader, caller: Caller) -> unfold_store::Result<CallerView> {
    let owner_name =
        qualified_name(reader, caller.owner.as_str())?.unwrap_or_else(|| caller.owner.to_string());
    Ok(CallerView {
        owner: caller.owner,
        owner_name,
        call: caller.call,
        path: caller.path,
        line: caller.line,
        kind: caller.kind,
    })
}

/// One definition that calls a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CallingDefinition {
    /// Its key.
    pub key: Key,
    /// Its display label.
    pub name: String,
    /// Its qualified name.
    pub qualified_name: String,
    /// Its file.
    pub path: RepoPath,
    /// Its first line.
    pub line: u32,
    /// How many of its calls reach the block.
    pub sites: u32,
}

/// The arguments of `callers`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallersArgs {
    /// The definition's project.
    pub project: ProjectId,
    /// The definition's key.
    pub key: String,
    /// How many calling definitions to list (20 when absent, at most 100).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// **Show callers**: the distinct definitions that call a definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Callers {
    /// The definition's project.
    pub project: ProjectId,
    /// The definition's key.
    pub key: String,
    /// How many distinct definitions call it (among the first 20,000 calls).
    pub total: u32,
    /// The first of them, by path and line.
    pub callers: Vec<CallingDefinition>,
}

/// `callers`: the distinct definitions that call a definition, with their calls counted.
pub fn callers(ctx: &Ctx<'_>, args: CallersArgs) -> CommandResult<Callers> {
    let key = key_arg(&args.key)?;
    let limit = args.limit.unwrap_or(DEFAULT_CALLING).clamp(1, MAX_CALLING) as usize;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let index = workspace.reader_index()?;
    if !index.contains(key) {
        return Err(CommandError::not_found(format!("no block `{key}`")));
    }
    let page = relations::callers(&reader, &index, key, None, MAX_SCANNED)?;
    // Calls per owner, ordered by the owner's file and first line.
    let mut owners: BTreeMap<(RepoPath, u32, Key), u32> = BTreeMap::new();
    for caller in page.callers {
        let line = index.line(caller.owner.as_str()).unwrap_or(1);
        *owners.entry((caller.path, line, caller.owner)).or_default() += 1;
    }
    let total = count(owners.len());
    let mut callers = Vec::new();
    for ((path, line, owner), sites) in owners.into_iter().take(limit) {
        let (name, qualified_name) = match reader.definition(owner.as_str())? {
            Some(definition) => (
                definition.label.to_string(),
                definition.qualified_name.to_string(),
            ),
            None => (owner.to_string(), owner.to_string()),
        };
        callers.push(CallingDefinition {
            key: owner,
            name,
            qualified_name,
            path,
            line,
            sites,
        });
    }
    Ok(Callers {
        project: args.project,
        key: key.to_owned(),
        total,
        callers,
    })
}
