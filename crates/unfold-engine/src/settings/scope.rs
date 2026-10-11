//! `[scope] include` and `exclude`: glob patterns relative to the open folder.

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Serialize, Serializer};
use unfold_core::RepoPath;

/// Which files are graphed, on top of the default exclusions (docs/engine.md#shared-config, file discovery).
///
/// A pattern matches a path when it equals the path, names a folder that contains it, or
/// matches it as a glob. Globs are `/`-aware: `*` and `?` stay within one component, `**`
/// crosses components, and `\` escapes the next character. With no `include` patterns every
/// file is included; any matching `exclude` pattern wins.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    include: Vec<Box<str>>,
    exclude: Vec<Box<str>>,
    include_set: GlobSet,
    exclude_set: GlobSet,
}

/// Where a path stands with respect to the scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeStatus {
    /// Graphed (as far as scope decides).
    InScope,
    /// `include` patterns are set and none matches.
    NotIncluded,
    /// An `exclude` pattern matches.
    Excluded,
}

/// A scope or evidence pattern that can't be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopePatternError {
    /// The pattern is empty.
    #[error("a pattern must not be empty")]
    Empty,
    /// The pattern is absolute.
    #[error("pattern `{0}` must be relative to the folder, not start with `/`")]
    Absolute(Box<str>),
    /// The pattern has a `.` or `..` component, which never matches a folder-relative path.
    #[error("pattern `{0}` must not contain `.` or `..` components")]
    Component(Box<str>),
    /// The glob syntax is invalid.
    #[error("pattern `{pattern}` is not a valid glob: {reason}")]
    Glob {
        /// The pattern.
        pattern: Box<str>,
        /// The glob parser's message.
        reason: Box<str>,
    },
}

impl Scope {
    /// Compiles `include` and `exclude` patterns, in the order written.
    pub fn new<I, E>(include: I, exclude: E) -> Result<Self, ScopePatternError>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
        E: IntoIterator,
        E::Item: AsRef<str>,
    {
        let (include, include_set) = compile(include)?;
        let (exclude, exclude_set) = compile(exclude)?;
        Ok(Self {
            include,
            exclude,
            include_set,
            exclude_set,
        })
    }

    /// Checks one pattern without building a scope.
    pub fn check_pattern(pattern: &str) -> Result<(), ScopePatternError> {
        globs(pattern).map(|_| ())
    }

    /// The `include` patterns as written.
    pub fn include(&self) -> &[Box<str>] {
        &self.include
    }

    /// The `exclude` patterns as written.
    pub fn exclude(&self) -> &[Box<str>] {
        &self.exclude
    }

    /// Where `path` stands.
    pub fn status(&self, path: &RepoPath) -> ScopeStatus {
        self.status_of(path.as_str())
    }

    /// Where a folder-relative `/`-separated path stands.
    pub fn status_of(&self, path: &str) -> ScopeStatus {
        if self.exclude_set.is_match(path) {
            ScopeStatus::Excluded
        } else if !self.include.is_empty() && !self.include_set.is_match(path) {
            ScopeStatus::NotIncluded
        } else {
            ScopeStatus::InScope
        }
    }

    /// Whether `path` is graphed as far as scope decides.
    pub fn contains(&self, path: &RepoPath) -> bool {
        self.status(path) == ScopeStatus::InScope
    }
}

/// The patterns as written and the glob set that matches them (each pattern matching a path,
/// a folder that contains it, or a glob, as [`Scope`] describes).
pub(super) fn compile<P>(patterns: P) -> Result<(Vec<Box<str>>, GlobSet), ScopePatternError>
where
    P: IntoIterator,
    P::Item: AsRef<str>,
{
    let mut written = Vec::new();
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = pattern.as_ref();
        for glob in globs(pattern)? {
            builder.add(glob);
        }
        written.push(pattern.into());
    }
    let set = builder.build().map_err(|error| ScopePatternError::Glob {
        pattern: written.last().cloned().unwrap_or_default(),
        reason: error.kind().to_string().into(),
    })?;
    Ok((written, set))
}

/// The two globs one pattern stands for: the pattern itself and everything beneath it.
fn globs(pattern: &str) -> Result<[Glob; 2], ScopePatternError> {
    if pattern.is_empty() {
        return Err(ScopePatternError::Empty);
    }
    if pattern.starts_with('/') {
        return Err(ScopePatternError::Absolute(pattern.into()));
    }
    let trimmed = pattern.strip_suffix('/').unwrap_or(pattern);
    if trimmed.split('/').any(|part| part == "." || part == "..") {
        return Err(ScopePatternError::Component(pattern.into()));
    }
    let build = |text: &str| {
        GlobBuilder::new(text)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
            .map_err(|error| ScopePatternError::Glob {
                pattern: pattern.into(),
                reason: error.kind().to_string().into(),
            })
    };
    Ok([build(trimmed)?, build(&format!("{trimmed}/**"))?])
}

impl PartialEq for Scope {
    fn eq(&self, other: &Self) -> bool {
        self.include == other.include && self.exclude == other.exclude
    }
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Scope", 2)?;
        state.serialize_field("include", &self.include)?;
        state.serialize_field("exclude", &self.exclude)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(scope: &Scope, path: &str) -> ScopeStatus {
        scope.status(&RepoPath::new(path).unwrap())
    }

    #[test]
    fn patterns_match_paths_folders_and_globs() {
        let scope = Scope::new(
            Vec::<&str>::new(),
            ["legacy/**", "**/generated/**", "docs", "src/*.gen.ts", "build/"],
        )
        .unwrap();
        for path in [
            "legacy/a.py",
            "legacy/deep/b.py",
            "x/generated/y.ts",
            "generated/y.ts",
            "docs/a.ts",
            "docs",
            "src/a.gen.ts",
            "build/out.ts",
        ] {
            assert_eq!(status(&scope, path), ScopeStatus::Excluded, "{path}");
        }
        for path in ["legacyx/a.py", "src/deep/a.gen.ts", "docs2/a.ts", "src/a.ts"] {
            assert_eq!(status(&scope, path), ScopeStatus::InScope, "{path}");
        }
    }

    #[test]
    fn include_narrows_and_exclude_wins() {
        let scope = Scope::new(["py-orders/src", "ts-orders/src/**/*.ts"], ["**/fixtures/**"]).unwrap();
        assert_eq!(status(&scope, "py-orders/src/a.py"), ScopeStatus::InScope);
        assert_eq!(status(&scope, "ts-orders/src/a/b.ts"), ScopeStatus::InScope);
        assert_eq!(status(&scope, "py-orders/tests/a.py"), ScopeStatus::NotIncluded);
        assert_eq!(
            status(&scope, "py-orders/src/fixtures/a.py"),
            ScopeStatus::Excluded
        );
        assert!(Scope::default().contains(&RepoPath::new("any/file.ts").unwrap()));
    }

    #[test]
    fn rejects_unusable_patterns() {
        assert_eq!(Scope::check_pattern(""), Err(ScopePatternError::Empty));
        assert!(matches!(
            Scope::check_pattern("/abs/**"),
            Err(ScopePatternError::Absolute(_))
        ));
        assert!(matches!(
            Scope::check_pattern("../x"),
            Err(ScopePatternError::Component(_))
        ));
        assert!(matches!(
            Scope::check_pattern("./x"),
            Err(ScopePatternError::Component(_))
        ));
        assert!(matches!(
            Scope::check_pattern("src/[a"),
            Err(ScopePatternError::Glob { .. })
        ));
        assert!(matches!(
            Scope::check_pattern("src/{a,b"),
            Err(ScopePatternError::Glob { .. })
        ));
    }
}
