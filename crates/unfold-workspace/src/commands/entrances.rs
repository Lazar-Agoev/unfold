//! `entrances`: how a project starts (contract §4.3, decision D-22), for Home's "How it starts"
//! and Add's map. Applications are named after their startup, with their launches and their
//! entrances; then the entrances no launch reaches, and the launches into code that isn't
//! indexed. Each application's entrances are filtered and paged in Rust: the first
//! `perApplication` rows, with the total.
//!
//! Rows sort HTTP entrances first, by group (the trigger's first path segment; routes without
//! one last), trigger path, and method (GET, POST, PUT, PATCH, DELETE, then the rest); then the
//! other kinds by kind and title. A resting project is served from its stored graph; `reaches` (the definitions an
//! entrance's activation closure holds) needs the live project's reader index and is `None`
//! while it rests.

use std::cmp::Ordering;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{
    BlockKind, DetectorId, EntranceKind, HttpMethod, Language, Resolution, TriggerStatus,
};
use unfold_core::{Entrance, Key};
use unfold_reader::ReaderIndex;
use unfold_reader::mermaid::entrance_title;
use unfold_reader::notes::note;
use unfold_store::{Application, Membership, Reader};

use super::count;
use crate::error::WorkspaceError;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;

/// Entrance rows per application when the request doesn't say.
const DEFAULT_PER_APPLICATION: u32 = 8;
/// The most entrance rows per application one request may ask for.
pub const MAX_PER_APPLICATION: u32 = 500;
/// The most launches listed per application.
pub const MAX_LAUNCHES: usize = 5;
/// The most unindexed launches listed.
pub const MAX_UNINDEXED_LAUNCHES: usize = 50;

/// The arguments of `entrances`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntrancesArgs {
    /// The project.
    pub project: ProjectId,
    /// Entrance rows per application (8 when absent; 1 to 500).
    #[serde(default)]
    #[ts(optional)]
    pub per_application: Option<u32>,
    /// Only the application with this startup key.
    #[serde(default)]
    #[ts(optional)]
    pub application: Option<String>,
    /// Only entrances whose title, trigger, label, or path contains this text (ignoring case).
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
}

/// One entrance row: a fixed method column, the trigger, and a language tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceRow {
    /// The entrance's key (open its card with it).
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub kind: EntranceKind,
    /// The HTTP method (the method column).
    pub method: Option<HttpMethod>,
    /// The trigger.
    pub trigger: Option<String>,
    /// How much of the trigger is known ("path unknown").
    pub trigger_status: TriggerStatus,
    /// The manual label.
    pub label: Option<String>,
    /// The detector.
    pub detector: DetectorId,
    /// The evidence file.
    pub path: String,
    /// The evidence's first line, for source evidence.
    pub line: Option<u32>,
    /// The evidence file's language (the language tag), for source evidence.
    pub language: Option<Language>,
    /// The first path segment of an HTTP trigger (`/orders/:id` → `orders`).
    pub group: Option<String>,
    /// The definitions in its activation closure; `None` while the project rests.
    pub reaches: Option<u32>,
}

/// One launch (a process entrance): `npm run start`, a console script, a `bin` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRow {
    /// The process entrance's key.
    pub key: String,
    /// Its title.
    pub title: String,
    /// Its trigger (the script or command name).
    pub trigger: Option<String>,
    /// The detector.
    pub detector: DetectorId,
    /// The evidence file (`package.json`, `pyproject.toml`, …).
    pub path: String,
    /// The startup as written.
    pub startup: Option<String>,
    /// Why the startup isn't indexed code (its module or reason), for an unindexed launch.
    pub note: Option<String>,
}

/// An application: a distinct resolved startup and what belongs to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationEntrances {
    /// The startup's key (a file root, or a console script's callable).
    pub startup: String,
    /// The name (`server`, `cli#main`).
    pub name: String,
    /// The startup's language.
    pub language: Language,
    /// The startup's file.
    pub path: String,
    /// The launches that start it (at most 5).
    pub launches: Vec<LaunchRow>,
    /// How many of its entrances match.
    pub total: u32,
    /// The first matching entrances, sorted.
    pub entrances: Vec<EntranceRow>,
}

/// Entrances that belong to no application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnreachedEntrances {
    /// How many match.
    pub total: u32,
    /// The first matching ones, sorted.
    pub entrances: Vec<EntranceRow>,
}

/// How a project starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceMap {
    /// The project.
    pub project: ProjectId,
    /// Applications, ordered by startup key.
    pub applications: Vec<ApplicationEntrances>,
    /// "Not reached from a launch": non-process entrances that belong to no application.
    pub unreached: UnreachedEntrances,
    /// "Launches into code that isn't indexed" (at most 50).
    pub unindexed_launches: Vec<LaunchRow>,
}

/// The stored catalog an entrance map is built from, read in one snapshot.
struct Catalog {
    entrances: Vec<Entrance>,
    applications: Vec<Application>,
    memberships: Vec<Membership>,
}

impl Catalog {
    fn read(reader: &Reader) -> CommandResult<Self> {
        let (entrances, applications, memberships) = reader
            .snapshot(|reader| Ok((reader.entrances()?, reader.applications()?, reader.memberships()?)))?;
        Ok(Self {
            entrances,
            applications,
            memberships,
        })
    }

    /// The application positions each entrance belongs to.
    fn members(&self) -> FxHashMap<&str, Vec<usize>> {
        let by_startup: FxHashMap<&Key, usize> = self
            .applications
            .iter()
            .enumerate()
            .map(|(index, application)| (&application.startup, index))
            .collect();
        let mut members: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
        for membership in &self.memberships {
            if let Some(&index) = by_startup.get(&membership.application) {
                members
                    .entry(membership.entrance.as_str())
                    .or_default()
                    .push(index);
            }
        }
        members
    }

    /// The application a launch starts, when its startup resolved to one.
    fn launched(&self, entrance: &Entrance) -> Option<usize> {
        let target = entrance
            .targets
            .first()
            .filter(|target| target.resolution == Resolution::Resolved)?
            .target
            .as_ref()?;
        self.applications
            .iter()
            .position(|application| application.startup == *target)
    }
}

/// The counts Home's header and Health line show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) applications: u32,
    pub(crate) entrances: u32,
    pub(crate) unreached: u32,
    pub(crate) unindexed_launches: u32,
}

impl Counts {
    /// The counts of the stored graph.
    pub(crate) fn of(reader: &Reader) -> CommandResult<Self> {
        let catalog = Catalog::read(reader)?;
        let members = catalog.members();
        let mut counts = Self {
            applications: count(catalog.applications.len()),
            ..Self::default()
        };
        for entrance in &catalog.entrances {
            if entrance.kind == EntranceKind::Process {
                if catalog.launched(entrance).is_none() {
                    counts.unindexed_launches += 1;
                }
            } else {
                counts.entrances += 1;
                if !members.contains_key(entrance.key.as_str()) {
                    counts.unreached += 1;
                }
            }
        }
        Ok(counts)
    }
}

/// `entrances`: the project's applications with their launches and first entrances, the
/// unreached entrances, and the unindexed launches. Served from the stored graph while the
/// project rests.
pub fn entrances(ctx: &Ctx<'_>, args: EntrancesArgs) -> CommandResult<EntranceMap> {
    let per = args.per_application.unwrap_or(DEFAULT_PER_APPLICATION);
    if !(1..=MAX_PER_APPLICATION).contains(&per) {
        return Err(CommandError::invalid_args(format!(
            "perApplication must be between 1 and {MAX_PER_APPLICATION}"
        )));
    }
    let project = ctx.project(&args.project)?;
    let pool = project.readers()?.ok_or(WorkspaceError::NotCompiled)?;
    let index = match project.live() {
        Some(workspace) => Some(workspace.reader_index()?),
        None => None,
    };
    let reader = pool.get()?;
    let catalog = Catalog::read(&reader)?;
    let members = catalog.members();
    let filter = args
        .filter
        .as_deref()
        .map(str::to_lowercase)
        .filter(|filter| !filter.is_empty());
    let matches = |entrance: &Entrance| {
        filter.as_deref().is_none_or(|filter| {
            [
                Some(entrance_title(entrance).as_str()),
                entrance.trigger.as_deref(),
                entrance.label.as_deref(),
                Some(entrance.evidence.path.as_str()),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_lowercase().contains(filter))
        })
    };
    let wanted = |index: usize| {
        args.application
            .as_deref()
            .is_none_or(|startup| catalog.applications[index].startup.as_str() == startup)
    };
    let mut launches: Vec<Vec<&Entrance>> = vec![Vec::new(); catalog.applications.len()];
    let mut entrances: Vec<Vec<&Entrance>> = vec![Vec::new(); catalog.applications.len()];
    let mut unreached: Vec<&Entrance> = Vec::new();
    let mut unindexed: Vec<&Entrance> = Vec::new();
    for entrance in &catalog.entrances {
        ctx.check_cancelled()?;
        if entrance.kind == EntranceKind::Process {
            match catalog.launched(entrance) {
                Some(index) => launches[index].push(entrance),
                None => unindexed.push(entrance),
            }
            continue;
        }
        if !matches(entrance) {
            continue;
        }
        match members.get(entrance.key.as_str()) {
            Some(indexes) => indexes.iter().for_each(|&index| entrances[index].push(entrance)),
            None => unreached.push(entrance),
        }
    }
    let rows = |mut list: Vec<&Entrance>| -> CommandResult<(u32, Vec<EntranceRow>)> {
        list.sort_by(|a, b| row_order(a, b));
        let total = count(list.len());
        let rows = list
            .into_iter()
            .take(per as usize)
            .map(|entrance| entrance_row(&reader, index.as_deref(), entrance))
            .collect::<CommandResult<_>>()?;
        Ok((total, rows))
    };
    let mut applications = Vec::new();
    for (position, application) in catalog.applications.iter().enumerate() {
        if !wanted(position) {
            continue;
        }
        let path = reader
            .block(application.startup.as_str())?
            .map_or_else(String::new, |located| located.path.as_str().to_owned());
        let (total, list) = rows(std::mem::take(&mut entrances[position]))?;
        applications.push(ApplicationEntrances {
            startup: application.startup.as_str().to_owned(),
            name: application.name.to_string(),
            language: application.language,
            path,
            launches: launches[position]
                .iter()
                .take(MAX_LAUNCHES)
                .map(|launch| launch_row(launch, false))
                .collect(),
            total,
            entrances: list,
        });
    }
    let whole_map = args.application.is_none();
    let (total, list) = if whole_map {
        rows(unreached)?
    } else {
        (0, Vec::new())
    };
    Ok(EntranceMap {
        project: args.project,
        applications,
        unreached: UnreachedEntrances {
            total,
            entrances: list,
        },
        unindexed_launches: unindexed
            .into_iter()
            .filter(|_| whole_map)
            .take(MAX_UNINDEXED_LAUNCHES)
            .map(|launch| launch_row(launch, true))
            .collect(),
    })
}

/// The group of an HTTP trigger: its first path segment.
fn group(entrance: &Entrance) -> Option<&str> {
    if entrance.kind != EntranceKind::Http {
        return None;
    }
    entrance
        .trigger
        .as_deref()?
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty())
}

/// Where a method sorts: GET, POST, PUT, PATCH, DELETE, then the rest.
fn method_rank(method: Option<HttpMethod>) -> usize {
    match method {
        Some(HttpMethod::Get) => 0,
        Some(HttpMethod::Post) => 1,
        Some(HttpMethod::Put) => 2,
        Some(HttpMethod::Patch) => 3,
        Some(HttpMethod::Delete) => 4,
        _ => 5,
    }
}

/// The rows' order. See the [module documentation](self).
fn row_order(a: &Entrance, b: &Entrance) -> Ordering {
    let http = |entrance: &Entrance| entrance.kind != EntranceKind::Http;
    http(a).cmp(&http(b)).then_with(|| {
        if a.kind == EntranceKind::Http && b.kind == EntranceKind::Http {
            let (a_group, b_group) = (group(a), group(b));
            (a_group.is_none(), a_group)
                .cmp(&(b_group.is_none(), b_group))
                .then_with(|| a.trigger.cmp(&b.trigger))
                .then_with(|| method_rank(a.method).cmp(&method_rank(b.method)))
                .then_with(|| a.method.cmp(&b.method))
                .then_with(|| a.key.as_str().cmp(b.key.as_str()))
        } else {
            a.kind
                .as_str()
                .cmp(b.kind.as_str())
                .then_with(|| entrance_title(a).cmp(&entrance_title(b)))
                .then_with(|| a.key.as_str().cmp(b.key.as_str()))
        }
    })
}

fn entrance_row(
    reader: &Reader,
    index: Option<&ReaderIndex>,
    entrance: &Entrance,
) -> CommandResult<EntranceRow> {
    let (line, language) = match &entrance.evidence.block {
        Some(block) => (
            reader
                .block(block.as_str())?
                .map(|located| located.block.lines.start),
            reader
                .file(entrance.evidence.path.as_str())?
                .map(|row| row.language),
        ),
        None => (None, None),
    };
    let reaches = index.map(|index| {
        let graph = index.index();
        let closure = index.entrance_closure(entrance.key.as_str()).unwrap_or_default();
        count(
            closure
                .iter()
                .filter_map(|key| graph.id(key.as_str()))
                .filter(|&id| {
                    let kind = graph.kind(id);
                    kind.is_function_like() || kind == BlockKind::Class
                })
                .count(),
        )
    });
    Ok(EntranceRow {
        key: entrance.key.as_str().to_owned(),
        title: entrance_title(entrance),
        kind: entrance.kind,
        method: entrance.method,
        trigger: entrance.trigger.as_deref().map(str::to_owned),
        trigger_status: entrance.trigger_status,
        label: entrance.label.as_deref().map(str::to_owned),
        detector: entrance.detector,
        path: entrance.evidence.path.as_str().to_owned(),
        line,
        language,
        group: group(entrance).map(str::to_owned),
        reaches,
    })
}

fn launch_row(entrance: &Entrance, unindexed: bool) -> LaunchRow {
    let startup = entrance.targets.first();
    LaunchRow {
        key: entrance.key.as_str().to_owned(),
        title: entrance_title(entrance),
        trigger: entrance.trigger.as_deref().map(str::to_owned),
        detector: entrance.detector,
        path: entrance.evidence.path.as_str().to_owned(),
        startup: startup.map(|target| target.text.to_string()),
        note: startup
            .filter(|_| unindexed)
            .and_then(|target| note(target.resolution, target.reason, target.module.as_ref())),
    }
}
