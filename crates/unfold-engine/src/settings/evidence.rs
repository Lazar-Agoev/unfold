//! `[evidence] files`: files boards cite that Unfold hashes but never compiles, such as SQL
//! migrations or a Prisma schema.

use globset::GlobSet;
use serde::{Serialize, Serializer};
use unfold_core::RepoPath;

use super::scope::{ScopePatternError, compile};

/// Which files are evidence: glob patterns relative to the open folder, matched exactly as
/// [`super::Scope`] matches its patterns (the path itself, a folder that contains it, or a
/// glob). `[scope]` doesn't apply to them; ignore files and the default exclusions do.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    patterns: Vec<Box<str>>,
    set: GlobSet,
}

impl Evidence {
    /// Compiles `patterns`, in the order written.
    pub fn new<P>(patterns: P) -> Result<Self, ScopePatternError>
    where
        P: IntoIterator,
        P::Item: AsRef<str>,
    {
        let (patterns, set) = compile(patterns)?;
        Ok(Self { patterns, set })
    }

    /// The patterns as written.
    pub fn patterns(&self) -> &[Box<str>] {
        &self.patterns
    }

    /// Whether `path` matches a pattern.
    pub fn contains(&self, path: &RepoPath) -> bool {
        !self.patterns.is_empty() && self.set.is_match(path.as_str())
    }
}

impl PartialEq for Evidence {
    fn eq(&self, other: &Self) -> bool {
        self.patterns == other.patterns
    }
}

impl Serialize for Evidence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.patterns.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(evidence: &Evidence, path: &str) -> bool {
        evidence.contains(&RepoPath::new(path).unwrap())
    }

    #[test]
    fn patterns_match_files_folders_and_globs() {
        let evidence = Evidence::new(["prisma/schema.prisma", "db/migrations/*.sql", "openapi"]).unwrap();
        for path in [
            "prisma/schema.prisma",
            "db/migrations/001_init.sql",
            "openapi/orders.yaml",
        ] {
            assert!(matches(&evidence, path), "{path}");
        }
        for path in [
            "db/migrations/old/001.sql",
            "prisma/schema.prisma.bak",
            "db/seed.sql",
        ] {
            assert!(!matches(&evidence, path), "{path}");
        }
        assert!(!matches(&Evidence::default(), "a.sql"));
        assert_eq!(evidence.patterns()[1].as_ref(), "db/migrations/*.sql");
    }

    #[test]
    fn patterns_are_confined_to_the_folder() {
        assert_eq!(Evidence::new([""]).unwrap_err(), ScopePatternError::Empty);
        assert!(matches!(
            Evidence::new(["/etc/passwd"]),
            Err(ScopePatternError::Absolute(_))
        ));
        assert!(matches!(
            Evidence::new(["../secrets.sql"]),
            Err(ScopePatternError::Component(_))
        ));
    }
}
