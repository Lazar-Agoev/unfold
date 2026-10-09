//! The engine's [`TargetResolver`]: launch paths and whether a missing file exists.
//!
//! A launch `(cwd, path)` becomes a folder-relative file ([`launch_file`]); then, as in the
//! proof of concept, an indexed file resolves to its root block, a language's emit map may lead
//! from an emitted output back to its source, and a path without an extension is probed with
//! [`LAUNCH_EXTENSIONS`] (`x.ts` ... then `x/index.ts` ...). What's left is `not_indexed` when
//! something exists on disk there and `not_found` otherwise. Probed candidates count as existing
//! too, which fixes the proof of concept's defect C-5 (`node scripts/export` naming an
//! existing, unindexed `scripts/export.js` reported `not_found`).

use std::collections::BTreeSet;
use std::path::Path;

use unfold_core::RepoPath;
use unfold_core::vocab::Reason;

use crate::discovery::exists_in_root;
use crate::entrances::TargetResolver;
use crate::language::{EmitSource, Linked};
use crate::launch::{LAUNCH_EXTENSIONS, launch_file};
use crate::program::{BlockRef, FactsSet};
use crate::resolve::Outcome;

/// Launch and locator lookups against one folder and its program.
pub(crate) struct FolderTargets<'a> {
    pub(crate) root: &'a Path,
    /// The root as an absolute `/`-separated path.
    pub(crate) root_text: &'a str,
    pub(crate) program: &'a FactsSet,
    pub(crate) unindexed: &'a BTreeSet<RepoPath>,
    pub(crate) linked: &'a [&'a dyn Linked],
}

impl FolderTargets<'_> {
    /// The outcome for one candidate file, if the program or an emit map knows it.
    fn lookup(&self, candidate: &RepoPath) -> Option<Outcome> {
        if let Some(file) = self.program.find(candidate.as_str())
            && self.program.facts(file).is_some()
        {
            return Some(Outcome::Resolved(BlockRef { file, local: 0 }));
        }
        for linked in self.linked {
            match linked.emitted_source(candidate) {
                EmitSource::Unknown => {}
                EmitSource::Ambiguous => return Some(Outcome::unresolved(Reason::Ambiguous)),
                EmitSource::Source(source) => {
                    let file = self.program.find(source.as_str());
                    return Some(match file.filter(|&file| self.program.facts(file).is_some()) {
                        Some(file) => Outcome::Resolved(BlockRef { file, local: 0 }),
                        None => Outcome::unresolved(Reason::NotIndexed),
                    });
                }
            }
        }
        None
    }

    /// The probed variants of an extensionless path, in order.
    fn probes(file: &RepoPath) -> impl Iterator<Item = RepoPath> + '_ {
        let bases = [file.as_str().to_owned(), format!("{}/index", file.as_str())];
        bases.into_iter().flat_map(|base| {
            LAUNCH_EXTENSIONS
                .iter()
                .filter_map(move |extension| RepoPath::new(&format!("{base}{extension}")).ok())
        })
    }
}

impl TargetResolver for FolderTargets<'_> {
    fn is_unindexed_file(&self, path: &RepoPath) -> bool {
        self.unindexed.contains(path)
            || (exists_in_root(self.root, path) && self.root.join(path.as_str()).is_file())
    }

    fn launch(&self, path: &str, cwd: &str) -> Outcome {
        // Detectors drop launches outside the folder before assembly; this is a safe fallback.
        let Some(file) = launch_file(self.root_text, cwd, path) else {
            return Outcome::unresolved(Reason::NotFound);
        };
        if let Some(outcome) = self.lookup(&file) {
            return outcome;
        }
        let extensionless = file.extension().is_none();
        if extensionless && let Some(outcome) = Self::probes(&file).find_map(|probe| self.lookup(&probe)) {
            return outcome;
        }
        let exists = exists_in_root(self.root, &file)
            || (extensionless && Self::probes(&file).any(|probe| exists_in_root(self.root, &probe)));
        Outcome::unresolved(if exists {
            Reason::NotIndexed
        } else {
            Reason::NotFound
        })
    }
}
