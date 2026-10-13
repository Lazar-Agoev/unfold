//! Strict parsing of `unfold.toml`: serde rejects unknown keys and wrong types, then each value
//! is validated with its exact position in the file.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use toml::Spanned;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Language};
use unfold_core::{ByteRange, RepoPath};

use super::{
    Application, EntranceOverride, Evidence, FolderPath, ManualEntrance, ManualTargets, OverrideMatch,
    Process, ProcessStart, PythonProject, PythonVersion, SETTINGS_PATH, Scope, Settings,
};
use crate::launch::LaunchTarget;
use crate::locator::Locator;

/// The longest application name, in characters.
const MAX_NAME: usize = 120;
/// The longest `id`, in characters (a valid id is ASCII).
const MAX_ID: usize = 64;
/// The longest description, in characters.
const MAX_DESCRIPTION: usize = 2000;
/// The longest schedule, in characters.
const MAX_SCHEDULE: usize = 200;
/// The detectors whose launches `[[entrance_override]]` can match.
const OVERRIDABLE: [DetectorId; 3] = [
    DetectorId::PackageScript,
    DetectorId::PackageBin,
    DetectorId::PyprojectScript,
];

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
    detectors: Option<RawDetectors>,
    python: Option<RawPython>,
    application: Option<Vec<Spanned<RawApplication>>>,
    entrance: Option<Vec<Spanned<RawEntrance>>>,
    entrance_override: Option<Vec<Spanned<RawOverride>>>,
    evidence: Option<RawEvidence>,
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
struct RawApplication {
    id: Spanned<String>,
    name: Spanned<String>,
    description: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntrance {
    id: Spanned<String>,
    kind: Spanned<String>,
    application: Option<Spanned<String>>,
    trigger: Option<Spanned<String>>,
    method: Option<Spanned<String>>,
    label: Option<Spanned<String>>,
    description: Option<Spanned<String>>,
    handler: Option<Spanned<RawHandler>>,
    cwd: Option<Spanned<String>>,
    command: Option<Spanned<String>>,
    startup: Option<Spanned<String>>,
    target: Option<Spanned<RawTarget>>,
    python_paths: Option<Spanned<Vec<Spanned<String>>>>,
    env: Option<Spanned<BTreeMap<Spanned<String>, Spanned<String>>>>,
    schedule: Option<Spanned<String>>,
}

impl RawEntrance {
    /// The process-only keys this table sets, with their spans, in key order.
    fn process_keys(&self) -> impl Iterator<Item = (&'static str, Range<usize>)> {
        [
            ("cwd", self.cwd.as_ref().map(Spanned::span)),
            ("command", self.command.as_ref().map(Spanned::span)),
            ("startup", self.startup.as_ref().map(Spanned::span)),
            ("target", self.target.as_ref().map(Spanned::span)),
            ("python_paths", self.python_paths.as_ref().map(Spanned::span)),
            ("env", self.env.as_ref().map(Spanned::span)),
            ("schedule", self.schedule.as_ref().map(Spanned::span)),
        ]
        .into_iter()
        .filter_map(|(key, span)| Some((key, span?)))
    }

    /// Where the table's last value ends.
    fn end(&self) -> Option<usize> {
        [
            Some(self.id.span()),
            Some(self.kind.span()),
            self.application.as_ref().map(Spanned::span),
            self.trigger.as_ref().map(Spanned::span),
            self.method.as_ref().map(Spanned::span),
            self.label.as_ref().map(Spanned::span),
            self.description.as_ref().map(Spanned::span),
            self.handler.as_ref().map(Spanned::span),
        ]
        .into_iter()
        .flatten()
        .chain(self.process_keys().map(|(_, span)| span))
        .map(|span| span.end)
        .max()
    }
}

/// `target = { type = "…", … }`. Every field is optional here so that a field the type doesn't
/// take is reported at its own position.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTarget {
    #[serde(rename = "type")]
    kind: Spanned<String>,
    path: Option<Spanned<String>>,
    module: Option<Spanned<String>>,
    attribute: Option<Spanned<String>>,
    factory: Option<Spanned<bool>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOverride {
    #[serde(rename = "match")]
    selector: Spanned<RawMatch>,
    application: Option<Spanned<String>>,
    label: Option<Spanned<String>>,
    description: Option<Spanned<String>>,
    schedule: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMatch {
    detector: Spanned<String>,
    path: Spanned<String>,
    pointer: Spanned<String>,
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
    let applications = applications(raw.application.unwrap_or_default(), source)?;
    let mut entrances: Vec<ManualEntrance> = Vec::new();
    for (index, table) in raw.entrance.unwrap_or_default().into_iter().enumerate() {
        let id_span = table.get_ref().id.span();
        let entrance = entrance(index, table, &applications, source)?;
        if entrances.iter().any(|other| other.id == entrance.id) {
            return Err(source.error(
                id_span,
                format!("two `[[entrance]]` tables have the id `{}`", entrance.id),
            ));
        }
        entrances.push(entrance);
    }
    let overrides = overrides(raw.entrance_override.unwrap_or_default(), &applications, source)?;
    let evidence = match raw.evidence.and_then(|evidence| evidence.files) {
        Some(files) => evidence(files, source)?,
        None => Evidence::default(),
    };
    Ok(Settings {
        languages,
        scope,
        disabled_detectors,
        python_projects,
        applications,
        entrances,
        overrides,
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

/// An `id`: 1 to 64 ASCII letters, digits, `.`, `_`, or `-`, starting with a letter or digit.
fn id(id: &Spanned<String>, source: &Source<'_>) -> Result<Box<str>, SettingsError> {
    let text = id.get_ref();
    let valid = text.len() <= MAX_ID
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'));
    if valid {
        return Ok(text.as_str().into());
    }
    Err(source.error(
        id.span(),
        format!(
            "invalid id \"{text}\"; use 1 to {MAX_ID} ASCII letters, digits, `.`, `_`, or `-`, starting with a letter or digit"
        ),
    ))
}

/// Text that must not be blank and has at most `max` characters, kept as written.
fn text(
    value: Option<Spanned<String>>,
    key: &str,
    max: usize,
    source: &Source<'_>,
) -> Result<Option<Box<str>>, SettingsError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.get_ref().trim().is_empty() {
        return Err(source.error(
            value.span(),
            format!("`{key}` must not be blank; omit it instead"),
        ));
    }
    if value.get_ref().chars().count() > max {
        return Err(source.error(
            value.span(),
            format!("`{key}` must have at most {max} characters"),
        ));
    }
    Ok(Some(value.into_inner().into()))
}

/// An `application` reference, which must name a declared `[[application]]`.
fn application_ref(
    value: Option<Spanned<String>>,
    applications: &[Application],
    source: &Source<'_>,
) -> Result<Option<Box<str>>, SettingsError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if applications
        .iter()
        .any(|application| *application.id == **value.get_ref())
    {
        return Ok(Some(value.into_inner().into()));
    }
    Err(source.error(
        value.span(),
        format!("no `[[application]]` has the id `{}`", value.get_ref()),
    ))
}

fn applications(
    raw: Vec<Spanned<RawApplication>>,
    source: &Source<'_>,
) -> Result<Vec<Application>, SettingsError> {
    let mut applications: Vec<Application> = Vec::new();
    for table in raw {
        let raw = table.into_inner();
        let id = id(&raw.id, source)?;
        if applications.iter().any(|application| application.id == id) {
            return Err(source.error(
                raw.id.span(),
                format!("two `[[application]]` tables have the id `{id}`"),
            ));
        }
        let name = text(Some(raw.name), "name", MAX_NAME, source)?.expect("a name was given");
        let description = text(raw.description, "description", MAX_DESCRIPTION, source)?;
        applications.push(Application {
            id,
            name,
            description,
        });
    }
    Ok(applications)
}

fn entrance(
    index: usize,
    table: Spanned<RawEntrance>,
    applications: &[Application],
    source: &Source<'_>,
) -> Result<ManualEntrance, SettingsError> {
    let span = table.span();
    let span_start = span.start;
    let raw = table.into_inner();
    let id = id(&raw.id, source)?;
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
    let end = raw.end().unwrap_or(span.end).max(span.end);
    let evidence = source.shifted(span.start..end);
    let targets = if kind == EntranceKind::Process {
        if let Some(handler) = &raw.handler {
            return Err(source.error(
                handler.span(),
                "a `process` entrance takes `command`, `startup`, or `target`, not `handler`",
            ));
        }
        ManualTargets::Process(process(&raw, span, source)?)
    } else {
        if let Some((key, span)) = raw.process_keys().next() {
            return Err(source.error(
                span,
                format!("only `process` entrances have `{key}`; a `{kind}` entrance takes `handler`"),
            ));
        }
        ManualTargets::Handlers(handlers(kind, raw.handler, span, source)?)
    };
    let application = application_ref(raw.application, applications, source)?;
    let description = text(raw.description, "description", MAX_DESCRIPTION, source)?;
    Ok(ManualEntrance {
        id,
        kind,
        application,
        trigger: raw.trigger.map(|trigger| trigger.into_inner().into()),
        method,
        label: raw.label.map(|label| label.into_inner().into()),
        description,
        targets,
        index,
        span: evidence,
        line: source.line(span_start),
    })
}

/// The process fields of a `process` entrance's table.
fn process(raw: &RawEntrance, span: Range<usize>, source: &Source<'_>) -> Result<Process, SettingsError> {
    let folder = |value: &Spanned<String>| {
        FolderPath::parse(value.get_ref()).map_err(|error| {
            source.error(
                value.span(),
                format!("invalid folder `{}`: {error}", value.get_ref()),
            )
        })
    };
    let cwd = match &raw.cwd {
        Some(cwd) => folder(cwd)?,
        None => FolderPath::ROOT,
    };
    let command = text(raw.command.clone(), "command", usize::MAX, source)?;
    let start = match (&raw.startup, &raw.target) {
        (Some(_), Some(target)) => {
            return Err(source.error(
                target.span(),
                "a `process` entrance takes `startup` or `target`, not both",
            ));
        }
        (Some(startup), None) => Some(ProcessStart::Startup(
            Locator::parse(startup.get_ref())
                .map_err(|error| source.error(startup.span(), error.to_string()))?,
        )),
        (None, Some(target)) => Some(ProcessStart::Target(self::target(target, source)?)),
        (None, None) => None,
    };
    if command.is_none() && start.is_none() {
        return Err(source.error(
            span,
            "a `process` entrance needs a `command`, `startup`, or `target`",
        ));
    }
    let python_paths = match &raw.python_paths {
        None => None,
        Some(list) => {
            let mut paths: Vec<FolderPath> = Vec::new();
            for value in list.get_ref() {
                let path = folder(value)?;
                if paths.contains(&path) {
                    return Err(source.error(value.span(), format!("duplicate Python path `{path}`")));
                }
                paths.push(path);
            }
            Some(paths)
        }
    };
    let mut env = BTreeMap::new();
    for (name, value) in raw.env.iter().flat_map(|env| env.get_ref()) {
        if !is_env_name(name.get_ref()) {
            return Err(source.error(
                name.span(),
                format!(
                    "invalid environment variable name `{}`; use ASCII letters, digits, and `_`, not starting with a digit",
                    name.get_ref()
                ),
            ));
        }
        if value.get_ref().contains('\0') {
            return Err(source.error(value.span(), "an environment value can't contain NUL"));
        }
        env.insert(name.get_ref().as_str().into(), value.get_ref().as_str().into());
    }
    let schedule = text(raw.schedule.clone(), "schedule", MAX_SCHEDULE, source)?;
    Ok(Process {
        cwd,
        command,
        start,
        python_paths,
        env,
        schedule,
    })
}

/// Whether `name` is a shell variable name: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_env_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// A `target` table: each `type` takes exactly its own fields.
fn target(table: &Spanned<RawTarget>, source: &Source<'_>) -> Result<LaunchTarget, SettingsError> {
    let raw = table.get_ref();
    let fields = [
        ("path", raw.path.as_ref().map(Spanned::span)),
        ("module", raw.module.as_ref().map(Spanned::span)),
        ("attribute", raw.attribute.as_ref().map(Spanned::span)),
        ("factory", raw.factory.as_ref().map(Spanned::span)),
    ];
    let kind = raw.kind.get_ref().as_str();
    let takes: &[&str] = match kind {
        "file" => &["path"],
        "python_module" => &["module"],
        "python_callable" => &["module", "attribute"],
        "python_asgi" => &["module", "attribute", "factory"],
        _ => {
            return Err(source.error(
                raw.kind.span(),
                format!(
                    "unknown target type `{kind}`; use file, python_module, python_callable, or python_asgi"
                ),
            ));
        }
    };
    for (field, span) in fields {
        if let Some(span) = span
            && !takes.contains(&field)
        {
            return Err(source.error(span, format!("a `{kind}` target doesn't take `{field}`")));
        }
    }
    let missing = |field: &str| source.error(table.span(), format!("a `{kind}` target needs `{field}`"));
    let dotted = |value: &Spanned<String>| -> Result<Box<str>, SettingsError> {
        if is_dotted_name(value.get_ref()) {
            return Ok(value.get_ref().as_str().into());
        }
        Err(source.error(
            value.span(),
            format!("`{}` is not a dotted Python name", value.get_ref()),
        ))
    };
    Ok(match kind {
        "file" => {
            let path = raw.path.as_ref().ok_or_else(|| missing("path"))?;
            LaunchTarget::File {
                path: RepoPath::new(path.get_ref()).map_err(|error| {
                    source.error(path.span(), format!("invalid path `{}`: {error}", path.get_ref()))
                })?,
            }
        }
        "python_module" => LaunchTarget::PythonModule {
            module: dotted(raw.module.as_ref().ok_or_else(|| missing("module"))?)?,
        },
        "python_callable" => LaunchTarget::PythonCallable {
            module: dotted(raw.module.as_ref().ok_or_else(|| missing("module"))?)?,
            attribute: dotted(raw.attribute.as_ref().ok_or_else(|| missing("attribute"))?)?,
        },
        _ => LaunchTarget::PythonAsgi {
            module: dotted(raw.module.as_ref().ok_or_else(|| missing("module"))?)?,
            attribute: dotted(raw.attribute.as_ref().ok_or_else(|| missing("attribute"))?)?,
            factory: raw.factory.as_ref().is_some_and(|factory| *factory.get_ref()),
        },
    })
}

/// Whether `name` is identifiers joined by `.`, such as `app.main`.
fn is_dotted_name(name: &str) -> bool {
    name.split('.').all(|part| {
        part.chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
            && part
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_')
    })
}

fn handlers(
    kind: EntranceKind,
    handler: Option<Spanned<RawHandler>>,
    span: Range<usize>,
    source: &Source<'_>,
) -> Result<Vec<Locator>, SettingsError> {
    let locator = |text: &str, span: Range<usize>| {
        Locator::parse(text).map_err(|error| source.error(span, error.to_string()))
    };
    let Some(handler) = handler else {
        return Err(source.error(
            span,
            format!("a `{kind}` entrance needs a `handler` locator or list"),
        ));
    };
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
    Ok(locators)
}

fn overrides(
    raw: Vec<Spanned<RawOverride>>,
    applications: &[Application],
    source: &Source<'_>,
) -> Result<Vec<EntranceOverride>, SettingsError> {
    let mut overrides: Vec<EntranceOverride> = Vec::new();
    for table in raw {
        let line = source.line(table.span().start);
        let raw = table.into_inner();
        let selector_span = raw.selector.span();
        let selector = raw.selector.into_inner();
        let detector = selector
            .detector
            .get_ref()
            .parse::<DetectorId>()
            .ok()
            .filter(|detector| OVERRIDABLE.contains(detector))
            .ok_or_else(|| {
                source.error(
                    selector.detector.span(),
                    format!(
                        "an override matches a launch of package.script, package.bin, or pyproject.script, not `{}`",
                        selector.detector.get_ref()
                    ),
                )
            })?;
        let path = RepoPath::new(selector.path.get_ref()).map_err(|error| {
            source.error(
                selector.path.span(),
                format!("invalid path `{}`: {error}", selector.path.get_ref()),
            )
        })?;
        if !is_pointer(selector.pointer.get_ref()) {
            return Err(source.error(
                selector.pointer.span(),
                format!(
                    "`{}` is not a JSON pointer such as `/scripts/start`",
                    selector.pointer.get_ref()
                ),
            ));
        }
        let selector = OverrideMatch {
            detector,
            path,
            pointer: selector.pointer.into_inner().into(),
        };
        if overrides.iter().any(|other| other.selector == selector) {
            return Err(source.error(
                selector_span,
                "two `[[entrance_override]]` tables have the same `match`",
            ));
        }
        overrides.push(EntranceOverride {
            selector,
            application: application_ref(raw.application, applications, source)?,
            label: raw.label.map(|label| label.into_inner().into()),
            description: text(raw.description, "description", MAX_DESCRIPTION, source)?,
            schedule: text(raw.schedule, "schedule", MAX_SCHEDULE, source)?,
            line,
        });
    }
    Ok(overrides)
}

/// Whether `pointer` is an RFC 6901 pointer to a member: it starts with `/` and every `~`
/// begins `~0` or `~1`.
fn is_pointer(pointer: &str) -> bool {
    pointer.starts_with('/')
        && pointer
            .match_indices('~')
            .all(|(at, _)| matches!(pointer.as_bytes().get(at + 1), Some(b'0' | b'1')))
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
