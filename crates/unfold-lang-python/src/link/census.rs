//! The census validation compares Python files with: call sites from the payload's independent
//! lexer scan. Python's call sites name no form: only linking can prove a construction.

use rustc_hash::FxHashMap;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::Language;
use unfold_core::{ByteRange, RepoPath};

/// The census of every Python file of a program.
#[derive(Debug, Default)]
pub struct PythonCensus {
    files: FxHashMap<RepoPath, Vec<CallSite>>,
}

impl PythonCensus {
    /// Adds a file's call sites.
    pub(crate) fn add(&mut self, path: RepoPath, calls: &[ByteRange]) {
        let calls = calls
            .iter()
            .map(|&range| CallSite { range, form: None })
            .collect();
        self.files.insert(path, calls);
    }
}

impl Census for PythonCensus {
    fn language(&self) -> Language {
        Language::Python
    }

    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]> {
        self.files.get(path).map(Vec::as_slice)
    }
}
