//! Strict parsing of `unfold.toml`: serde rejects unknown keys and wrong types in the compile
//! settings, then each value is validated with its exact position in the file. The `[[app]]`
//! tables are read separately ([`super::apps`]), so a bad entry marks only itself.

use std::ops::Range;
use std::path::PathBuf;

use serde::Deserialize;
use toml::Spanned;
use toml::de::{DeTable, Deserializer};
use unfold_core::ByteRange;
use unfold_core::vocab::Language;

use super::apps;
use super::{
    Evidence, FolderPath, PythonProject, PythonVersion, SETTINGS_PATH, Scope, Settings, SettingsFile,
};

/// Why the settings can't be used. Every validation error carries a one-based line and column
/// (counted in characters) and the byte span of the offending key or value.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The file exists but couldn't be read.
    #[error("cannot read {}: {source}", path.display())]
    Read {
        /// The settings file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file isn't UTF-8.
    #[error("{SETTINGS_PATH} is not valid UTF-8 (at byte {offset})")]
    Encoding {
        /// The first invalid byte.
        offset: usize,
    },
    /// The TOML is malformed, or a key or value is invalid.
    #[error("{SETTINGS_PATH}:{line}:{column}: {message}")]
    Invalid {
        /// What is wrong.
        message: Box<str>,
        /// The one-based line.
        line: u32,
        /// The one-based column, in characters.
        column: u32,
        /// The bytes at fault.
        span: ByteRange,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSettings {
    languages: Option<Spanned<Vec<Spanned<String>>>>,
    scope: Option<RawScope>,
    python: Option<RawPython>,
    evidence: Option<RawEvidence>,
    /// The `[[app]]` tables, read separately ([`super::apps`]) so a bad entry marks only itself;
    /// declared here so an unknown table's error names `app` among the known keys.
    #[serde(rename = "app")]
    _app: Option<serde::de::IgnoredAny>,
}

/// `[evidence]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvidence {
    files: Option<Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScope {
    include: Option<Vec<Spanned<String>>>,
    exclude: Option<Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPython {
    project: Option<Vec<Spanned<RawProject>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    root: Option<Spanned<String>>,
    source_roots: Option<Spanned<Vec<Spanned<String>>>>,
    version: Option<Spanned<String>>,
}

/// Positions errors in the original file text.
pub(super) struct Source<'a> {
    text: &'a str,
    /// Bytes skipped before the TOML body (a byte-order mark).
    base: usize,
}

impl Source<'_> {
    pub(super) fn error(&self, span: Range<usize>, message: impl Into<Box<str>>) -> SettingsError {
        let start = (span.start + self.base).min(self.text.len());
        let end = (span.end + self.base).clamp(start, self.text.len());
        let before = &self.text[..start];
        let line_start = before.rfind('\n').map_or(0, |at| at + 1);
        let line = before.matches('\n').count() + 1;
        let column = before[line_start..].chars().count() + 1;
        SettingsError::Invalid {
            message: message.into(),
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
            span: range(start..end),
        }
    }

    /// A span of the TOML body as a range of the file's bytes.
    pub(super) fn range(&self, span: Range<usize>) -> ByteRange {
        let start = (span.start + self.base).min(self.text.len());
        let end = (span.end + self.base).clamp(start, self.text.len());
        range(start..end)
    }
}

fn range(span: Range<usize>) -> ByteRange {
    let clamp = |offset: usize| u32::try_from(offset).unwrap_or(u32::MAX);
    ByteRange {
        start: clamp(span.start),
        end: clamp(span.end).max(clamp(span.start)),
    }
}

pub(super) fn parse(text: &str) -> Result<SettingsFile, SettingsError> {
    let (body, base) = match text.strip_prefix('\u{FEFF}') {
        Some(body) => (body, '\u{FEFF}'.len_utf8()),
        None => (text, 0),
    };
    let source = Source { text, base };
    let invalid = |error: toml::de::Error| {
        let span = error.span().unwrap_or(0..0);
        source.error(span, error.message().trim_end())
    };
    let document = DeTable::parse(body).map_err(invalid)?;
    let apps = document.get_ref().get("app").cloned();
    let raw = RawSettings::deserialize(Deserializer::from(document)).map_err(invalid)?;
    let settings = validate(raw, &source)?;
    let apps = match apps {
        Some(apps) => apps::parse(apps, &source)?,
        None => Vec::new(),
    };
    Ok(SettingsFile { settings, apps })
}

fn validate(raw: RawSettings, source: &Source<'_>) -> Result<Settings, SettingsError> {
    let languages = raw.languages.map(|list| languages(list, source)).transpose()?;
    let scope = match raw.scope {
        Some(scope) => self::scope(scope, source)?,
        None => Scope::default(),
    };
    let python_projects = match raw.python.and_then(|python| python.project) {
        Some(projects) => self::projects(projects, source)?,
        None => Vec::new(),
    };
    let evidence = match raw.evidence.and_then(|evidence| evidence.files) {
        Some(files) => evidence(files, source)?,
        None => Evidence::default(),
    };
    Ok(Settings {
        languages,
        scope,
        python_projects,
        evidence,
    })
}

fn languages(
    list: Spanned<Vec<Spanned<String>>>,
    source: &Source<'_>,
) -> Result<Vec<Language>, SettingsError> {
    let span = list.span();
    let mut languages = Vec::new();
    for name in list.into_inner() {
        let language = name.get_ref().parse::<Language>().map_err(|_| {
            source.error(
                name.span(),
                format!(
                    "unknown language `{}`; use \"typescript\" or \"python\"",
                    name.get_ref()
                ),
            )
        })?;
        languages.push(language);
    }
    if languages.is_empty() {
        return Err(source.error(
            span,
            "`languages` must name at least one language; omit it to graph every language",
        ));
    }
    languages.sort_unstable();
    languages.dedup();
    Ok(languages)
}

fn scope(raw: RawScope, source: &Source<'_>) -> Result<Scope, SettingsError> {
    let include = raw.include.unwrap_or_default();
    let exclude = raw.exclude.unwrap_or_default();
    for pattern in include.iter().chain(&exclude) {
        Scope::check_pattern(pattern.get_ref())
            .map_err(|error| source.error(pattern.span(), error.to_string()))?;
    }
    Scope::new(
        include.iter().map(Spanned::get_ref),
        exclude.iter().map(Spanned::get_ref),
    )
    .map_err(|error| source.error(0..0, error.to_string()))
}

fn evidence(files: Vec<Spanned<String>>, source: &Source<'_>) -> Result<Evidence, SettingsError> {
    for pattern in &files {
        Scope::check_pattern(pattern.get_ref())
            .map_err(|error| source.error(pattern.span(), error.to_string()))?;
    }
    Evidence::new(files.iter().map(Spanned::get_ref)).map_err(|error| source.error(0..0, error.to_string()))
}

fn projects(raw: Vec<Spanned<RawProject>>, source: &Source<'_>) -> Result<Vec<PythonProject>, SettingsError> {
    let mut projects: Vec<PythonProject> = Vec::new();
    for table in raw {
        let table_span = table.span();
        let raw = table.into_inner();
        let folder = |value: &Spanned<String>| {
            FolderPath::parse(value.get_ref()).map_err(|error| {
                source.error(
                    value.span(),
                    format!("invalid folder `{}`: {error}", value.get_ref()),
                )
            })
        };
        let root = match &raw.root {
            Some(root) => folder(root)?,
            None => FolderPath::ROOT,
        };
        if projects.iter().any(|project| project.root == root) {
            let span = raw.root.as_ref().map_or(table_span, Spanned::span);
            return Err(source.error(
                span,
                format!("two `[[python.project]]` tables have the root `{root}`"),
            ));
        }
        let source_roots = match raw.source_roots {
            None => None,
            Some(list) => {
                let span = list.span();
                let mut roots: Vec<FolderPath> = Vec::new();
                for value in list.get_ref() {
                    let joined = root.join(&folder(value)?);
                    if roots.contains(&joined) {
                        return Err(
                            source.error(value.span(), format!("duplicate Python source root `{joined}`"))
                        );
                    }
                    roots.push(joined);
                }
                if roots.is_empty() {
                    return Err(source.error(
                        span,
                        "`source_roots` must name at least one folder; omit it for the default",
                    ));
                }
                Some(roots)
            }
        };
        let version = match &raw.version {
            None => PythonVersion::default(),
            Some(version) => PythonVersion::parse(version.get_ref()).ok_or_else(|| {
                source.error(
                    version.span(),
                    format!(
                        "unsupported Python grammar version \"{}\"; use \"3.12\", \"3.13\", or \"3.14\"",
                        version.get_ref()
                    ),
                )
            })?,
        };
        projects.push(PythonProject {
            root,
            source_roots,
            version,
        });
    }
    projects.sort_by(|a, b| a.root.cmp(&b.root));
    Ok(projects)
}
