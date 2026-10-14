//! The census validation compares Python files with: token boundaries and call sites from the
//! payload's independent lexer scan, and the file's runtime module dependencies (the ones the
//! pre-pass found, plus the named child imports linking keeps). Python's call sites name no
//! form: only linking can prove a construction.

use rustc_hash::FxHashMap;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::Language;
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
            .map(|&range| CallSite { range, form: None })
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

    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]> {
        self.files.get(path).map(|file| file.calls.as_slice())
    }

    fn module_dependencies(&self, path: &RepoPath) -> Option<&[ByteRange]> {
        self.files.get(path).map(|file| file.dependencies.as_slice())
    }
}
