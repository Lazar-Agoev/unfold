//! Manual entrance locators: the text a `[[entrance]]` uses to name its handler or startup.
//!
//! The grammar is `path`, `path#name`, `path#Owner.member`, `path#default`, or `path:line`.
//! Parsing only checks syntax; resolving a locator against the current declarations and exports
//! happens in the backend on every compile, so a well-formed locator that names nothing stays a
//! graph record with an unresolved target.

use std::fmt;
use std::str::FromStr;

use unfold_core::RepoPath;
use unfold_core::path::PathError;

/// The largest line a locator may name: JavaScript's largest safe integer, `2^53 - 1`, which
/// keeps the grammar identical to the proof of concept's.
pub const MAX_LOCATOR_LINE: u64 = (1 << 53) - 1;

/// A parsed manual locator. It keeps the text as written, which is the entrance target's text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locator {
    text: Box<str>,
    path: RepoPath,
    selector: Selector,
}

/// What a locator selects inside its file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selector {
    /// The bare path: the file itself (a startup's module root).
    File,
    /// `path#name`: an exported or top-level name. `path#default` is the default export.
    Name(Box<str>),
    /// `path#Owner.member`: a direct member of a named class or constant object literal.
    Member {
        /// The class or object name.
        owner: Box<str>,
        /// The member name.
        member: Box<str>,
    },
    /// `path:line`: the outermost permitted block starting on this one-based line.
    Line(u64),
}

/// Why a string is not a valid locator. Each variant names the first rule the text breaks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocatorError {
    /// The locator is empty.
    #[error("a locator must not be empty")]
    Empty,
    /// The locator starts with a drive prefix such as `C:`.
    #[error("locator `{0}` starts with a drive prefix; paths are relative to the folder")]
    Drive(Box<str>),
    /// More than one `#` or `:` delimiter.
    #[error("locator `{0}` has more than one `#` or `:` selector")]
    Delimiters(Box<str>),
    /// The path part is not a valid folder-relative path.
    #[error("locator `{text}` has an invalid path: {reason}")]
    Path {
        /// The whole locator.
        text: Box<str>,
        /// Why the path is invalid.
        reason: PathError,
    },
    /// The name selector is not one identifier or `Owner.member`.
    #[error(
        "locator `{0}` has an invalid name selector; use `#name`, `#Owner.member`, or `#default` with identifier names"
    )]
    Name(Box<str>),
    /// The line selector is not a canonical positive decimal integer in range.
    #[error("locator `{0}` has an invalid line; use a positive decimal number without leading zeros")]
    Line(Box<str>),
}

impl Locator {
    /// Parses `text`.
    pub fn parse(text: &str) -> Result<Self, LocatorError> {
        let owned = || Box::<str>::from(text);
        if text.is_empty() {
            return Err(LocatorError::Empty);
        }
        if has_drive_prefix(text) {
            return Err(LocatorError::Drive(owned()));
        }
        let mut delimiters = text.match_indices(['#', ':']);
        let delimiter = delimiters.next();
        if delimiters.next().is_some() {
            return Err(LocatorError::Delimiters(owned()));
        }
        let (path, suffix) = match delimiter {
            Some((at, mark)) => (&text[..at], Some((mark, &text[at + 1..]))),
            None => (text, None),
        };
        let path = RepoPath::new(path).map_err(|reason| LocatorError::Path {
            text: owned(),
            reason,
        })?;
        let selector = match suffix {
            None => Selector::File,
            Some((":", line)) => Selector::Line(parse_line(line).ok_or_else(|| LocatorError::Line(owned()))?),
            Some((_, names)) => match names.split_once('.') {
                None if is_identifier(names) => Selector::Name(names.into()),
                Some((owner, member)) if is_identifier(owner) && is_identifier(member) => Selector::Member {
                    owner: owner.into(),
                    member: member.into(),
                },
                _ => return Err(LocatorError::Name(owned())),
            },
        };
        Ok(Self {
            text: owned(),
            path,
            selector,
        })
    }

    /// The locator as written.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The file the locator names.
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// What it selects in that file.
    pub fn selector(&self) -> &Selector {
        &self.selector
    }
}

fn has_drive_prefix(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// A canonical positive decimal: no sign, no leading zero, at most [`MAX_LOCATOR_LINE`].
fn parse_line(text: &str) -> Option<u64> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok().filter(|&line| line <= MAX_LOCATOR_LINE)
}

/// A JavaScript identifier name: `$`, `_`, or an `ID_Start` character, then `$`, ZWNJ, ZWJ, or
/// `ID_Continue` characters. Python identifiers are a subset, so one rule serves both languages.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '$' || first == '_' || unicode_id_start::is_id_start(first))
        && chars.all(|c| matches!(c, '$' | '\u{200C}' | '\u{200D}') || unicode_id_start::is_id_continue(c))
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl serde::Serialize for Locator {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl FromStr for Locator {
    type Err = LocatorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> (String, Selector) {
        let locator = Locator::parse(text).unwrap();
        assert_eq!(locator.text(), text);
        (locator.path().to_string(), locator.selector().clone())
    }

    #[test]
    fn accepts_paths_names_members_defaults_and_lines() {
        assert_eq!(parsed("src/main.ts"), ("src/main.ts".into(), Selector::File));
        assert_eq!(
            parsed("src/main.ts#default"),
            ("src/main.ts".into(), Selector::Name("default".into()))
        );
        assert_eq!(
            parsed("src/main.ts#Owner.run"),
            (
                "src/main.ts".into(),
                Selector::Member {
                    owner: "Owner".into(),
                    member: "run".into()
                }
            )
        );
        assert_eq!(
            parsed("src/main.ts#δ_2.$run"),
            (
                "src/main.ts".into(),
                Selector::Member {
                    owner: "δ_2".into(),
                    member: "$run".into()
                }
            )
        );
        assert_eq!(
            parsed("src/my file.ts:12"),
            ("src/my file.ts".into(), Selector::Line(12))
        );
        assert_eq!(
            parsed("src/jobs/import_orders.py#import_orders"),
            (
                "src/jobs/import_orders.py".into(),
                Selector::Name("import_orders".into())
            )
        );
        assert_eq!(
            parsed("a.ts:9007199254740991"),
            ("a.ts".into(), Selector::Line(MAX_LOCATOR_LINE))
        );
        assert_eq!(
            parsed("a.ts#a\u{200C}b"),
            ("a.ts".into(), Selector::Name("a\u{200C}b".into()))
        );
    }

    #[test]
    fn rejects_escapes_and_ambiguous_delimiters() {
        for text in [
            "",
            "/root.ts",
            "../root.ts",
            "src/../main.ts",
            "src//main.ts",
            "./main.ts",
            "src/./main.ts",
            "C:/root.ts",
            "C:12",
            "src\\main.ts",
            "src/main.ts#",
            "src/main.ts#A.b.c",
            "src/main.ts##a",
            "src/main.ts#a:1",
            "src/main.ts:1#a",
            "src/main.ts:0",
            "src/main.ts:01",
            "src/main.ts:-1",
            "src/main.ts:+1",
            "src/main.ts:",
            "src/main.ts:1.5",
            "src/main.ts:9007199254740992",
            "src/main.ts:99999999999999999999999",
            "src/main.ts#not-valid",
            "src/main.ts#.run",
            "src/main.ts#Owner.",
            "src/main.ts#1abc",
            "src/main.ts\0",
            "#name",
            ":12",
        ] {
            assert!(Locator::parse(text).is_err(), "{text:?} should be rejected");
        }
    }

    #[test]
    fn errors_name_the_broken_rule() {
        assert_eq!(Locator::parse(""), Err(LocatorError::Empty));
        assert!(matches!(Locator::parse("C:/a.ts"), Err(LocatorError::Drive(_))));
        assert!(matches!(
            Locator::parse("a.ts#b:1"),
            Err(LocatorError::Delimiters(_))
        ));
        assert!(matches!(
            Locator::parse("../a.ts"),
            Err(LocatorError::Path { .. })
        ));
        assert!(matches!(Locator::parse("a.ts#a.b.c"), Err(LocatorError::Name(_))));
        assert!(matches!(Locator::parse("a.ts:0"), Err(LocatorError::Line(_))));
        assert_eq!(
            Locator::parse("a.ts:01").unwrap_err().to_string(),
            "locator `a.ts:01` has an invalid line; use a positive decimal number without leading zeros"
        );
    }
}
