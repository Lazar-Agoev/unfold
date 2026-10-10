//! Graph commands for one card (contract §4.4, architecture §10.2–10.4): `card` (an entrance
//! or block card with its Code and Flow models), `peek` (the hover over a call link), and
//! `preview` (Add's preview pane and the map popover). Each wakes the card's project.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{
    BlockKind, DetectorId, EdgeKind, EntranceKind, FileState, HttpMethod, Language, Reason, Resolution,
    TargetRole, TriggerStatus,
};
use unfold_core::{Entrance, FileGraph, LineIndex, ModuleLabel, RepoPath};
use unfold_reader::code::{CodeLine, CodeSource, CodeSpan, CodeView, FoldPolicy, render};
use unfold_reader::flow::{FlowContext, FlowModel, build_flow};
use unfold_reader::mermaid::entrance_title;
use unfold_reader::notes::note;
use unfold_reader::relations::owned_edges;
use unfold_reader::source::{SourceState, Verified, verify_file};
use unfold_reader::{ReaderIndex, catalog, file_owners};
use unfold_store::Reader;

use super::file_view::render_page;
use super::relations::{EntranceRef, distinct_callers, entrance_ref};
use super::{LineRange, ProjectKeyArgs, count, key_arg};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// Which of a block card's models to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardLens {
    /// Only the Code model.
    Code,
    /// Only the Flow model.
    Flow,
    /// Both.
    #[default]
    Both,
}

/// The arguments of `card`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardArgs {
    /// The card's project.
    pub project: ProjectId,
    /// The block or entrance key.
    pub key: String,
    /// Which models to build for a block card (both when absent).
    #[serde(default)]
    #[ts(optional)]
    pub lens: Option<CardLens>,
}

/// A block card's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
    /// The block's key.
    pub key: String,
    /// Its kind.
    pub kind: BlockKind,
    /// Its display label.
    pub name: String,
    /// Its qualified name.
    pub qualified_name: String,
    /// Its file.
    pub path: String,
    /// Its first line.
    pub start_line: u32,
    /// Its last line.
    pub end_line: u32,
    /// The file's language.
    pub language: Language,
    /// The file's state in the graph.
    pub file_state: FileState,
    /// Whether the bytes on disk are the ones the graph was built from.
    pub source: SourceState,
    /// The function, class, or file whose activation the block belongs to.
    pub owner: String,
}

/// An entrance card's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceHeader {
    /// The entrance's key.
    pub key: String,
    /// Its kind.
    pub kind: EntranceKind,
    /// Its title: the label, or the method (or kind) and trigger.
    pub title: String,
    /// The manual label, if any.
    pub label: Option<String>,
    /// The HTTP method.
    pub method: Option<HttpMethod>,
    /// The trigger (path, queue, schedule, command, …).
    pub trigger: Option<String>,
    /// How much of the trigger is known.
    pub trigger_status: TriggerStatus,
    /// The detector that found it.
    pub detector: DetectorId,
    /// The evidence file's language, when it is source.
    pub language: Option<Language>,
}

/// Where an entrance is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceView {
    /// The source file or configuration input.
    pub path: String,
    /// The declaration's first line, when its file verified.
    pub start_line: Option<u32>,
    /// The declaration's last line, when its file verified.
    pub end_line: Option<u32>,
    /// The JSON pointer into a configuration file.
    pub pointer: Option<String>,
    /// Whether the evidence file is the one the graph was built from.
    pub source: SourceState,
    /// The Code model of source evidence (the registration call).
    pub code: Option<CodeView>,
    /// The exact text of configuration evidence (a `package.json` script), when it verified.
    pub text: Option<String>,
}

/// One handler or startup of an entrance, in registration order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TargetView {
    /// The declaration order.
    pub position: u32,
    /// `startup` or `handler`.
    pub role: TargetRole,
    /// The handler, locator, or launch path as written.
    pub text: String,
    /// How it resolved.
    pub resolution: Resolution,
    /// The target's key, when resolved.
    pub target: Option<String>,
    /// The target's qualified name, when resolved.
    pub name: Option<String>,
    /// The target's file, when resolved.
    pub path: Option<String>,
    /// The target's first line, when resolved.
    pub line: Option<u32>,
    /// The module (external) or reason (unresolved) in words.
    pub note: Option<String>,
    /// Why it is unresolved.
    pub reason: Option<Reason>,
    /// Where an external target comes from.
    pub module: Option<ModuleLabel>,
}

/// A card: an entrance with its evidence and targets, or a block with its Code and Flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum CardView {
    /// A block card.
    Block {
        /// The header.
        header: BlockHeader,
        /// The Code model, unless only Flow was asked for.
        code: Option<CodeView>,
        /// The Flow model ("Paths from written structure"), unless only Code was asked for.
        flow: Option<FlowModel>,
    },
    /// An entrance card.
    Entrance {
        /// The header.
        header: EntranceHeader,
        /// Where it is declared.
        evidence: EvidenceView,
        /// Its targets in registration order.
        targets: Vec<TargetView>,
        /// The applications it belongs to, by name.
        applications: Vec<String>,
    },
}

/// Names of targets in other files, from the store. A page's authored Flow labels are the UI's
/// to apply (decision V-56), so one card serves every page.
struct CardContext<'a> {
    reader: &'a Reader,
}

impl FlowContext for CardContext<'_> {
    fn label(&self, _key: &str) -> Option<&str> {
        None
    }

    fn target_name(&self, key: &str) -> Option<String> {
        self.reader
            .definition(key)
            .ok()
            .flatten()
            .map(|definition| definition.qualified_name.to_string())
    }
}

/// `card`: everything one card shows. An entrance key (`entrance:…`) gives an entrance card;
/// any block key gives a block card.
pub fn card(ctx: &Ctx<'_>, args: CardArgs) -> CommandResult<CardView> {
    let key = key_arg(&args.key)?;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    if key.starts_with("entrance:") {
        let entrance = reader
            .entrance(key)?
            .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))?;
        return entrance_card(&workspace, &reader, entrance);
    }
    let (file, local) = locate(&reader, key)?;
    let context = CardContext { reader: &reader };
    block_card(&workspace, &file, local, &context, args.lens.unwrap_or_default())
}

/// The file graph holding block `key` and the block's local ID. Both reads share one snapshot,
/// so a commit between them can't pair the block's local ID with another version of its file:
/// the returned local ID always indexes `blocks`.
pub(crate) fn locate(reader: &Reader, key: &str) -> CommandResult<(FileGraph, u32)> {
    let found = reader.snapshot(|reader| {
        let Some(located) = reader.block(key)? else {
            return Ok(None);
        };
        let file = reader.file_graph(located.path.as_str())?;
        Ok(file.map(|file| (file, located.block.local_id)))
    })?;
    match found {
        Some((file, local)) if (local as usize) < file.blocks.len() => Ok((file, local)),
        _ => Err(CommandError::not_found(format!("no block `{key}`"))),
    }
}

/// A block card's header, with the block's file verified against the graph.
fn block_header(file: &FileGraph, local: u32, source: SourceState) -> BlockHeader {
    let block = &file.blocks[local as usize];
    let owner = file_owners(&file.blocks).get(local as usize).map_or_else(
        || block.key.clone(),
        |&owner| file.blocks[owner as usize].key.clone(),
    );
    BlockHeader {
        key: block.key.as_str().to_owned(),
        kind: block.kind,
        name: catalog::label(file, local).unwrap_or_default(),
        qualified_name: catalog::qualified_name(file, local).unwrap_or_default(),
        path: file.file.path.as_str().to_owned(),
        start_line: block.lines.start,
        end_line: block.lines.end,
        language: file.file.language,
        file_state: file.file.state,
        source,
        owner: owner.as_str().to_owned(),
    }
}

fn block_card(
    workspace: &Workspace,
    file: &FileGraph,
    local: u32,
    context: &CardContext<'_>,
    lens: CardLens,
) -> CommandResult<CardView> {
    let block = &file.blocks[local as usize];
    let verified = verify_file(workspace.root(), &file.file.path, file.file.hash);
    let state = verified.state();
    let text = match &verified {
        Verified::Current(snapshot) => Some(snapshot.text()),
        Verified::Changed | Verified::Missing => None,
    };
    let code = if lens == CardLens::Flow {
        None
    } else {
        let highlighter = workspace.highlighter(file.file.language);
        let source = match (&verified, text) {
            (Verified::Current(_), Some(text)) => {
                CodeSource::Current(render(file, local, text, highlighter, FoldPolicy::Card)?)
            }
            (Verified::Missing, _) => CodeSource::Missing,
            _ => CodeSource::Changed,
        };
        Some(CodeView {
            key: block.key.clone(),
            path: file.file.path.clone(),
            language: file.file.language,
            kind: block.kind,
            start_line: block.lines.start,
            end_line: block.lines.end,
            source,
        })
    };
    let flow = if lens == CardLens::Code {
        None
    } else {
        Some(build_flow(file, local, text, state, context)?)
    };
    Ok(CardView::Block {
        header: block_header(file, local, state),
        code,
        flow,
    })
}

/// An entrance card's header.
fn entrance_header(reader: &Reader, entrance: &Entrance) -> CommandResult<EntranceHeader> {
    let language = reader
        .file(entrance.evidence.path.as_str())?
        .map(|row| row.language);
    Ok(EntranceHeader {
        key: entrance.key.as_str().to_owned(),
        kind: entrance.kind,
        title: entrance_title(entrance),
        label: entrance.label.as_deref().map(str::to_owned),
        method: entrance.method,
        trigger: entrance.trigger.as_deref().map(str::to_owned),
        trigger_status: entrance.trigger_status,
        detector: entrance.detector,
        language,
    })
}

fn entrance_card(workspace: &Workspace, reader: &Reader, entrance: Entrance) -> CommandResult<CardView> {
    let evidence = evidence_view(workspace, reader, &entrance)?;
    let mut targets = Vec::with_capacity(entrance.targets.len());
    for target in &entrance.targets {
        let resolved = match target
            .target
            .as_ref()
            .filter(|_| target.resolution == Resolution::Resolved)
        {
            Some(key) => reader.block(key.as_str())?.map(|located| {
                let name = reader
                    .definition(key.as_str())
                    .ok()
                    .flatten()
                    .map(|definition| definition.qualified_name.to_string());
                (name, located.path.as_str().to_owned(), located.block.lines.start)
            }),
            None => None,
        };
        let (name, path, line) = match resolved {
            Some((name, path, line)) => (name, Some(path), Some(line)),
            None => (None, None, None),
        };
        targets.push(TargetView {
            position: target.position,
            role: target.role,
            text: target.text.to_string(),
            resolution: target.resolution,
            target: target.target.as_ref().map(|key| key.as_str().to_owned()),
            name,
            path,
            line,
            note: note(target.resolution, target.reason, target.module.as_ref()),
            reason: target.reason,
            module: target.module.clone(),
        });
    }
    let applications = application_names(reader, entrance.key.as_str())?;
    Ok(CardView::Entrance {
        header: entrance_header(reader, &entrance)?,
        evidence,
        targets,
        applications,
    })
}

/// The names of the applications an entrance belongs to.
fn application_names(reader: &Reader, entrance: &str) -> CommandResult<Vec<String>> {
    let memberships = reader.memberships()?;
    let applications = reader.applications()?;
    Ok(memberships
        .iter()
        .filter(|membership| membership.entrance.as_str() == entrance)
        .filter_map(|membership| {
            applications
                .iter()
                .find(|application| application.startup == membership.application)
                .map(|application| application.name.to_string())
        })
        .collect())
}

fn evidence_view(workspace: &Workspace, reader: &Reader, entrance: &Entrance) -> CommandResult<EvidenceView> {
    let evidence = &entrance.evidence;
    let mut view = EvidenceView {
        path: evidence.path.as_str().to_owned(),
        start_line: None,
        end_line: None,
        pointer: evidence.pointer.as_deref().map(str::to_owned),
        source: SourceState::Missing,
        code: None,
        text: None,
    };
    if let Some(block) = &evidence.block
        && let Ok((file, local)) = locate(reader, block.as_str())
    {
        let card = block_card(workspace, &file, local, &CardContext { reader }, CardLens::Code)?;
        if let CardView::Block { header, code, .. } = card {
            view.start_line = Some(header.start_line);
            view.end_line = Some(header.end_line);
            view.source = header.source;
            view.code = code;
        }
        return Ok(view);
    }
    let input = reader
        .inputs()?
        .into_iter()
        .find(|input| input.path == evidence.path);
    let expected = match input {
        Some(input) => Some(input.hash),
        None => reader
            .file(evidence.path.as_str())?
            .and_then(|row| row.facts_hash),
    };
    let verified = verify_file(workspace.root(), &evidence.path, expected);
    view.source = verified.state();
    if let Verified::Current(snapshot) = verified
        && let Some(text) = snapshot.text().get(evidence.range.as_usize())
    {
        let lines = LineIndex::new(snapshot.text());
        view.start_line = Some(lines.line_of(evidence.range.start));
        view.end_line = Some(lines.line_of(evidence.range.end.saturating_sub(1).max(evidence.range.start)));
        view.text = Some(text.to_owned());
    }
    Ok(view)
}

/// What hovering a call link shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Peek {
    /// The key.
    pub key: String,
    /// The block kind, or `entrance`.
    pub kind: String,
    /// The display label or entrance title.
    pub name: String,
    /// The qualified name of a block.
    pub qualified_name: Option<String>,
    /// The first written line of a block (its signature), when its source verified.
    pub signature: Option<String>,
    /// The file.
    pub path: String,
    /// The first line.
    pub line: Option<u32>,
    /// The file's language.
    pub language: Option<Language>,
}

/// The longest signature line a peek shows, in characters.
const MAX_SIGNATURE: usize = 200;

/// `peek`: a block's signature line and location, or an entrance's title and location.
pub fn peek(ctx: &Ctx<'_>, args: ProjectKeyArgs) -> CommandResult<Peek> {
    let key = key_arg(&args.key)?;
    let workspace = ctx.live(&args.project)?;
    let reader = workspace.readers()?.get()?;
    if key.starts_with("entrance:") {
        let entrance = reader
            .entrance(key)?
            .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))?;
        let evidence = evidence_view(&workspace, &reader, &entrance)?;
        return Ok(Peek {
            key: key.to_owned(),
            kind: "entrance".to_owned(),
            name: entrance_title(&entrance),
            qualified_name: None,
            signature: None,
            path: evidence.path,
            line: evidence.start_line,
            language: reader
                .file(entrance.evidence.path.as_str())?
                .map(|row| row.language),
        });
    }
    let (file, local) = locate(&reader, key)?;
    let block = &file.blocks[local as usize];
    let signature = match verify_file(workspace.root(), &file.file.path, file.file.hash) {
        Verified::Current(snapshot) => snapshot
            .text()
            .get(block.range.as_usize())
            .and_then(|text| text.split(['\n', '\r', '\u{2028}', '\u{2029}']).next())
            .map(|line| line.trim_end().chars().take(MAX_SIGNATURE).collect()),
        Verified::Changed | Verified::Missing => None,
    };
    Ok(Peek {
        key: key.to_owned(),
        kind: block.kind.as_str().to_owned(),
        name: catalog::label(&file, local).unwrap_or_default(),
        qualified_name: catalog::qualified_name(&file, local),
        signature,
        path: file.file.path.as_str().to_owned(),
        line: Some(block.lines.start),
        language: Some(file.file.language),
    })
}

/// The most lines a block preview shows.
pub const PREVIEW_LINES: u32 = 5;
/// The most entrances a block preview lists under "Reached from".
const PREVIEW_REACHED: usize = 5;
/// The most calls a block preview lists.
const PREVIEW_CALLS: usize = 8;
/// The most callers a block preview lists.
const PREVIEW_CALLERS: usize = 5;
/// The most handlers an entrance preview lists.
const PREVIEW_HANDLERS: usize = 8;
/// The most external modules an entrance preview lists.
const PREVIEW_MODULES: usize = 5;
/// The most files of an entrance's closure whose external calls an entrance preview counts.
const PREVIEW_MODULE_FILES: usize = 200;

/// A count and the first few items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Listed<T> {
    /// How many there are.
    pub total: u32,
    /// The first few.
    pub items: Vec<T>,
}

impl<T> Listed<T> {
    fn first(items: Vec<T>, limit: usize) -> Self {
        let total = count(items.len());
        let mut items = items;
        items.truncate(limit);
        Self { total, items }
    }
}

/// The first lines of a block, as the peek sheet's page model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCode {
    /// The first line shown.
    pub start_line: u32,
    /// Where `text` begins in the file, in UTF-16 code units.
    pub text_start: u32,
    /// The text of the lines shown, terminators included.
    pub text: String,
    /// The lines shown.
    pub lines: Vec<CodeLine>,
    /// Syntax colors overlapping them.
    pub spans: Vec<CodeSpan>,
}

/// One call a block makes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCall {
    /// The written callee.
    pub name: String,
    /// The target's key, when the call resolved.
    pub key: Option<String>,
    /// The module of an external target.
    pub module: Option<String>,
}

/// One definition that calls a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCaller {
    /// The calling definition's key.
    pub key: String,
    /// Its qualified name (its file's path for a file-level call).
    pub name: String,
    /// Its file.
    pub path: String,
    /// Its first line.
    pub line: u32,
}

/// One handler an entrance runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PreviewHandler {
    /// Its qualified name, or the handler as written when it didn't resolve.
    pub name: String,
    /// Its key, when it resolved.
    pub key: Option<String>,
    /// Its file, when it resolved.
    pub path: Option<String>,
    /// Its first line, when it resolved.
    pub line: Option<u32>,
}

/// Where an entrance is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    /// The source file or configuration input.
    pub path: String,
    /// The declaration's first line, when known.
    pub line: Option<u32>,
}

/// What Add's preview pane and the map popover show for one result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Preview {
    /// A block.
    Block {
        /// Its project.
        project: ProjectId,
        /// Its card header.
        header: BlockHeader,
        /// Its first lines (at most 5).
        code: PreviewCode,
        /// The entrances it is reached from (at most 5 listed).
        reached_from: Listed<EntranceRef>,
        /// The calls it makes, in source order (at most 8 listed).
        calls: Listed<PreviewCall>,
        /// The definitions that call it, by path and line (at most 5 listed).
        called_by: Listed<PreviewCaller>,
    },
    /// An entrance.
    Entrance {
        /// Its project.
        project: ProjectId,
        /// Its card header.
        header: EntranceHeader,
        /// The handlers it runs, in registration order (at most 8).
        handlers: Vec<PreviewHandler>,
        /// Where it is declared.
        declared: Declared,
        /// The definitions in its activation closure.
        reaches: u32,
        /// The external modules its closure calls, most called first (at most 5).
        external_modules: Vec<String>,
    },
}

/// `preview`: a block's header, first lines, reached-from, calls, and callers; or an entrance's
/// header, handlers, declaration, reach, and external modules.
pub fn preview(ctx: &Ctx<'_>, args: ProjectKeyArgs) -> CommandResult<Preview> {
    let key = key_arg(&args.key)?;
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let reader = workspace.readers()?.get()?;
    let index = workspace.reader_index()?;
    if key.starts_with("entrance:") {
        let entrance = reader
            .entrance(key)?
            .ok_or_else(|| CommandError::not_found(format!("no entrance `{key}`")))?;
        return entrance_preview(&workspace, &reader, &index, args.project, &entrance);
    }
    let (file, local) = locate(&reader, key)?;
    let block = &file.blocks[local as usize];
    let verified = verify_file(workspace.root(), &file.file.path, file.file.hash);
    let header = block_header(&file, local, verified.state());
    let lines = LineRange {
        start_line: block.lines.start,
        end_line: block
            .lines
            .end
            .min(block.lines.start.saturating_add(PREVIEW_LINES - 1)),
    };
    let page = render_page(
        &workspace,
        &project.extras(),
        file.file.path.as_str(),
        Some(lines),
    )?;
    ctx.check_cancelled()?;
    let reached = index.reached_from(key).unwrap_or_default();
    let reached_total = count(reached.len());
    let mut reached_items = Vec::new();
    for entrance in reached.iter().take(PREVIEW_REACHED) {
        if let Some(entrance) = reader.entrance(entrance.as_str())? {
            reached_items.push(entrance_ref(&entrance));
        }
    }
    let mut seen = BTreeSet::new();
    let calls: Vec<PreviewCall> = owned_edges(&file, local)
        .into_iter()
        .filter(|edge| matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs))
        .filter(|edge| seen.insert((edge.text.clone(), edge.target.clone())))
        .map(|edge| PreviewCall {
            name: edge.text.to_string(),
            key: edge.target.as_ref().map(|target| target.as_str().to_owned()),
            module: edge.module.as_ref().map(|module| module.as_str().to_owned()),
        })
        .collect();
    let (called_total, callers) = distinct_callers(&reader, &index, key, PREVIEW_CALLERS)?;
    let called_items = callers
        .into_iter()
        .map(|caller| PreviewCaller {
            name: caller.qualified_name,
            key: caller.key,
            path: caller.path,
            line: caller.line,
        })
        .collect();
    Ok(Preview::Block {
        project: args.project,
        header,
        code: PreviewCode {
            start_line: page.start_line,
            text_start: page.text_start,
            text: page.text,
            lines: page.lines,
            spans: page.spans,
        },
        reached_from: Listed {
            total: reached_total,
            items: reached_items,
        },
        calls: Listed::first(calls, PREVIEW_CALLS),
        called_by: Listed {
            total: called_total,
            items: called_items,
        },
    })
}

fn entrance_preview(
    workspace: &Workspace,
    reader: &Reader,
    index: &ReaderIndex,
    project: ProjectId,
    entrance: &Entrance,
) -> CommandResult<Preview> {
    let mut handlers = Vec::new();
    for target in entrance.targets.iter().take(PREVIEW_HANDLERS) {
        let resolved = match target
            .target
            .as_ref()
            .filter(|_| target.resolution == Resolution::Resolved)
        {
            Some(key) => reader.block(key.as_str())?.map(|located| (key, located)),
            None => None,
        };
        handlers.push(match resolved {
            Some((key, located)) => PreviewHandler {
                name: reader.definition(key.as_str())?.map_or_else(
                    || target.text.to_string(),
                    |definition| definition.qualified_name.to_string(),
                ),
                key: Some(key.as_str().to_owned()),
                path: Some(located.path.as_str().to_owned()),
                line: Some(located.block.lines.start),
            },
            None => PreviewHandler {
                name: target.text.to_string(),
                key: None,
                path: None,
                line: None,
            },
        });
    }
    let line = match &entrance.evidence.block {
        Some(block) => reader
            .block(block.as_str())?
            .map(|located| located.block.lines.start),
        None => evidence_view(workspace, reader, entrance)?.start_line,
    };
    let closure = index.entrance_closure(entrance.key.as_str()).unwrap_or_default();
    let graph = index.index();
    let ids: Vec<_> = closure.iter().filter_map(|key| graph.id(key.as_str())).collect();
    let reaches = count(
        ids.iter()
            .filter(|&&id| {
                let kind = graph.kind(id);
                kind.is_function_like() || kind == BlockKind::Class
            })
            .count(),
    );
    // External calls owned by the closure's blocks, counted per module, file by file.
    let mut by_file: BTreeMap<&RepoPath, BTreeSet<u32>> = BTreeMap::new();
    for &id in &ids {
        by_file
            .entry(graph.path(id))
            .or_default()
            .insert(graph.local_id(id));
    }
    let mut modules: BTreeMap<String, u32> = BTreeMap::new();
    for (path, owners) in by_file.into_iter().take(PREVIEW_MODULE_FILES) {
        let Some(file) = reader.file_graph(path.as_str())? else {
            continue;
        };
        let owned_by = file_owners(&file.blocks);
        for edge in &file.edges {
            if edge.resolution == Resolution::External
                && owned_by
                    .get(edge.source as usize)
                    .is_some_and(|owner| owners.contains(owner))
                && let Some(module) = &edge.module
            {
                *modules.entry(module.as_str().to_owned()).or_default() += 1;
            }
        }
    }
    let mut modules: Vec<(String, u32)> = modules.into_iter().collect();
    modules.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(Preview::Entrance {
        project,
        header: entrance_header(reader, entrance)?,
        handlers,
        declared: Declared {
            path: entrance.evidence.path.as_str().to_owned(),
            line,
        },
        reaches,
        external_modules: modules
            .into_iter()
            .take(PREVIEW_MODULES)
            .map(|(module, _)| module)
            .collect(),
    })
}
