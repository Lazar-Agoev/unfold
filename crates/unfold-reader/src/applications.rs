//! Applications and membership (docs/reader.md#derived-views).
//!
//! - An **application** is an `[[application]]` table (declared), or a group of launches without
//!   one (inferred): launches whose startup resolves to the same block share an inferred
//!   application named after their first label, else after the startup (`server`, `cli#main`);
//!   a launch that starts nothing Unfold resolved is an application of its own, named after its
//!   label, trigger, ID, or command. A declared
//!   application with no launch is still an application.
//! - A launch's **activation seeds** are its startup and its record's seeds (the packages a
//!   module initializes, the module of a callable or ASGI application).
//! - A **member** is an entrance that belongs to an application. Every launch is a member of its
//!   application (declared when configuration groups it). Another entrance is a proven member of
//!   each application with a launch that reaches it: a launch serving an ASGI application owns
//!   exactly the routes bound to that application; other routes belong to launches whose
//!   activation closure holds their application's module; an entrance without an application
//!   binding belongs to launches whose closure holds its **anchor** (the owner of its evidence
//!   block, or else its first resolved target; a branch target anchors to its owner). A
//!   configured entrance that names its application is a declared member of it.
//! - A **category** follows from the members, in display order: needs a look (a launch can't be
//!   read and has no target set by hand), services (routes, or a launch serving an ASGI
//!   application), scheduled (a launch schedule, or schedule entrances), and workers otherwise.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{BlockKind, EntranceKind};
use unfold_core::{Graph, Key, Launch, LaunchMode, Launches};
use unfold_store::{Application, Basis, Category, Membership};

use crate::catalog::qualified_name;
use crate::closure::{closure, is_branch};
use crate::index::{BlockId, EntranceFact, GraphIndex, NONE};
use crate::launches::needs_look;

/// The activation seeds of an entrance, for "closure per entrance": a launch's startup and
/// seeds; any other entrance's resolved targets.
pub(crate) fn entrance_seeds(entrance: &EntranceFact) -> Vec<BlockId> {
    if entrance.kind != EntranceKind::Process {
        return entrance.resolved.to_vec();
    }
    let mut seeds: Vec<BlockId> = (entrance.first != NONE)
        .then_some(entrance.first)
        .into_iter()
        .collect();
    seeds.extend(entrance.seeds.iter().copied());
    seeds
}

/// The block a non-process entrance is placed by, if any.
pub(crate) fn anchor(index: &GraphIndex, entrance: &EntranceFact) -> Option<BlockId> {
    if entrance.evidence_block != NONE {
        return Some(index.owner(entrance.evidence_block));
    }
    let target = *entrance.resolved.first()?;
    Some(if is_branch(index.kind(target)) {
        index.owner(target)
    } else {
        target
    })
}

/// The name of an inferred application started at `startup`.
fn application_name(graph: &Graph, index: &GraphIndex, startup: BlockId) -> String {
    let file_name = index.path(startup).file_name();
    let stem = match file_name.rsplit_once('.') {
        Some((stem, extension)) if is_source_extension(extension) => stem,
        _ => file_name,
    };
    if index.kind(startup) == BlockKind::File {
        return stem.to_owned();
    }
    let file = &graph.files[index.file_of(startup)];
    match qualified_name(file, index.local_id(startup)) {
        Some(callable) => format!("{stem}#{callable}"),
        None => stem.to_owned(),
    }
}

/// `py`, or a JavaScript-family extension: `[cm]?[jt]sx?`.
fn is_source_extension(extension: &str) -> bool {
    let rest = extension.strip_prefix(['c', 'm']).unwrap_or(extension);
    let rest = rest.strip_suffix('x').unwrap_or(rest);
    extension == "py" || rest == "ts" || rest == "js"
}

/// One application while it is derived.
struct Group<'a> {
    id: Box<str>,
    name: Box<str>,
    description: Option<Box<str>>,
    declared: bool,
    /// Its launches: positions among the entrances, with their records.
    launches: Vec<(usize, Option<&'a Launch>)>,
}

/// The applications (sorted by ID) and memberships (sorted) of `graph` and its launch records.
/// `index` must be built from both.
pub(crate) fn applications(
    graph: &Graph,
    index: &GraphIndex,
    launches: &Launches,
) -> (Vec<Application>, Vec<Membership>) {
    let records: FxHashMap<&str, &Launch> = launches
        .launches
        .iter()
        .map(|launch| (launch.entrance.as_str(), launch))
        .collect();
    let mut groups: Vec<Group<'_>> = launches
        .applications
        .iter()
        .map(|application| Group {
            id: application.id.clone(),
            name: application.name.clone(),
            description: application.description.clone(),
            declared: true,
            launches: Vec::new(),
        })
        .collect();
    let mut by_id: FxHashMap<Box<str>, usize> = groups
        .iter()
        .enumerate()
        .map(|(position, group)| (group.id.clone(), position))
        .collect();
    let facts = index.entrances();
    for (position, (fact, entrance)) in facts.iter().zip(&graph.entrances).enumerate() {
        if fact.kind != EntranceKind::Process {
            continue;
        }
        let record = records.get(fact.key.as_str()).copied();
        let declared = record
            .and_then(|launch| launch.application.as_deref())
            .and_then(|id| by_id.get(id).copied());
        let group = match declared {
            Some(group) => group,
            None => {
                let label = record.and_then(|launch| launch.label.as_deref());
                let (id, name): (Box<str>, String) = if fact.first == NONE {
                    let command = record.and_then(|launch| launch.command.as_deref());
                    let name = label
                        .or(entrance.trigger.as_deref())
                        .or(fact.key.configured_id())
                        .or(command)
                        .unwrap_or(fact.key.as_str());
                    (fact.key.as_str().into(), name.to_owned())
                } else {
                    let name =
                        label.map_or_else(|| application_name(graph, index, fact.first), str::to_owned);
                    (index.key(fact.first).as_str().into(), name)
                };
                *by_id.entry(id.clone()).or_insert_with(|| {
                    groups.push(Group {
                        id,
                        name: name.into(),
                        description: record.and_then(|launch| launch.description.clone()),
                        declared: false,
                        launches: Vec::new(),
                    });
                    groups.len() - 1
                })
            }
        };
        groups[group].launches.push((position, record));
    }

    let mut memberships: Vec<Membership> = Vec::new();
    // Launches with the same seeds share one walk.
    let mut closures: FxHashMap<Vec<BlockId>, FxHashSet<BlockId>> = FxHashMap::default();
    let mut applications = Vec::with_capacity(groups.len());
    for group in &groups {
        let mut members: FxHashMap<usize, Basis> = FxHashMap::default();
        for &(position, record) in &group.launches {
            let fact = &facts[position];
            let basis = if record.is_some_and(|launch| launch.application.is_some()) {
                Basis::Declared
            } else {
                Basis::Proven
            };
            members.insert(position, basis);
            let seeds = entrance_seeds(fact);
            let reached = closures
                .entry(seeds.clone())
                .or_insert_with(|| closure(index, &seeds).into_iter().collect());
            let asgi =
                record.and_then(|launch| launch.mode) == Some(LaunchMode::PythonAsgi) && fact.first != NONE;
            for (other, entrance) in facts.iter().enumerate() {
                if entrance.kind == EntranceKind::Process {
                    continue;
                }
                let reaches = if entrance.binding != NONE {
                    if asgi {
                        entrance.binding == fact.first
                    } else {
                        reached.contains(&index.owner(entrance.binding))
                    }
                } else {
                    anchor(index, entrance).is_some_and(|anchor| reached.contains(&anchor))
                };
                if reaches {
                    members.insert(other, Basis::Proven);
                }
            }
        }
        for member in &launches.members {
            if *member.application == *group.id
                && let Some(position) = facts.iter().position(|fact| fact.key == member.entrance)
            {
                members.entry(position).or_insert(Basis::Declared);
            }
        }
        let kinds = || members.keys().map(|&position| facts[position].kind);
        let records = || group.launches.iter().filter_map(|(_, record)| *record);
        let category = if records().any(needs_look) {
            Category::NeedsLook
        } else if kinds().any(|kind| kind == EntranceKind::Http)
            || records().any(|launch| launch.mode == Some(LaunchMode::PythonAsgi))
        {
            Category::Services
        } else if kinds().any(|kind| kind == EntranceKind::Schedule)
            || records().any(|launch| launch.schedule.is_some())
        {
            Category::Scheduled
        } else {
            Category::Workers
        };
        memberships.extend(members.into_iter().map(|(position, basis)| Membership {
            entrance: facts[position].key.clone(),
            application: group.id.clone(),
            basis,
        }));
        applications.push(Application {
            id: group.id.clone(),
            name: group.name.clone(),
            description: group.description.clone(),
            declared: group.declared,
            category,
        });
    }
    applications.sort_by(|a, b| a.id.cmp(&b.id));
    memberships.sort();
    (applications, memberships)
}

/// The keys of `ids`, in order.
pub(crate) fn keys(index: &GraphIndex, ids: &[BlockId]) -> Vec<Key> {
    ids.iter().map(|&id| index.key(id).clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_extensions_follow_the_poc_pattern() {
        for extension in ["py", "ts", "tsx", "js", "jsx", "mts", "cjs", "mjsx"] {
            assert!(is_source_extension(extension), "{extension}");
        }
        for extension in ["json", "pyi", "md", "x", "mx"] {
            assert!(!is_source_extension(extension), "{extension}");
        }
    }
}
