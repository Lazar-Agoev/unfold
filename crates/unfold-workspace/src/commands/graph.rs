//! Commands for one card (docs/reader.md#the-drawn-flow): `card` (a
//! definition's drawn flow with the page's overlay, the card's open paths, and Show all calls;
//! or an app's or entrance's card), and the agent text the MCP server prints: `code_text` (the
//! compiled tree of a definition) and `flow_text` (a drawn flow). Each wakes the card's project
//! and reads its committed graph; the reader draws and prints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::highlight::Highlighter;
use unfold_core::vocab::{NodeKind, TargetState};
use unfold_core::{Key, LinkTarget, RepoPath};
use unfold_reader::catalog::{self, Start};
use unfold_reader::drawing::{self, CodeLabel, CodeText, Combine, DrawnFlow, Overlay, Proposal, TextSources};
use unfold_reader::overlay::{MAX_OPEN_DEPTH, MAX_OPEN_PATHS, ProposalSpot};
use unfold_reader::{Blocks, FileTree, Trees};
use unfold_store::Reader;

use super::{ProjectKeyArgs, count, key_arg, key_of};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// The most names an overlay may hold.
pub const MAX_NAMES: usize = 4_000;
/// The most combined steps an overlay may hold.
pub const MAX_COMBINED: usize = 1_000;
/// The most blocks one combined step may hold.
pub const MAX_MEMBERS: usize = 500;
/// The most proposed steps an overlay may hold (a page's limit).
pub const MAX_PROPOSED: usize = unfold_reader::board::MAX_PROPOSALS;
/// The longest name, title, or id, in characters.
pub const MAX_TEXT: usize = 400;

/// The page's overlay: names, combined steps, and proposed steps. It applies page-wide, by key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayArgs {
    /// Names, which replace a block's code name.
    #[serde(default)]
    #[ts(optional)]
    pub names: Option<Vec<NameArg>>,
    /// Combined steps.
    #[serde(default)]
    #[ts(optional)]
    pub combined: Option<Vec<CombineArg>>,
    /// Proposed steps.
    #[serde(default)]
    #[ts(optional)]
    pub proposed: Option<Vec<ProposalArg>>,
}

/// One name of the overlay.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NameArg {
    /// The named block.
    pub key: String,
    /// The name.
    pub text: String,
}

/// One combined step of the overlay.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CombineArg {
    /// Its id on the page.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its blocks: consecutive steps of one body, or one whole control block.
    pub members: Vec<String>,
    /// Whether it draws as one step.
    pub collapsed: bool,
}

/// One proposed step of the overlay: after a block, or at the start of a body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposalArg {
    /// Its id on the page.
    pub id: String,
    /// The step it goes after.
    #[serde(default)]
    #[ts(optional)]
    pub after: Option<String>,
    /// The definition or part at the start of whose body it goes.
    #[serde(default)]
    #[ts(optional)]
    pub start: Option<String>,
    /// What it would do.
    pub title: String,
}

/// The arguments of `card` and `flow_text`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardArgs {
    /// The card's project.
    pub project: ProjectId,
    /// The card's key: a module, definition, or type; or an app or entrance.
    pub key: String,
    /// The page's overlay (none when absent).
    #[serde(default)]
    #[ts(optional)]
    pub overlay: Option<OverlayArgs>,
    /// The calls and callbacks the card opens in place: at most 64 paths of 1 to 3 keys,
    /// outermost first.
    #[serde(default)]
    #[ts(optional)]
    pub open: Option<Vec<Vec<String>>>,
    /// Whether builtin calls are drawn (Show all calls; off when absent).
    #[serde(default)]
    #[ts(optional)]
    pub show_all_calls: Option<bool>,
}

/// A card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CardView {
    /// A module's, definition's, or type's card: its drawn flow, whose header names it.
    Definition {
        /// The drawn flow.
        flow: Box<DrawnFlow>,
    },
    /// An app's or entrance's card.
    Start(StartCard),
}

/// Whether a start is an app or one of its entrances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StartKind {
    /// What an app's `run` command starts.
    App,
    /// Something an app handles.
    Entrance,
}

/// An app's or entrance's card: declared, so drawn in the authored style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartCard {
    /// Its key.
    pub key: Key,
    /// An app or an entrance.
    pub kind: StartKind,
    /// The app's name (in small caps on an entrance's card).
    pub app: String,
    /// Its name.
    pub name: String,
    /// What starts it, as written: an entrance's `on`, an app's `run`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<String>,
    /// Its one sentence.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub about: Option<String>,
    /// The blocks it starts at, in the order they run: the cards it connects to.
    pub code: Vec<StartTarget>,
}

/// One block an app or entrance starts at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartTarget {
    /// The block key, as written.
    pub key: Key,
    /// What it names.
    pub state: TargetState,
    /// The definition's label, when the key names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// Its file, when the key names a block.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<RepoPath>,
    /// Its first line, when the key names a block.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub line: Option<u32>,
}

/// `card`: an app's or entrance's card, or a definition's drawn flow shaped by the page's
/// overlay, with the calls it opens in place drawn inside it.
pub fn card(ctx: &Ctx<'_>, args: CardArgs) -> CommandResult<CardView> {
    let key = key_arg(&args.key)?;
    let (overlay, open) = (overlay(args.overlay)?, open_paths(args.open)?);
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let highlighters = workspace.highlighters();
    let sources = text_sources(&workspace, &highlighters);
    let show_all_calls = args.show_all_calls.unwrap_or(false);
    reader
        .snapshot(|reader| {
            let apps = reader.apps()?;
            if let Some(start) = catalog::start(&apps, key) {
                return Ok(Some(CardView::Start(start_card(reader, start)?)));
            }
            let Some(trees) = trees(reader, key, &open)? else {
                return Ok(None);
            };
            Ok(draw(&trees, &sources, key, &overlay, open, show_all_calls)
                .map(|flow| CardView::Definition { flow: Box::new(flow) }))
        })?
        .ok_or_else(|| CommandError::not_found(format!("no card `{key}`")))
}

/// The agent text of a drawn flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FlowText {
    /// The drawn flow as text, each line ending with its blocks' full keys (`key=…`).
    pub text: String,
    /// Each block of the card's definition by its short label (relative to the definition, as
    /// agents write it) and full key, so a short label maps to a key by lookup.
    pub labels: Vec<CodeLabel>,
}

/// `flow_text`: a definition's drawn flow as agent text, shaped like `card`'s, with the label
/// to key map of the definition's blocks.
pub fn flow_text(ctx: &Ctx<'_>, args: CardArgs) -> CommandResult<FlowText> {
    let key = key_arg(&args.key)?;
    let (overlay, open) = (overlay(args.overlay)?, open_paths(args.open)?);
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let highlighters = workspace.highlighters();
    let sources = text_sources(&workspace, &highlighters);
    let show_all_calls = args.show_all_calls.unwrap_or(false);
    reader
        .snapshot(|reader| {
            let Some(trees) = trees(reader, key, &open)? else {
                return Ok(None);
            };
            let labels = match trees.block(key) {
                Some(block) => drawing::code_text(block.tree, &sources, block.id).labels,
                None => Vec::new(),
            };
            Ok(
                draw(&trees, &sources, key, &overlay, open, show_all_calls).map(|flow| FlowText {
                    text: drawing::flow_text(&flow),
                    labels,
                }),
            )
        })?
        .ok_or_else(|| no_definition(key))
}

/// `code_text`: the compiled tree of a definition, for `read_code`: each block it owns with its
/// key, what it calls, binds, or tests, and the lines of code in between, with the label to key
/// map.
pub fn code_text(ctx: &Ctx<'_>, args: ProjectKeyArgs) -> CommandResult<CodeText> {
    let key = key_arg(&args.key)?;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    let highlighters = workspace.highlighters();
    let sources = text_sources(&workspace, &highlighters);
    reader
        .snapshot(|reader| {
            Ok(definition_tree(reader, key)?.map(|(tree, id)| drawing::code_text(&tree, &sources, id)))
        })?
        .ok_or_else(|| no_definition(key))
}

/// Where a drawing reads its files' text: the folder, with the languages' highlighters and the
/// sources kept from when they last verified.
fn text_sources<'a>(workspace: &'a Workspace, highlighters: &'a [&'a dyn Highlighter]) -> TextSources<'a> {
    TextSources {
        root: workspace.root(),
        highlighters,
        kept: workspace.kept_sources(),
    }
}

fn no_definition(key: &str) -> CommandError {
    CommandError::not_found(format!("no module, definition, or type `{key}`"))
}

/// The tree of the file holding block `key`, and the block's ID there.
/// Both reads share one snapshot, so the ID always indexes that version of the file.
pub(crate) fn block_tree(reader: &Reader, key: &str) -> unfold_store::Result<Option<(FileTree, u32)>> {
    let graph = reader.snapshot(|reader| {
        let Some(located) = reader.node(key)? else {
            return Ok(None);
        };
        reader.file_graph(located.path.as_str())
    })?;
    Ok(graph
        .map(FileTree::new)
        .and_then(|tree| tree.id(key).map(|id| (tree, id))))
}

/// [`block_tree`] for a module, definition, or type.
fn definition_tree(reader: &Reader, key: &str) -> unfold_store::Result<Option<(FileTree, u32)>> {
    Ok(block_tree(reader, key)?.filter(|(tree, id)| tree.kind(*id).is_definition()))
}

/// The trees a drawn flow of definition `key` needs: its file, and the files of the callees its
/// open paths reach. `None` when no definition has the key.
fn trees(reader: &Reader, key: &str, open: &[Vec<Key>]) -> unfold_store::Result<Option<Trees>> {
    let Some((tree, _)) = definition_tree(reader, key)? else {
        return Ok(None);
    };
    let mut trees = Trees::new();
    trees.insert(tree);
    for path in open {
        load_callees(reader, &mut trees, path)?;
    }
    Ok(Some(trees))
}

/// The drawn flow of definition `key`, whose [`trees`] are loaded.
fn draw(
    trees: &Trees,
    sources: &TextSources<'_>,
    key: &str,
    overlay: &Overlay,
    open: Vec<Vec<Key>>,
    show_all_calls: bool,
) -> Option<DrawnFlow> {
    let card = drawing::Card {
        definition: trees.block(key)?.key().clone(),
        open,
        show_all_calls,
    };
    drawing::draw(trees, sources, overlay, &card)
}

/// Loads the files of the callees an open path reaches, outermost first: a project call opens
/// its target (in its own file), a callback opens in place (in its call's file).
fn load_callees(reader: &Reader, trees: &mut Trees, path: &[Key]) -> unfold_store::Result<()> {
    for key in path {
        let target = match trees.block(key.as_str()) {
            Some(block) if block.kind() == NodeKind::Call => match block.tree.calls_link(block.id) {
                Some(link) => match &link.to {
                    LinkTarget::Project(target) => Some(target.clone()),
                    _ => return Ok(()),
                },
                None => return Ok(()),
            },
            Some(_) => None,
            None => return Ok(()),
        };
        if let Some(target) = target
            && trees.block(target.as_str()).is_none()
        {
            match block_tree(reader, target.as_str())? {
                Some((tree, _)) => trees.insert(tree),
                None => return Ok(()),
            }
        }
    }
    Ok(())
}

/// An app's or entrance's card, with each block its code names.
fn start_card(reader: &Reader, start: Start<'_>) -> unfold_store::Result<StartCard> {
    let mut code = Vec::with_capacity(start.targets().len());
    for target in start.targets() {
        let located = reader.node(target.code.as_str())?;
        let label = match &located {
            Some(_) => reader
                .definition(target.code.as_str())?
                .map(|definition| definition.label.to_string()),
            None => None,
        };
        code.push(StartTarget {
            key: target.code.clone(),
            state: target.state,
            label,
            line: located.as_ref().map(|located| located.node.lines.start),
            path: located.map(|located| located.path),
        });
    }
    Ok(StartCard {
        key: start.key().clone(),
        kind: match start {
            Start::App(_) => StartKind::App,
            Start::Entrance { .. } => StartKind::Entrance,
        },
        app: start.app().name.to_string(),
        name: start.name().to_owned(),
        on: start.on().map(str::to_owned),
        about: start.about().map(str::to_owned),
        code,
    })
}

/// Text of at most [`MAX_TEXT`] characters.
fn limited(text: String, what: &str) -> CommandResult<String> {
    if text.chars().count() > MAX_TEXT {
        return Err(CommandError::invalid_args(format!(
            "{what} is longer than {MAX_TEXT} characters"
        )));
    }
    Ok(text)
}

/// The overlay the drawing reads, with its sizes checked.
fn overlay(args: Option<OverlayArgs>) -> CommandResult<Overlay> {
    let args = args.unwrap_or_default();
    let (names, combined, proposed) = (
        args.names.unwrap_or_default(),
        args.combined.unwrap_or_default(),
        args.proposed.unwrap_or_default(),
    );
    if names.len() > MAX_NAMES || combined.len() > MAX_COMBINED || proposed.len() > MAX_PROPOSED {
        return Err(CommandError::invalid_args(format!(
            "an overlay holds at most {MAX_NAMES} names, {MAX_COMBINED} combined steps, and \
             {MAX_PROPOSED} proposed steps"
        )));
    }
    let mut overlay = Overlay {
        names: BTreeMap::new(),
        combined: Vec::with_capacity(combined.len()),
        proposed: Vec::with_capacity(proposed.len()),
    };
    for name in names {
        overlay
            .names
            .insert(key_of(&name.key)?, limited(name.text, "a name")?);
    }
    for combine in combined {
        if combine.members.is_empty() || combine.members.len() > MAX_MEMBERS {
            return Err(CommandError::invalid_args(format!(
                "a combined step holds 1 to {MAX_MEMBERS} blocks"
            )));
        }
        overlay.combined.push(Combine {
            id: limited(combine.id, "a combined step's id")?,
            title: limited(combine.title, "a combined step's title")?,
            members: combine
                .members
                .into_iter()
                .map(|member| key_of(&member))
                .collect::<CommandResult<_>>()?,
            collapsed: combine.collapsed,
        });
    }
    for proposal in proposed {
        let spot = match (proposal.after, proposal.start) {
            (Some(after), None) => ProposalSpot::After(key_of(&after)?),
            (None, Some(start)) => ProposalSpot::Start(key_of(&start)?),
            _ => {
                return Err(CommandError::invalid_args(
                    "a proposed step goes either `after` a block or at the `start` of a body",
                ));
            }
        };
        overlay.proposed.push(Proposal {
            id: limited(proposal.id, "a proposed step's id")?,
            spot,
            title: limited(proposal.title, "a proposed step's title")?,
        });
    }
    Ok(overlay)
}

/// The open paths, with their sizes checked.
fn open_paths(open: Option<Vec<Vec<String>>>) -> CommandResult<Vec<Vec<Key>>> {
    let open = open.unwrap_or_default();
    if open.len() > MAX_OPEN_PATHS {
        return Err(CommandError::invalid_args(format!(
            "a card opens at most {MAX_OPEN_PATHS} paths"
        )));
    }
    open.into_iter()
        .map(|path| {
            if path.is_empty() || path.len() > MAX_OPEN_DEPTH {
                return Err(CommandError::invalid_args(format!(
                    "an open path lists 1 to {MAX_OPEN_DEPTH} keys (got {})",
                    count(path.len())
                )));
            }
            path.iter().map(|key| key_of(key)).collect()
        })
        .collect()
}
