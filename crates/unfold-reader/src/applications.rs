//! Applications and membership (architecture §10.1).
//!
//! - An **application** is a distinct resolved startup among process entrances: a file, or the
//!   callable a Python console script names. Launches that start the same target share one
//!   application; external or unresolved startups create none. Its activation seeds are the
//!   startup and, for a console script, the callable's module root, so import-time
//!   registrations count without activating every function in the module.
//! - A non-process entrance's **anchor** is the owner of its evidence block, or else its first
//!   resolved target (a branch target anchors to its owner). It **belongs** to every application
//!   whose activation closure contains the anchor.
//!
//! An application is named after its startup file without the extension (`server`), followed by
//! `#` and the callable's qualified name for a console script (`cli#main`).

use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind};
use unfold_core::{Graph, Key};
use unfold_store::{Application, Membership};

use crate::catalog::qualified_name;
use crate::closure::{closure, is_branch};
use crate::index::{BlockId, EntranceFact, GraphIndex, NONE};

/// One application during derivation: its startup and whether a console script launches it.
struct Startup {
    block: BlockId,
    console_script: bool,
}

/// The startup targets of the process entrances, one per distinct target, in canonical entrance
/// order of their first launch.
fn startups(index: &GraphIndex) -> Vec<Startup> {
    let mut startups: Vec<Startup> = Vec::new();
    for entrance in index.entrances() {
        if entrance.kind != EntranceKind::Process || entrance.first == NONE {
            continue;
        }
        let console_script = entrance.detector == DetectorId::PyprojectScript;
        match startups
            .iter_mut()
            .find(|startup| startup.block == entrance.first)
        {
            Some(startup) => startup.console_script |= console_script,
            None => startups.push(Startup {
                block: entrance.first,
                console_script,
            }),
        }
    }
    startups
}

/// The activation seeds of an application: its startup, plus the module root for a console
/// script's callable.
fn seeds(index: &GraphIndex, startup: &Startup) -> Vec<BlockId> {
    let root = index.file_root(startup.block);
    if startup.console_script && root != startup.block {
        vec![startup.block, root]
    } else {
        vec![startup.block]
    }
}

/// The activation seeds of an entrance, for "closure per entrance": a process entrance seeds
/// like its application; any other entrance seeds every resolved target.
pub(crate) fn entrance_seeds(index: &GraphIndex, entrance: &EntranceFact) -> Vec<BlockId> {
    if entrance.kind != EntranceKind::Process {
        return entrance.resolved.to_vec();
    }
    if entrance.first == NONE {
        return Vec::new();
    }
    let startup = Startup {
        block: entrance.first,
        console_script: entrance.detector == DetectorId::PyprojectScript,
    };
    seeds(index, &startup)
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

/// The application name for a startup block.
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

/// The applications (sorted by startup key) and memberships (sorted) of `graph`. `index` must
/// be built from `graph`.
pub(crate) fn applications(graph: &Graph, index: &GraphIndex) -> (Vec<Application>, Vec<Membership>) {
    let startups = startups(index);
    // The closure of a seed set is the union of its seeds' closures, so one walk per
    // application covers both of a console script's seeds.
    let closures: Vec<Vec<BlockId>> = startups
        .iter()
        .map(|startup| closure(index, &seeds(index, startup)))
        .collect();
    let mut applications: Vec<Application> = startups
        .iter()
        .map(|startup| Application {
            startup: index.key(startup.block).clone(),
            name: application_name(graph, index, startup.block).into(),
            language: index.language(startup.block),
        })
        .collect();
    let mut memberships: Vec<Membership> = Vec::new();
    for entrance in index.entrances() {
        if entrance.kind == EntranceKind::Process {
            continue;
        }
        let Some(anchor) = anchor(index, entrance) else {
            continue;
        };
        for (startup, reached) in startups.iter().zip(&closures) {
            if reached.binary_search(&anchor).is_ok() {
                memberships.push(Membership {
                    entrance: entrance.key.clone(),
                    application: index.key(startup.block).clone(),
                });
            }
        }
    }
    applications.sort_by(|a, b| a.startup.cmp(&b.startup));
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
