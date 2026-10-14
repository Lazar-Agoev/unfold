//! Resolving the `code` of apps and entrances (docs/designs/entrance-definitions.md, "Resolving
//! `code`"): after every update, and when only the `[[app]]` tables change, each `code` key is
//! looked up in the graph and given a [`TargetState`]. The graph's apps hold the valid entries;
//! one with a problem stays out ([`crate::settings::EntryProblem`]).
//!
//! | State | When |
//! | --- | --- |
//! | `found` | the key names a module, a definition, or an inline function |
//! | `missing` | no block has the key, or it isn't a block key |
//! | `not_analyzed` | the key's file exists but isn't compiled: scope or ignore rules exclude it, or it isn't a source Unfold compiles |
//! | `not_a_start` | the key names a call, an exit, a control block, or a part |
//! | `stale` | the key names a start in a file that stopped parsing, from its last good facts |
//!
//! A missing key may have a [`Fix`], offered and never applied, when exactly one candidate fits:
//!
//! | Fix | Candidate |
//! | --- | --- |
//! | [`FixKind::MovedDefinition`] | the one definition in another file with the same qualified name |
//! | [`FixKind::EditedRoute`] | for an inline handler of a call, the one other call under the same parent to the same callee with a handler in the same place, which no entrance names; the fix is that handler |
//! | [`FixKind::MovedModule`] | the one module in another file with the same file name and the same top-level definitions as the missing module had when it was last seen |

use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::FxHashSet;
use unfold_core::key::{ParsedKey, Segment};
use unfold_core::vocab::{FileState, Inline, NodeKind, TargetState};
use unfold_core::{App, Entrance, EntranceTarget, FileGraph, Graph, Key, RepoPath, owner};

use crate::settings::AppEntry;

/// A one-candidate fix for a `code` key that names nothing: the key to write instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Why it fits.
    pub kind: FixKind,
    /// The key to write instead.
    pub code: Key,
}

/// Why a [`Fix`] fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixKind {
    /// The definition moved to another file.
    MovedDefinition,
    /// The literal of a route's registration call was edited.
    EditedRoute,
    /// The module moved.
    MovedModule,
}

/// The fixes of the current apps, by the missing `code` key they fix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fixes(BTreeMap<Key, Fix>);

impl Fixes {
    /// The fix for a target, if it is missing and exactly one candidate fits.
    pub fn get(&self, target: &EntranceTarget) -> Option<&Fix> {
        self.0.get(&target.code)
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The top-level definitions of the modules some `code` names, as each was last seen, so a
/// moved module can still be recognized after its old file is gone from the graph.
#[derive(Debug, Clone, Default)]
pub(crate) struct Modules(BTreeMap<Key, Vec<Box<str>>>);

impl Modules {
    /// Notes what `graph` shows of the modules `apps` name, and forgets the modules no `code`
    /// names any more.
    pub(crate) fn observe(&mut self, graph: &Graph, apps: &[AppEntry]) {
        let named: BTreeSet<&Key> = codes(apps).collect();
        self.0.retain(|key, _| named.contains(key));
        for key in named {
            if let Ok(ParsedKey::Block(block)) = key.parse()
                && block.segments().is_empty()
                && let Some(file) = graph.file(block.path().as_str())
                && !file.nodes.is_empty()
            {
                self.0.insert(key.clone(), top_level(file));
            }
        }
    }
}

/// Every `code` key the entries write, valid or not.
fn codes(apps: &[AppEntry]) -> impl Iterator<Item = &Key> {
    apps.iter().flat_map(|app| {
        app.code
            .iter()
            .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
    })
}

/// The valid apps and entrances of `apps` with their targets resolved against `graph`, and the
/// fixes of the missing ones. `on_disk` says whether a file exists in the folder.
pub(crate) fn resolve(
    graph: &Graph,
    apps: &[AppEntry],
    modules: &Modules,
    on_disk: &dyn Fn(&RepoPath) -> bool,
) -> (Vec<App>, Fixes) {
    let target = |code: &Key| EntranceTarget {
        code: code.clone(),
        state: state(graph, code, on_disk),
    };
    let resolved: Vec<App> = apps
        .iter()
        .filter(|app| app.is_valid())
        .filter_map(|app| {
            let entrances = app
                .entrances
                .iter()
                .filter(|entrance| entrance.is_valid())
                .filter_map(|entrance| {
                    Some(Entrance {
                        key: entrance.key.clone()?,
                        name: entrance.name.clone()?,
                        on: entrance.on.clone(),
                        about: entrance.about.clone(),
                        code: entrance.code.iter().map(target).collect(),
                    })
                })
                .collect();
            Some(App {
                key: app.key.clone()?,
                name: app.name.clone()?,
                run: app.run.clone(),
                code: app.code.as_ref().map(target),
                about: app.about.clone(),
                entrances,
            })
        })
        .collect();
    let named: FxHashSet<&str> = codes(apps).map(Key::as_str).collect();
    let mut fixes = BTreeMap::new();
    for target in targets(&resolved) {
        if target.state == TargetState::Missing
            && !fixes.contains_key(&target.code)
            && let Some(fix) = fix(graph, &target.code, &named, modules)
        {
            fixes.insert(target.code.clone(), fix);
        }
    }
    (resolved, Fixes(fixes))
}

fn targets(apps: &[App]) -> impl Iterator<Item = &EntranceTarget> {
    apps.iter().flat_map(|app| {
        app.code
            .iter()
            .chain(app.entrances.iter().flat_map(|entrance| &entrance.code))
    })
}

fn state(graph: &Graph, code: &Key, on_disk: &dyn Fn(&RepoPath) -> bool) -> TargetState {
    let Ok(ParsedKey::Block(block)) = code.parse() else {
        return TargetState::Missing;
    };
    let Some(file) = graph.file(block.path().as_str()) else {
        return if on_disk(block.path()) {
            TargetState::NotAnalyzed
        } else {
            TargetState::Missing
        };
    };
    let Some(node) = file
        .local_id(code.as_str())
        .and_then(|id| file.nodes.get(id as usize))
    else {
        return TargetState::Missing;
    };
    if !node.kind().is_definition() {
        TargetState::NotAStart
    } else if file.file.state == FileState::LastGood {
        TargetState::Stale
    } else {
        TargetState::Found
    }
}

fn fix(graph: &Graph, code: &Key, named: &FxHashSet<&str>, modules: &Modules) -> Option<Fix> {
    let Ok(ParsedKey::Block(block)) = code.parse() else {
        return None;
    };
    let segments = block.segments();
    if segments.is_empty() {
        let definitions = modules.0.get(code)?;
        return one(graph
            .files
            .iter()
            .filter(|file| file.file.path != *block.path())
            .filter(|file| file.file.path.file_name() == block.path().file_name())
            .filter(|file| !file.nodes.is_empty() && top_level(file) == *definitions)
            .map(|file| file.nodes[0].key.clone()))
        .map(|code| Fix {
            kind: FixKind::MovedModule,
            code,
        });
    }
    if let Some(names) = names(segments) {
        return one(definitions_named(graph, block.path(), &names)).map(|code| Fix {
            kind: FixKind::MovedDefinition,
            code,
        });
    }
    let file = graph.file(block.path().as_str())?;
    edited_route(file, segments, named).map(|code| Fix {
        kind: FixKind::EditedRoute,
        code,
    })
}

/// The only item, if there is exactly one.
fn one<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

/// The names of a named definition's key, without repeat ordinals; `None` for any other key.
fn names(segments: &[Segment]) -> Option<Vec<&str>> {
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Name { name, .. } => Some(&**name),
            _ => None,
        })
        .collect()
}

/// The keys of the definitions outside `path` whose qualified names are `names`.
fn definitions_named<'g>(
    graph: &'g Graph,
    path: &'g RepoPath,
    qualified: &'g [&str],
) -> impl Iterator<Item = Key> + 'g {
    let last = qualified.last().copied();
    graph
        .files
        .iter()
        .filter(move |file| file.file.path != *path)
        .flat_map(|file| &file.nodes)
        .filter(move |node| {
            node.kind().is_definition() && node.kind() != NodeKind::Module && node.name.as_deref() == last
        })
        .filter(move |node| match node.key.parse() {
            Ok(ParsedKey::Block(found)) => names(found.segments()).is_some_and(|found| found == qualified),
            _ => false,
        })
        .map(|node| node.key.clone())
}

/// For a key that ends with a call and an inline handler in its arguments, the same handler
/// of the one other call under the same parent to the same callee that no entrance names.
fn edited_route(file: &FileGraph, segments: &[Segment], named: &FxHashSet<&str>) -> Option<Key> {
    let [
        parent @ ..,
        Segment::Call { callee, .. },
        handler @ Segment::Inline {
            inline: Inline::Callback,
            ..
        },
    ] = segments
    else {
        return None;
    };
    let segments_of = |key: &Key| match key.parse() {
        Ok(ParsedKey::Block(block)) => Some(block.segments().to_vec()),
        _ => None,
    };
    let candidates = file.nodes.iter().enumerate().filter_map(|(id, call)| {
        if call.kind() != NodeKind::Call || call.name.as_deref() != Some(&**callee) {
            return None;
        }
        let found = segments_of(&call.key)?;
        let [found_parent @ .., Segment::Call { .. }] = found.as_slice() else {
            return None;
        };
        if found_parent != parent || named.contains(call.key.as_str()) {
            return None;
        }
        let id = u32::try_from(id).ok()?;
        file.nodes
            .iter()
            .filter(|child| child.parent == Some(id))
            .find(|child| segments_of(&child.key).is_some_and(|child| child.last() == Some(handler)))
            .filter(|child| !named.contains(child.key.as_str()))
            .map(|child| child.key.clone())
    });
    one(candidates)
}

/// A module's top-level definitions: the names of the definitions it owns, sorted.
fn top_level(file: &FileGraph) -> Vec<Box<str>> {
    let mut names: Vec<Box<str>> = file
        .nodes
        .iter()
        .enumerate()
        .filter(|(id, node)| {
            node.kind().is_definition()
                && node.kind() != NodeKind::Module
                && u32::try_from(*id).is_ok_and(|id| owner(&file.nodes, id) == Some(0))
        })
        .filter_map(|(_, node)| node.name.clone())
        .collect();
    names.sort_unstable();
    names
}
