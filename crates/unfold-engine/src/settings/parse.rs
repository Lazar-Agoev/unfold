//! Strict parsing of `.unfold/config.toml`: serde rejects unknown keys and wrong types, then each
//! value is validated with its exact position in the file.

use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use toml::Spanned;
use unfold_core::ByteRange;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Language};

use super::{
    FolderPath, ManualEntrance, ManualTargets, PythonProject, PythonVersion, SETTINGS_PATH, SETTINGS_VERSION,
    Scope, Settings,
};
use crate::locator::Locator;

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
    version: Option<Spanned<i64>>,
    languages: Option<Spanned<Vec<Spanned<String>>>>,
    scope: Option<RawScope>,
    detectors: Option<RawDetectors>,
    python: Option<RawPython>,
    entrance: Option<Vec<Spanned<RawEntrance>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScope {
    include: Option<Vec<Spanned<String>>>,
    exclude: Option<Vec<Spanned<String>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDetectors {
    disabled: Option<Vec<Spanned<String>>>,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntrance {
    kind: Spanned<String>,
    trigger: Option<Spanned<String>>,
    method: Option<Spanned<String>>,
    label: Option<Spanned<String>>,
    handler: Option<Spanned<RawHandler>>,
    startup: Option<Spanned<String>>,
}

/// `handler = "a.ts#f"` or `handler = ["a.ts#auth", "a.ts#f"]`.
enum RawHandler {
    One(String),
    Many(Vec<Spanned<String>>),
}

impl<'de> Deserialize<'de> for RawHandler {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HandlerVisitor;

        impl<'de> Visitor<'de> for HandlerVisitor {
            type Value = RawHandler;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a locator string or an array of locator strings")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<RawHandler, E> {
                Ok(RawHandler::One(value.to_owned()))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<RawHandler, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<Spanned<String>>()? {
                    items.push(item);
                }
                Ok(RawHandler::Many(items))
            }
        }

        deserializer.deserialize_any(HandlerVisitor)
    }
}

/// Positions errors in the original file text.
struct Source<'a> {
    text: &'a str,
    /// Bytes skipped before the TOML body (a byte-order mark).
    base: usize,
}

impl Source<'_> {
    fn error(&self, span: Range<usize>, message: impl Into<Box<str>>) -> SettingsError {
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
            span: self.range(start..end),
        }
    }

    /// The one-based line of a body offset.
    fn line(&self, offset: usize) -> u32 {
        let end = (offset + self.base).min(self.text.len());
        u32::try_from(
            self.text.as_bytes()[..end]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1,
        )
        .unwrap_or(u32::MAX)
    }

    fn range(&self, span: Range<usize>) -> ByteRange {
        let clamp = |offset: usize| u32::try_from(offset).unwrap_or(u32::MAX);
        ByteRange {
            start: clamp(span.start),
            end: clamp(span.end).max(clamp(span.start)),
        }
    }

    fn shifted(&self, span: Range<usize>) -> ByteRange {
        self.range(span.start + self.base..span.end + self.base)
    }
}

pub(super) fn parse(text: &str) -> Result<Settings, SettingsError> {
    let (body, base) = match text.strip_prefix('\u{FEFF}') {
        Some(body) => (body, '\u{FEFF}'.len_utf8()),
        None => (text, 0),
    };
    let source = Source { text, base };
    let raw: RawSettings = toml::from_str(body).map_err(|error| {
        let span = error.span().unwrap_or(0..0);
        source.error(span, error.message().trim_end())
    })?;
    validate(raw, &source)
}

fn validate(raw: RawSettings, source: &Source<'_>) -> Result<Settings, SettingsError> {
    if let Some(version) = &raw.version
        && *version.get_ref() != i64::from(SETTINGS_VERSION)
    {
        return Err(source.error(
            version.span(),
            format!(
                "unsupported settings version {}; this version of Unfold reads version {SETTINGS_VERSION}",
                version.get_ref()
            ),
        ));
    }
    let languages = raw.languages.map(|list| languages(list, source)).transpose()?;
    let scope = match raw.scope {
        Some(scope) => self::scope(scope, source)?,
        None => Scope::default(),
    };
    let disabled_detectors = match raw.detectors.and_then(|detectors| detectors.disabled) {
        Some(disabled) => detectors(disabled, source)?,
        None => Vec::new(),
    };
    let python_projects = match raw.python.and_then(|python| python.project) {
        Some(projects) => self::projects(projects, source)?,
        None => Vec::new(),
    };
    let entrances = raw
        .entrance
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, entrance)| self::entrance(index, entrance, source))
        .collect::<Result<_, _>>()?;
    Ok(Settings {
        version: SETTINGS_VERSION,
        languages,
        scope,
        disabled_detectors,
        python_projects,
        entrances,
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

fn detectors(disabled: Vec<Spanned<String>>, source: &Source<'_>) -> Result<Vec<DetectorId>, SettingsError> {
    let mut ids = Vec::new();
    for name in disabled {
        let id = name.get_ref().parse::<DetectorId>().map_err(|_| {
            let known: Vec<&str> = DetectorId::ALL.iter().map(|id| id.as_str()).collect();
            source.error(
                name.span(),
                format!(
                    "unknown detector `{}`; known detectors: {}",
                    name.get_ref(),
                    known.join(", ")
                ),
            )
        })?;
        if id == DetectorId::Manual {
            return Err(source.error(
                name.span(),
                "the `manual` detector can't be disabled; remove the `[[entrance]]` tables instead",
            ));
        }
        ids.push(id);
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
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

fn entrance(
    index: usize,
    table: Spanned<RawEntrance>,
    source: &Source<'_>,
) -> Result<ManualEntrance, SettingsError> {
    let span = table.span();
    let span_start = span.start;
    let raw = table.into_inner();
    let kind = raw.kind.get_ref().parse::<EntranceKind>().map_err(|_| {
        let known: Vec<&str> = EntranceKind::ALL.iter().map(|kind| kind.as_str()).collect();
        source.error(
            raw.kind.span(),
            format!(
                "unknown entrance kind `{}`; use one of {}",
                raw.kind.get_ref(),
                known.join(", ")
            ),
        )
    })?;
    if let Some(trigger) = &raw.trigger
        && trigger.get_ref().is_empty()
    {
        return Err(source.error(
            trigger.span(),
            "`trigger` must not be empty; omit it when the trigger is unknown",
        ));
    }
    let method = match &raw.method {
        None => None,
        Some(method) if kind != EntranceKind::Http => {
            return Err(source.error(
                method.span(),
                format!("only `http` entrances have a `method`, not `{kind}`"),
            ));
        }
        Some(method) => Some(method.get_ref().parse::<HttpMethod>().map_err(|_| {
            let known: Vec<&str> = HttpMethod::ALL.iter().map(|method| method.as_str()).collect();
            source.error(
                method.span(),
                format!(
                    "unknown HTTP method `{}`; use one of {}",
                    method.get_ref(),
                    known.join(", ")
                ),
            )
        })?),
    };
    // An array-of-tables entry's span is just its `[[entrance]]` header; the evidence runs on
    // to the end of the table's last value.
    let end = [
        Some(raw.kind.span()),
        raw.trigger.as_ref().map(Spanned::span),
        raw.method.as_ref().map(Spanned::span),
        raw.label.as_ref().map(Spanned::span),
        raw.handler.as_ref().map(Spanned::span),
        raw.startup.as_ref().map(Spanned::span),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.end)
    .fold(span.end, usize::max);
    let evidence = source.shifted(span.start..end);
    let targets = targets(kind, raw.handler, raw.startup, span, source)?;
    Ok(ManualEntrance {
        kind,
        trigger: raw.trigger.map(|trigger| trigger.into_inner().into()),
        method,
        label: raw.label.map(|label| label.into_inner().into()),
        targets,
        index,
        span: evidence,
        line: source.line(span_start),
    })
}

fn targets(
    kind: EntranceKind,
    handler: Option<Spanned<RawHandler>>,
    startup: Option<Spanned<String>>,
    span: Range<usize>,
    source: &Source<'_>,
) -> Result<ManualTargets, SettingsError> {
    let locator = |text: &str, span: Range<usize>| {
        Locator::parse(text).map_err(|error| source.error(span, error.to_string()))
    };
    match (kind, handler, startup) {
        (EntranceKind::Process, Some(handler), _) => Err(source.error(
            handler.span(),
            "a `process` entrance takes `startup`, not `handler`",
        )),
        (EntranceKind::Process, None, Some(startup)) => Ok(ManualTargets::Startup(locator(
            startup.get_ref(),
            startup.span(),
        )?)),
        (EntranceKind::Process, None, None) => {
            Err(source.error(span, "a `process` entrance needs a `startup` locator"))
        }
        (_, _, Some(startup)) => Err(source.error(
            startup.span(),
            format!("only `process` entrances have a `startup`; a `{kind}` entrance takes `handler`"),
        )),
        (_, None, None) => Err(source.error(
            span,
            format!("a `{kind}` entrance needs a `handler` locator or list"),
        )),
        (_, Some(handler), None) => {
            let handler_span = handler.span();
            let locators = match handler.into_inner() {
                RawHandler::One(text) => vec![locator(&text, handler_span.clone())?],
                RawHandler::Many(items) if items.is_empty() => {
                    return Err(source.error(handler_span, "`handler` must list at least one locator"));
                }
                RawHandler::Many(items) => items
                    .iter()
                    .map(|item| locator(item.get_ref(), item.span()))
                    .collect::<Result<Vec<_>, _>>()?,
            };
            let mut languages = locators.iter().filter_map(target_language);
            if let Some(first) = languages.next()
                && languages.any(|language| language != first)
            {
                return Err(source.error(
                    handler_span,
                    "one entrance's handlers can't mix TypeScript and Python files",
                ));
            }
            Ok(ManualTargets::Handlers(locators))
        }
    }
}

/// The language a locator's file belongs to, by extension (declaration files count as
/// TypeScript), or `None` for any other file.
fn target_language(locator: &Locator) -> Option<Language> {
    match locator.path().extension()? {
        "ts" | "tsx" | "mts" | "cts" => Some(Language::TypeScript),
        "py" => Some(Language::Python),
        _ => None,
    }
}
