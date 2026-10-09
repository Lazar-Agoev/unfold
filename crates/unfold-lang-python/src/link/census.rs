//! The census validation compares Python files with: token boundaries and call sites from the
//! payload's independent lexer scan, and the file's runtime module dependencies (the static
//! ones extraction found, plus the named child imports linking keeps).

use rustc_hash::FxHashMap;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::{BlockKind, Language};
use unfold_core::{ByteRange, RepoPath};

use crate::payload::CensusData;

/// One file's census, decoded once.
#[derive(Debug)]
struct CensusFile {
    starts: Vec<u32>,
    ends: Vec<u32>,
    calls: Vec<CallSite>,
    dependencies: Vec<ByteRange>,
}

/// The census of every Python file of a program.
#[derive(Debug, Default)]
pub struct PythonCensus {
    files: FxHashMap<RepoPath, CensusFile>,
}

impl PythonCensus {
    /// Adds a file's census; `children` are the ranges of the child imports linking keeps.
    pub(crate) fn add(
        &mut self,
        path: RepoPath,
        data: &CensusData,
        children: impl IntoIterator<Item = ByteRange>,
    ) {
        let calls = data
            .calls
            .iter()
            .map(|call| CallSite {
                range: call.range,
                block_kind: BlockKind::Call,
                edge_kind: None,
                ineligible_references: call.ineligible.clone(),
            })
            .collect();
        let mut dependencies = data.dependencies.clone();
        dependencies.extend(children);
        dependencies.sort_by_key(|range| (range.start, range.end));
        self.files.insert(
            path,
            CensusFile {
                starts: data.starts(),
                ends: data.ends(),
                calls,
                dependencies,
            },
        );
    }
}

impl Census for PythonCensus {
    fn language(&self) -> Language {
        Language::Python
    }

    fn is_token_range(&self, path: &RepoPath, range: ByteRange) -> bool {
        self.files.get(path).is_some_and(|file| {
            range.start <= range.end
                && file.starts.binary_search(&range.start).is_ok()
                && file.ends.binary_search(&range.end).is_ok()
        })
    }

    fn call_sites(&self, path: &RepoPath) -> Option<Vec<CallSite>> {
        self.files.get(path).map(|file| file.calls.clone())
    }

    fn module_dependencies(&self, path: &RepoPath) -> Option<Vec<ByteRange>> {
        self.files.get(path).map(|file| file.dependencies.clone())
    }
}
