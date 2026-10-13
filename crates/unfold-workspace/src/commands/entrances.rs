//! `entrances`: the entrance catalog (docs/reader.md#derived-views), for Home's "How it starts"
//! and Add's entrance browser.
//!
//! The overview lists the applications by category (needs a look, services, scheduled, workers)
//! with their launches; a scope (one application, or the entrances no launch reaches) lists
//! entrance rows, filtered, then paged with a cursor. Totals count unique declarations: a route
//! two applications mount from one router at one path is one route; a row or application shows
//! the other applications that mount its registration as "also in".
//!
//! Scope rows come in sections: the launches; then routes by the first segment of their path,
//! split where they are mounted from a router file other than their application's (sections
//! without a segment last); then workers, schedules, commands, and events. Routes sort by path
//! and method (GET, POST, PUT, PATCH, DELETE, then the rest); the other rows by title. A resting
//! project is served from its stored catalog; `reaches` (the functions and classes an
//! entrance's activation closure holds) needs the live project's reader index and is `None`
//! while it rests.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::Path;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind, HttpMethod, Resolution};
use unfold_core::{Entrance, Key, Launch, LaunchMode, Launches, RepoPath};
use unfold_engine::discovery::{DiscoverOptions, IgnoreRules, is_default_excluded};
use unfold_engine::settings::{ScopeStatus, Settings};
use unfold_reader::ReaderIndex;
use unfold_reader::launches::{LaunchState, folder_label, launch_state, schedule_gloss, start_kind};
use unfold_reader::mermaid::entrance_title;
use unfold_reader::notes::reason_text;
use unfold_store::{Application, Basis, Membership, Reader};

use super::count;
use crate::error::WorkspaceError;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::settings_edit::MatchSpec;

/// Rows per page when the request doesn't say.
const DEFAULT_LIMIT: u32 = 100;
/// The most rows one page may hold.
pub const MAX_LIMIT: u32 = 500;

/// The arguments of `entrances`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntrancesArgs {
    /// The project.
    pub project: ProjectId,
    /// The rows of the application with this ID.
    #[serde(default)]
    #[ts(optional)]
    pub application: Option<String>,
    /// The rows of the entrances no launch reaches.
    #[serde(default)]
    #[ts(optional)]
    pub unreached: Option<bool>,
    /// Only rows whose title, trigger, handler, or path contains this text (ignoring case).
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
    /// Rows per page (100 when absent; 1 to 500).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Where the page starts: the previous page's `nextCursor`.
    #[serde(default)]
    #[ts(optional)]
    pub cursor: Option<String>,
}

/// What `entrances` returns: the overview, or one scope's rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "view", rename_all = "snake_case")]
pub enum EntrancesView {
    /// Without `application` or `unreached`.
    Overview(EntranceOverview),
    /// With `application` or `unreached`.
    Scope(EntranceScope),
}

/// The catalog without entrance rows: applications by category, with their launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceOverview {
    /// The project.
    pub project: ProjectId,
    /// The totals.
    pub counts: CatalogCounts,
    /// The categories that have applications, in display order.
    pub categories: Vec<CategoryGroup>,
    /// "Not reached from a launch": entrances other than launches that belong to no application.
    pub unreached: u32,
}

/// The catalog's totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCounts {
    /// Applications.
    pub applications: u32,
    /// Launches (process entrances).
    pub launches: u32,
    /// Unique route declarations.
    pub routes: u32,
    /// Unique launches and unique other entrances, unreached ones included.
    pub entrances: u32,
}

/// An application category, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// "Needs a look": some launch can't be read and has no target set by hand.
    NeedsLook,
    /// "Services": it has routes or serves HTTP.
    Services,
    /// "Workers": everything else.
    Workers,
    /// "Scheduled": some launch has a schedule, or it has scheduled handlers.
    Scheduled,
}

/// One category's applications, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategoryGroup {
    /// The category.
    pub category: Category,
    /// Its applications.
    pub applications: Vec<AppRow>,
}

/// Where an application comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AppOrigin {
    /// An `[[application]]` table.
    Declared,
    /// Launches that start the same code.
    Inferred,
}

/// One application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppRow {
    /// Its ID: the `[[application]]` ID, or the startup's key for an inferred one. `entrances`
    /// takes it as `application`.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its description.
    pub description: Option<String>,
    /// Where it comes from.
    pub origin: AppOrigin,
    /// Its launches and unique handler entrances (Add's count).
    pub entrances: u32,
    /// Its unique route declarations.
    pub routes: u32,
    /// The other applications that serve some of its routes, by name.
    pub also_in: Vec<String>,
    /// Its launches; empty for "No launch defined".
    pub launches: Vec<LaunchRow>,
}

/// One launch (a process entrance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRow {
    /// The process entrance's key.
    pub key: String,
    /// The `[[entrance]]` ID, for a configured launch.
    pub id: Option<String>,
    /// The label.
    pub label: Option<String>,
    /// The folder it runs in (`data-pipelines`, or the project's name for its folder).
    pub folder: String,
    /// The command: as written for a configured launch, `npm run start`, a bin, or a console
    /// script for a detected one. `None` when a configured launch has only a target.
    pub command: Option<String>,
    /// The schedule, as written.
    pub schedule: Option<String>,
    /// The schedule in words (`daily at 07:30`), for a standard five-field cron.
    pub schedule_gloss: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// Whether Unfold can follow it into the code.
    pub status: LaunchStatus,
    /// Why it can't be read, in a few words (`Can't read sh -c`).
    pub problem: Option<String>,
    /// More about the status (`evals/ is in .gitignore, so this code isn't indexed`).
    pub note: Option<String>,
    /// Whether its target was set by hand.
    pub target_set_by_hand: bool,
    /// What it starts, when that is indexed code.
    pub starts: Option<LaunchStart>,
    /// Its Python import path, in search order.
    pub import_paths: Vec<ImportPathRow>,
    /// The functions and classes it reaches; `None` while the project rests.
    pub reaches: Option<u32>,
    /// The unique routes it serves; `None` when it serves none.
    pub routes: Option<u32>,
    /// Where it is declared.
    pub declared_in: DeclaredIn,
    /// How it is edited.
    pub editable: Editable,
    /// The detected launch an `[[entrance_override]]` matches, for a detected launch.
    #[serde(rename = "match")]
    #[ts(rename = "match")]
    pub match_: Option<MatchSpec>,
}

/// Whether Unfold can follow a launch into the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStatus {
    /// It starts indexed code.
    Ok,
    /// Its command can't be read.
    Unreadable,
    /// It starts code that isn't indexed.
    NotIndexed,
    /// What it starts doesn't exist.
    NotFound,
}

/// What a launch starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStart {
    /// The file.
    pub path: String,
    /// The binding or callable it names (`app`), if any.
    pub name: Option<String>,
    /// What it is (`FastAPI application`, `Python script`, `Python module`, `Node file`).
    pub kind: String,
}

/// One folder on a launch's import path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportPathRow {
    /// The project-relative folder (`.` for the project's folder).
    pub path: String,
    /// Why it is there (`from --app-dir`, `the working folder`, `set by hand`).
    pub origin: String,
}

/// Where a launch is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredIn {
    /// The file (`unfold.toml`, `apps/api/package.json`).
    pub path: String,
    /// The first line of its declaration.
    pub line: Option<u32>,
}

/// How a launch is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Editable {
    /// Its `[[entrance]]` table.
    Configured,
    /// An `[[entrance_override]]` of a detected launch.
    Override,
}

/// One scope's rows (an application, or the unreached entrances), filtered and paged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceScope {
    /// The project.
    pub project: ProjectId,
    /// Every section of the filtered rows, in order; rows refer to them by position.
    pub sections: Vec<EntranceSection>,
    /// This page's rows, in order.
    pub rows: Vec<CatalogRow>,
    /// How many rows match.
    pub total: u32,
    /// The next page's cursor, when there is one.
    pub next_cursor: Option<String>,
}

/// What a section holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// The launches.
    Launch,
    /// Routes under one first path segment (and one mount).
    Http,
    /// Workers.
    Worker,
    /// Schedules.
    Schedule,
    /// CLI commands.
    Command,
    /// Events.
    Event,
}

/// A run of rows under one heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntranceSection {
    /// The heading (`Launch`, `/runs`, `Jobs and commands`).
    pub title: String,
    /// What it holds.
    pub kind: SectionKind,
    /// The router file its routes are mounted from, when that isn't the application's file.
    pub mounted_from: Option<String>,
    /// The other applications that serve its routes, by name.
    pub also_in: Vec<String>,
    /// How many of the filtered rows it holds.
    pub count: u32,
}

/// One entrance row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRow {
    /// The entrance's key (open its card with it).
    pub key: String,
    /// The `[[entrance]]` ID, for a configured entrance.
    pub id: Option<String>,
    /// Its section's position in `sections`.
    pub section: u32,
    /// Its kind.
    pub kind: EntranceKind,
    /// The HTTP method.
    pub method: Option<HttpMethod>,
    /// The trigger.
    pub trigger: Option<String>,
    /// Its title.
    pub title: String,
    /// The first resolved handler's name (`create_run`).
    pub handler: Option<String>,
    /// The evidence file.
    pub path: String,
    /// The evidence's first line, for source evidence.
    pub line: Option<u32>,
    /// The functions and classes it reaches; `None` while the project rests.
    pub reaches: Option<u32>,
    /// The other applications it belongs to, by name.
    pub also_in: Vec<String>,
}

/// The stored catalog, read in one snapshot, with the lookups every view needs.
pub(crate) struct Catalog {
    pub(crate) entrances: Vec<Entrance>,
    pub(crate) applications: Vec<Application>,
    pub(crate) launches: Launches,
    /// Each entrance's applications, by entrance position (application positions).
    pub(crate) apps_of: Vec<Vec<usize>>,
    /// Each application's members, by application position (entrance positions, in canonical
    /// order).
    members: Vec<Vec<usize>>,
    /// Each application's proven members, by application position (a subset of its members).
    proven: Vec<Vec<usize>>,
    /// Each entrance's declaration, by entrance position: entrances with equal numbers are one
    /// declaration (a route two applications mount from one router at the same path).
    pub(crate) declaration: Vec<u32>,
    /// Each entrance's registration, by entrance position: like its declaration, whatever path
    /// it is mounted at.
    pub(crate) registration: Vec<u32>,
}

impl Catalog {
    pub(crate) fn read(reader: &Reader) -> CommandResult<Self> {
        let (entrances, applications, memberships, launches) = reader.snapshot(|reader| {
            Ok((
                reader.entrances()?,
                reader.applications()?,
                reader.memberships()?,
                reader.launches()?,
            ))
        })?;
        Ok(Self::new(entrances, applications, &memberships, launches))
    }

    fn new(
        entrances: Vec<Entrance>,
        applications: Vec<Application>,
        memberships: &[Membership],
        launches: Launches,
    ) -> Self {
        let position: FxHashMap<&str, usize> = entrances
            .iter()
            .enumerate()
            .map(|(index, entrance)| (entrance.key.as_str(), index))
            .collect();
        let app: FxHashMap<&str, usize> = applications
            .iter()
            .enumerate()
            .map(|(index, application)| (&*application.id, index))
            .collect();
        let mut apps_of = vec![Vec::new(); entrances.len()];
        let mut members = vec![Vec::new(); applications.len()];
        let mut proven = vec![Vec::new(); applications.len()];
        for membership in memberships {
            if let (Some(&entrance), Some(&application)) = (
                position.get(membership.entrance.as_str()),
                app.get(&*membership.application),
            ) {
                apps_of[entrance].push(application);
                members[application].push(entrance);
                if membership.basis == Basis::Proven {
                    proven[application].push(entrance);
                }
            }
        }
        members.iter_mut().for_each(|list| list.sort_unstable());
        proven.iter_mut().for_each(|list| list.sort_unstable());
        // A registration is the evidence and method; a declaration adds the trigger.
        let mut registrations: FxHashMap<(&str, u32, u32, &str, Option<HttpMethod>), u32> =
            FxHashMap::default();
        let mut declarations: FxHashMap<(u32, Option<&str>), u32> = FxHashMap::default();
        let mut declaration = Vec::with_capacity(entrances.len());
        let mut registration = Vec::with_capacity(entrances.len());
        for entrance in &entrances {
            let evidence = &entrance.evidence;
            let registered = (
                evidence.path.as_str(),
                evidence.range.start,
                evidence.range.end,
                entrance.detector.as_str(),
                entrance.method,
            );
            let next = count(registrations.len());
            let registered = *registrations.entry(registered).or_insert(next);
            registration.push(registered);
            let next = count(declarations.len());
            declaration.push(
                *declarations
                    .entry((registered, entrance.trigger.as_deref()))
                    .or_insert(next),
            );
        }
        Self {
            entrances,
            applications,
            launches,
            apps_of,
            members,
            proven,
            declaration,
            registration,
        }
    }

    /// The launch record of the entrance at `position`.
    fn record(&self, position: usize) -> Option<&Launch> {
        let key = &self.entrances[position].key;
        self.launches
            .launches
            .iter()
            .find(|launch| launch.entrance == *key)
    }

    /// How many distinct declarations `positions` hold.
    pub(crate) fn unique(&self, positions: impl IntoIterator<Item = usize>) -> u32 {
        let set: BTreeSet<u32> = positions
            .into_iter()
            .map(|position| self.declaration[position])
            .collect();
        count(set.len())
    }

    /// The names of the applications other than `application` that hold the registration of the
    /// entrance at `position` (mounted at the same path or another), sorted.
    fn also_in(&self, position: usize, application: Option<usize>) -> Vec<String> {
        let registration = self.registration[position];
        let mut names: Vec<String> = self
            .registration
            .iter()
            .enumerate()
            .filter(|&(_, &other)| other == registration)
            .flat_map(|(other, _)| self.apps_of[other].iter().copied())
            .filter(|&app| Some(app) != application)
            .map(|app| self.applications[app].name.to_string())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The positions of entrances other than launches that belong to no application.
    fn unreached(&self) -> Vec<usize> {
        (0..self.entrances.len())
            .filter(|&position| {
                self.entrances[position].kind != EntranceKind::Process && self.apps_of[position].is_empty()
            })
            .collect()
    }

    pub(crate) fn is_route(&self, position: usize) -> bool {
        self.entrances[position].kind == EntranceKind::Http
    }

    /// Whether an ASGI launch serves the application bound at `binding`.
    fn serves_asgi(&self, binding: &Key) -> bool {
        self.launches.launches.iter().any(|launch| {
            launch.mode == Some(LaunchMode::PythonAsgi)
                && self
                    .entrances
                    .iter()
                    .find(|entrance| entrance.key == launch.entrance)
                    .and_then(|entrance| entrance.targets.first())
                    .and_then(|target| target.target.as_ref())
                    == Some(binding)
        })
    }
}

/// What rows and launch rows read besides the catalog.
pub(crate) struct Context<'a> {
    pub(crate) reader: &'a Reader,
    /// The live project's reader index; `None` while it rests.
    pub(crate) index: Option<&'a ReaderIndex>,
    /// The project's shared name, for its folder.
    pub(crate) project: &'a str,
    /// The project's folder, settings, and discovery options, for why code isn't indexed.
    pub(crate) root: &'a Path,
    pub(crate) settings: &'a Settings,
    pub(crate) discover: &'a DiscoverOptions,
}

impl Context<'_> {
    /// The functions and classes the entrance with `key` reaches, when the project is live.
    fn reaches(&self, key: &str) -> Option<u32> {
        let index = self.index?;
        Some(definitions(
            index,
            &index.entrance_closure(key).unwrap_or_default(),
        ))
    }

    /// The functions and classes a launch reaches with the handlers of the routes it serves,
    /// when the project is live.
    fn launch_reaches(&self, entrance: &Entrance, routes: &[&Entrance]) -> Option<u32> {
        let index = self.index?;
        let mut seeds: Vec<Key> = index.entrance_closure(entrance.key.as_str()).unwrap_or_default();
        seeds.extend(
            routes
                .iter()
                .flat_map(|route| &route.targets)
                .filter_map(|target| target.target.clone()),
        );
        let seeds: Vec<&str> = seeds.iter().map(Key::as_str).collect();
        Some(definitions(index, &index.closure(&seeds)))
    }

    /// The row of the launch at `position`.
    pub(crate) fn launch_row(&self, catalog: &Catalog, position: usize) -> CommandResult<LaunchRow> {
        let entrance = &catalog.entrances[position];
        let record = catalog.record(position);
        let state = launch_state(entrance, record);
        let start = entrance.targets.first();
        let text = start.map_or("", |target| &*target.text);
        let (problem, note) = match state {
            LaunchState::Ok => (None, None),
            LaunchState::Unreadable => match record.and_then(|launch| launch.problem.as_ref()) {
                Some(problem) => (Some(problem.label.to_string()), Some(problem.message.to_string())),
                None => (
                    Some(format!("Can't resolve {text}")),
                    start
                        .and_then(|target| target.reason)
                        .map(|reason| reason_text(reason).to_owned()),
                ),
            },
            LaunchState::NotIndexed => (
                None,
                Some(unindexed_note(
                    self.root,
                    self.settings,
                    self.discover,
                    &code_path(entrance, record, text),
                )),
            ),
            LaunchState::NotFound => (None, Some(format!("{text} wasn't found"))),
        };
        let startup = start
            .filter(|target| target.resolution == Resolution::Resolved)
            .and_then(|target| target.target.as_ref());
        let bound: Vec<usize> = startup.map_or_else(Vec::new, |startup| {
            (0..catalog.entrances.len())
                .filter(|&other| {
                    catalog.is_route(other) && catalog.entrances[other].binding.as_ref() == Some(startup)
                })
                .collect()
        });
        // An ASGI launch serves the routes bound to its application; any other launch, the
        // routes its application proves it reaches (membership is per application), except those
        // an ASGI launch serves: a cron sharing an API's application serves none of its routes.
        let served: Vec<usize> = if record.and_then(|launch| launch.mode) == Some(LaunchMode::PythonAsgi) {
            bound.clone()
        } else {
            let mut served: Vec<usize> = catalog.apps_of[position]
                .iter()
                .flat_map(|&application| catalog.proven[application].iter().copied())
                .filter(|&other| {
                    catalog.is_route(other)
                        && !catalog.entrances[other]
                            .binding
                            .as_ref()
                            .is_some_and(|binding| catalog.serves_asgi(binding))
                })
                .collect();
            served.sort_unstable();
            served.dedup();
            served
        };
        let routes = catalog.unique(served.iter().copied());
        let starts = match startup {
            Some(key) => self.reader.block(key.as_str())?.map(|located| LaunchStart {
                path: located.path.as_str().to_owned(),
                name: (located.block.kind != BlockKind::File)
                    .then(|| located.block.name.as_deref().map(str::to_owned))
                    .flatten(),
                kind: start_kind(
                    record.and_then(|launch| launch.mode),
                    located.language,
                    !bound.is_empty(),
                )
                .to_owned(),
            }),
            None => None,
        };
        let (detector, evidence) = (entrance.detector, &entrance.evidence);
        Ok(LaunchRow {
            key: entrance.key.as_str().to_owned(),
            id: entrance.key.configured_id().map(str::to_owned),
            label: record
                .and_then(|launch| launch.label.as_deref())
                .map(str::to_owned),
            folder: folder_label(record.map_or(".", |launch| &launch.folder), self.project),
            command: record
                .and_then(|launch| launch.command.as_deref())
                .map(str::to_owned),
            schedule: record
                .and_then(|launch| launch.schedule.as_deref())
                .map(str::to_owned),
            schedule_gloss: record
                .and_then(|launch| launch.schedule.as_deref())
                .and_then(schedule_gloss),
            description: record
                .and_then(|launch| launch.description.as_deref())
                .map(str::to_owned),
            status: match state {
                LaunchState::Ok => LaunchStatus::Ok,
                LaunchState::Unreadable => LaunchStatus::Unreadable,
                LaunchState::NotIndexed => LaunchStatus::NotIndexed,
                LaunchState::NotFound => LaunchStatus::NotFound,
            },
            problem,
            note,
            target_set_by_hand: record.is_some_and(|launch| launch.target_set_by_hand),
            starts,
            import_paths: record.map_or_else(Vec::new, |launch| {
                launch
                    .import_paths
                    .iter()
                    .map(|path| ImportPathRow {
                        path: path.path.to_string(),
                        origin: path.origin.to_string(),
                    })
                    .collect()
            }),
            reaches: self.launch_reaches(
                entrance,
                &served
                    .iter()
                    .map(|&route| &catalog.entrances[route])
                    .collect::<Vec<_>>(),
            ),
            routes: (routes > 0).then_some(routes),
            declared_in: DeclaredIn {
                path: evidence.path.as_str().to_owned(),
                line: record.map(|launch| launch.line),
            },
            editable: if detector == DetectorId::Manual {
                Editable::Configured
            } else {
                Editable::Override
            },
            match_: (detector != DetectorId::Manual).then(|| MatchSpec {
                detector: detector.as_str().to_owned(),
                path: evidence.path.as_str().to_owned(),
                pointer: evidence.pointer.as_deref().unwrap_or_default().to_owned(),
            }),
        })
    }
}

/// How many functions and classes `keys` hold.
fn definitions(index: &ReaderIndex, keys: &[Key]) -> u32 {
    let graph = index.index();
    count(
        keys.iter()
            .filter_map(|key| graph.id(key.as_str()))
            .filter(|&id| {
                let kind = graph.kind(id);
                kind.is_function_like() || kind == BlockKind::Class
            })
            .count(),
    )
}

/// Why code at `path` isn't indexed: the shallowest folder on its way that the default
/// exclusions, ignore files, or `[scope]` leave out, or its being JavaScript.
pub(crate) fn unindexed_note(
    root: &Path,
    settings: &Settings,
    discover: &DiscoverOptions,
    path: &str,
) -> String {
    let mut ignore = IgnoreRules::new(root, discover);
    let mut folder = String::new();
    for part in path.split('/') {
        if !folder.is_empty() {
            folder.push('/');
        }
        folder.push_str(part);
        let Ok(relative) = RepoPath::new(&folder) else {
            break;
        };
        let why = if is_default_excluded(&folder) {
            Some("is always left out")
        } else if ignore.is_ignored(&relative, true) {
            Some("is in .gitignore")
        } else if settings.scope.status(&relative) == ScopeStatus::Excluded {
            Some("is excluded in unfold.toml")
        } else {
            None
        };
        if let Some(why) = why {
            return format!("{folder}/ {why}, so this code isn't indexed");
        }
    }
    let javascript = path
        .rsplit_once('.')
        .is_some_and(|(_, extension)| matches!(extension, "js" | "jsx" | "mjs" | "cjs"));
    if javascript {
        return format!("{path} is JavaScript, which isn't indexed");
    }
    "This code isn't indexed".to_owned()
}

/// The project file or folder a launch that isn't indexed names: its file, or for a Python
/// target the module under its first import folder.
fn code_path(entrance: &Entrance, record: Option<&Launch>, text: &str) -> String {
    let Some(launch) = record else {
        return text.to_owned();
    };
    let join = |folder: &str, path: &str| {
        if folder == "." {
            path.to_owned()
        } else {
            format!("{folder}/{path}")
        }
    };
    match launch.mode {
        Some(LaunchMode::File) | None if entrance.detector == DetectorId::Manual => text.to_owned(),
        Some(LaunchMode::File) | None => join(&launch.folder, text),
        Some(_) => {
            let module = text.split(':').next().unwrap_or(text).replace('.', "/");
            let folder = launch
                .import_paths
                .first()
                .map_or(&*launch.folder, |path| &*path.path);
            join(folder, &module)
        }
    }
}

/// `entrances`: the overview, or one scope's rows. Served from the stored catalog while the
/// project rests.
pub fn entrances(ctx: &Ctx<'_>, args: EntrancesArgs) -> CommandResult<EntrancesView> {
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(CommandError::invalid_args(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    let unreached = args.unreached.unwrap_or(false);
    if unreached && args.application.is_some() {
        return Err(CommandError::invalid_args(
            "ask for one application or the unreached entrances, not both",
        ));
    }
    let project = ctx.project(&args.project)?;
    let pool = project.readers()?.ok_or(WorkspaceError::NotCompiled)?;
    let live = project.live();
    let index = match &live {
        Some(workspace) => Some(workspace.reader_index()?),
        None => None,
    };
    let settings = match &live {
        Some(workspace) => workspace.settings(),
        None => crate::workspace::load_settings(project.root()),
    }
    .unwrap_or_default();
    let discover = live.as_ref().map_or_else(DiscoverOptions::default, |workspace| {
        workspace.options().engine.discover.clone()
    });
    let reader = pool.get()?;
    let catalog = Catalog::read(&reader)?;
    let context = Context {
        reader: &reader,
        index: index.as_deref(),
        project: project.name(),
        root: project.root(),
        settings: &settings,
        discover: &discover,
    };
    if !unreached && args.application.is_none() {
        return Ok(EntrancesView::Overview(overview(
            ctx,
            &context,
            &catalog,
            args.project,
        )?));
    }
    let (application, positions) = match &args.application {
        Some(id) => {
            let application = catalog
                .applications
                .iter()
                .position(|application| *application.id == **id)
                .ok_or_else(|| CommandError::not_found(format!("no application `{id}`")))?;
            (Some(application), catalog.members[application].clone())
        }
        None => (None, catalog.unreached()),
    };
    let scope = Scope {
        context: &context,
        catalog: &catalog,
        application,
    };
    let filter = args
        .filter
        .as_deref()
        .map(str::to_lowercase)
        .filter(|filter| !filter.is_empty());
    let mut rows: Vec<Placed> = Vec::with_capacity(positions.len());
    for position in positions {
        ctx.check_cancelled()?;
        let placed = scope.place(position)?;
        if filter.as_deref().is_none_or(|filter| placed.matches(filter)) {
            rows.push(placed);
        }
    }
    rows.sort_by(|a, b| a.order(b));
    let mut sections: Vec<EntranceSection> = Vec::new();
    let mut section_of: Vec<u32> = Vec::with_capacity(rows.len());
    for row in &rows {
        let same = sections.last().is_some_and(|last| {
            last.title == row.section.title && last.mounted_from == row.section.mounted_from
        });
        if !same {
            sections.push(row.section.clone());
        }
        let last = sections.last_mut().expect("a section was just pushed");
        last.count += 1;
        for name in &row.row.also_in {
            if !last.also_in.contains(name) {
                last.also_in.push(name.clone());
            }
        }
        section_of.push(count(sections.len() - 1));
    }
    sections.iter_mut().for_each(|section| section.also_in.sort());
    let start = match &args.cursor {
        None => 0,
        Some(cursor) => {
            rows.iter()
                .position(|row| row.row.key == *cursor)
                .ok_or_else(|| CommandError::invalid_args("the cursor is out of date"))?
                + 1
        }
    };
    let total = count(rows.len());
    let end = (start + limit as usize).min(rows.len());
    let next_cursor = (end < rows.len()).then(|| rows[end - 1].row.key.clone());
    let page = rows
        .into_iter()
        .zip(section_of)
        .skip(start)
        .take(end - start)
        .map(|(placed, section)| scope.finish(placed, section))
        .collect::<CommandResult<_>>()?;
    Ok(EntrancesView::Scope(EntranceScope {
        project: args.project,
        sections,
        rows: page,
        total,
        next_cursor,
    }))
}

fn overview(
    ctx: &Ctx<'_>,
    context: &Context<'_>,
    catalog: &Catalog,
    project: ProjectId,
) -> CommandResult<EntranceOverview> {
    let mut order: Vec<usize> = (0..catalog.applications.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&catalog.applications[a], &catalog.applications[b]);
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut categories: Vec<CategoryGroup> = Vec::new();
    for position in order {
        ctx.check_cancelled()?;
        let application = &catalog.applications[position];
        let members = &catalog.members[position];
        let launches: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&member| catalog.entrances[member].kind == EntranceKind::Process)
            .collect();
        let others = members
            .iter()
            .copied()
            .filter(|&member| catalog.entrances[member].kind != EntranceKind::Process);
        let mut also_in: Vec<String> = members
            .iter()
            .filter(|&&member| catalog.is_route(member))
            .flat_map(|&member| catalog.also_in(member, Some(position)))
            .collect();
        also_in.sort();
        also_in.dedup();
        let row = AppRow {
            id: application.id.to_string(),
            name: application.name.to_string(),
            description: application.description.as_deref().map(str::to_owned),
            origin: if application.declared {
                AppOrigin::Declared
            } else {
                AppOrigin::Inferred
            },
            entrances: count(launches.len()) + catalog.unique(others),
            routes: catalog.unique(members.iter().copied().filter(|&member| catalog.is_route(member))),
            also_in,
            launches: launches
                .iter()
                .map(|&launch| context.launch_row(catalog, launch))
                .collect::<CommandResult<_>>()?,
        };
        let category = category(application.category);
        match categories.last_mut() {
            Some(group) if group.category == category => group.applications.push(row),
            _ => categories.push(CategoryGroup {
                category,
                applications: vec![row],
            }),
        }
    }
    let all = 0..catalog.entrances.len();
    Ok(EntranceOverview {
        project,
        counts: CatalogCounts {
            applications: count(catalog.applications.len()),
            launches: count(
                all.clone()
                    .filter(|&position| catalog.entrances[position].kind == EntranceKind::Process)
                    .count(),
            ),
            routes: catalog.unique(all.clone().filter(|&position| catalog.is_route(position))),
            entrances: catalog.unique(all),
        },
        categories,
        unreached: catalog.unique(catalog.unreached()),
    })
}

fn category(category: unfold_store::Category) -> Category {
    match category {
        unfold_store::Category::NeedsLook => Category::NeedsLook,
        unfold_store::Category::Services => Category::Services,
        unfold_store::Category::Workers => Category::Workers,
        unfold_store::Category::Scheduled => Category::Scheduled,
    }
}

/// One scope: an application's members, or the unreached entrances.
struct Scope<'a> {
    context: &'a Context<'a>,
    catalog: &'a Catalog,
    application: Option<usize>,
}

/// A row before paging: what it sorts and filters by, and its section.
struct Placed {
    position: usize,
    section: EntranceSection,
    /// The section's place among sections: launches, routes (by segment, routes without one
    /// last), then the other kinds.
    rank: (u8, bool, Option<String>, Option<String>),
    row: CatalogRow,
}

impl Placed {
    fn matches(&self, filter: &str) -> bool {
        [
            Some(self.row.title.as_str()),
            self.row.trigger.as_deref(),
            self.row.handler.as_deref(),
            Some(self.row.path.as_str()),
        ]
        .into_iter()
        .flatten()
        .any(|text| text.to_lowercase().contains(filter))
    }

    fn order(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank).then_with(|| {
            if self.row.kind == EntranceKind::Http {
                self.row
                    .trigger
                    .cmp(&other.row.trigger)
                    .then_with(|| method_rank(self.row.method).cmp(&method_rank(other.row.method)))
                    .then_with(|| self.row.method.cmp(&other.row.method))
                    .then_with(|| self.row.key.cmp(&other.row.key))
            } else {
                self.row
                    .title
                    .cmp(&other.row.title)
                    .then_with(|| self.row.key.cmp(&other.row.key))
            }
        })
    }
}

impl Scope<'_> {
    /// The row of the entrance at `position`, without its page-only fields (line, reaches).
    fn place(&self, position: usize) -> CommandResult<Placed> {
        let catalog = self.catalog;
        let entrance = &catalog.entrances[position];
        let record = catalog.record(position);
        let (title, handler) = match entrance.kind {
            EntranceKind::Process => (
                record
                    .and_then(|launch| launch.label.as_deref().or(launch.command.as_deref()))
                    .map_or_else(|| entrance_title(entrance), str::to_owned),
                None,
            ),
            _ => (
                entrance_title(entrance),
                entrance
                    .targets
                    .first()
                    .filter(|target| target.resolution == Resolution::Resolved)
                    .map(|target| target.text.to_string()),
            ),
        };
        let section = |title: &str, kind| EntranceSection {
            title: title.to_owned(),
            kind,
            mounted_from: None,
            also_in: Vec::new(),
            count: 0,
        };
        let (rank, section) = match entrance.kind {
            EntranceKind::Process => ((0, false, None, None), section("Launch", SectionKind::Launch)),
            EntranceKind::Http => {
                let segment = segment(entrance);
                let mounted_from = self.mounted_from(entrance)?;
                let title = format!("/{}", segment.unwrap_or_default());
                let rank = (
                    1,
                    segment.is_none(),
                    segment.map(str::to_owned),
                    mounted_from.clone(),
                );
                (
                    rank,
                    EntranceSection {
                        mounted_from,
                        ..section(&title, SectionKind::Http)
                    },
                )
            }
            EntranceKind::Worker => ((2, false, None, None), section("Workers", SectionKind::Worker)),
            EntranceKind::Schedule => (
                (3, false, None, None),
                section("Schedules", SectionKind::Schedule),
            ),
            EntranceKind::Command => ((4, false, None, None), section("Commands", SectionKind::Command)),
            EntranceKind::Event => ((5, false, None, None), section("Events", SectionKind::Event)),
        };
        Ok(Placed {
            position,
            section,
            rank,
            row: CatalogRow {
                key: entrance.key.as_str().to_owned(),
                id: entrance.key.configured_id().map(str::to_owned),
                section: 0,
                kind: entrance.kind,
                method: entrance.method,
                trigger: entrance.trigger.as_deref().map(str::to_owned),
                title,
                handler,
                path: entrance.evidence.path.as_str().to_owned(),
                line: None,
                reaches: None,
                also_in: catalog.also_in(position, self.application),
            },
        })
    }

    /// The router file a route is mounted from: its evidence file, when that isn't the file of
    /// the application it is bound to.
    fn mounted_from(&self, entrance: &Entrance) -> CommandResult<Option<String>> {
        let Some(binding) = &entrance.binding else {
            return Ok(None);
        };
        let application = self.context.reader.block(binding.as_str())?;
        Ok(application
            .filter(|located| located.path != entrance.evidence.path)
            .map(|_| entrance.evidence.path.as_str().to_owned()))
    }

    /// The finished row of a page.
    fn finish(&self, placed: Placed, section: u32) -> CommandResult<CatalogRow> {
        let entrance = &self.catalog.entrances[placed.position];
        let line = match (&entrance.evidence.block, self.catalog.record(placed.position)) {
            (Some(block), _) => self
                .context
                .reader
                .block(block.as_str())?
                .map(|located| located.block.lines.start),
            (None, Some(launch)) => Some(launch.line),
            (None, None) => None,
        };
        Ok(CatalogRow {
            section,
            line,
            reaches: self.context.reaches(entrance.key.as_str()),
            ..placed.row
        })
    }
}

/// The first path segment of an HTTP trigger (`/runs/{id}` → `runs`).
fn segment(entrance: &Entrance) -> Option<&str> {
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
