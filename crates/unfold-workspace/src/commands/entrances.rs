//! `entrance_tree`: the Entrances view (architecture §11.3). Applications are named after their
//! startup, with their launches underneath, followed by their entrances in canonical order;
//! then the entrances no launch reaches, and the launches into code that isn't indexed.

use rustc_hash::FxHashMap;
use serde::Serialize;
use ts_rs::TS;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Language, Resolution, TriggerStatus};
use unfold_core::{Entrance, Key};
use unfold_reader::mermaid::entrance_title;
use unfold_reader::notes::note;
use unfold_store::Reader;

use super::NoArgs;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;

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
pub struct ApplicationNode {
    /// The startup's key (a file root, or a console script's callable).
    pub startup: String,
    /// The name (`server`, `cli#main`).
    pub name: String,
    /// The startup's language.
    pub language: Language,
    /// The startup's file.
    pub path: String,
    /// The launches that start it.
    pub launches: Vec<LaunchRow>,
    /// The non-process entrances that belong to it, in canonical order.
    pub entrances: Vec<EntranceRow>,
}

/// The Entrances view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceTree {
    /// Applications, ordered by startup key.
    pub applications: Vec<ApplicationNode>,
    /// "Not reached from a launch": non-process entrances that belong to no application.
    pub unreached: Vec<EntranceRow>,
    /// "Launches into code that isn't indexed": launches whose startup is external or
    /// unresolved.
    pub unindexed_launches: Vec<LaunchRow>,
}

/// `entrance_tree`: the whole Entrances view (entrances are few; it is one small payload).
pub fn entrance_tree(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<EntranceTree> {
    let workspace = ctx.workspace()?;
    let reader = workspace.readers()?.get()?;
    let (entrances, stored_applications, memberships) =
        reader.snapshot(|reader| Ok((reader.entrances()?, reader.applications()?, reader.memberships()?)))?;
    let mut applications = Vec::with_capacity(stored_applications.len());
    let mut by_startup: FxHashMap<Key, usize> = FxHashMap::default();
    for application in &stored_applications {
        let path = reader
            .block(application.startup.as_str())?
            .map_or_else(String::new, |located| located.path.as_str().to_owned());
        by_startup.insert(application.startup.clone(), applications.len());
        applications.push(ApplicationNode {
            startup: application.startup.as_str().to_owned(),
            name: application.name.to_string(),
            language: application.language,
            path,
            launches: Vec::new(),
            entrances: Vec::new(),
        });
    }
    let mut member_of: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
    for membership in &memberships {
        if let Some(&index) = by_startup.get(&membership.application) {
            member_of
                .entry(membership.entrance.as_str())
                .or_default()
                .push(index);
        }
    }
    let mut unreached = Vec::new();
    let mut unindexed_launches = Vec::new();
    for entrance in &entrances {
        ctx.check_cancelled()?;
        if entrance.kind == EntranceKind::Process {
            let startup = entrance.targets.first();
            let resolved = startup
                .filter(|target| target.resolution == Resolution::Resolved)
                .and_then(|target| target.target.as_ref())
                .and_then(|key| by_startup.get(key));
            let launch = launch_row(entrance, resolved.is_none());
            match resolved {
                Some(&index) => applications[index].launches.push(launch),
                None => unindexed_launches.push(launch),
            }
            continue;
        }
        let row = entrance_row(&reader, entrance)?;
        match member_of.get(entrance.key.as_str()) {
            Some(indexes) => {
                for &index in indexes {
                    applications[index].entrances.push(row.clone());
                }
            }
            None => unreached.push(row),
        }
    }
    Ok(EntranceTree {
        applications,
        unreached,
        unindexed_launches,
    })
}

fn entrance_row(reader: &Reader, entrance: &Entrance) -> CommandResult<EntranceRow> {
    let line = match &entrance.evidence.block {
        Some(block) => reader
            .block(block.as_str())?
            .map(|located| located.block.lines.start),
        None => None,
    };
    let language = match &entrance.evidence.block {
        Some(_) => reader
            .file(entrance.evidence.path.as_str())?
            .map(|row| row.language),
        None => None,
    };
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
