//! The emit map: which source each compiled output comes from, so a launch such as
//! `node dist/server.js` starts `src/server.ts` (architecture §6.3, "Emit map"; the proof of
//! concept's `source/emit.ts`). Nothing is emitted, and outputs never need to exist.
//!
//! Every distinct nearest `tsconfig.json` of an indexed TypeScript file contributes, in path
//! order, unless it sets `noEmit` or `emitDeclarationOnly`:
//!
//! 1. Its members are the folder's TypeScript files (and, with `allowJs`, JavaScript files) that
//!    its own `files`, `include`, and `exclude` select. Without `files` or `include`, `include`
//!    is `**/*`; without `exclude`, it is `node_modules`, `bower_components`, `jspm_packages`,
//!    and the `outDir`. Declaration files contribute nothing.
//! 2. A member's output is the member's path relative to `rootDir` (the configuration's folder
//!    when unset) inside `outDir` (beside the source when unset), with `.ts`/`.tsx` becoming
//!    `.js` (`.jsx` for `.tsx` with `jsx: preserve`), `.mts` `.mjs`, and `.cts` `.cjs`; with
//!    `outFile`, every member's output is that file. A member outside `rootDir`, or an output
//!    outside the folder, has none.
//! 3. An output two different members produce is ambiguous for good.
//!
//! Members include sources that aren't indexed (excluded by the scope, or failed): they still
//! make an output ambiguous, and an output only they produce leads to a source that the launch
//! reports as `not_indexed`.

use rustc_hash::FxHashMap;
use unfold_core::RepoPath;
use unfold_core::vocab::Language;
use unfold_engine::{EmitSource, FactsSet, ProjectInputs};

use crate::config::options::resolve_value;
use crate::config::{Config, ConfigSet, Jsx, Placed};
use crate::paths;

/// Output path → the source that emits it, or ambiguity.
#[derive(Debug, Default)]
pub struct EmitMap {
    outputs: FxHashMap<String, EmitSource>,
}

impl EmitMap {
    /// The emit map of the program's configurations.
    pub(crate) fn build(configs: &ConfigSet, program: &FactsSet, inputs: &ProjectInputs) -> Self {
        let mut used: Vec<&Config> = Vec::new();
        for (_, file) in program.iter() {
            if file.language != Language::TypeScript || file.facts.is_none() {
                continue;
            }
            let config = configs.nearest(&file.path);
            if config.path.is_some() && !used.iter().any(|seen| std::ptr::eq(*seen, &**config)) {
                used.push(config);
            }
        }
        used.sort_by(|a, b| a.path.cmp(&b.path));
        let mut universe: Vec<&str> = program
            .iter()
            .map(|(_, file)| file.path.as_str())
            .chain(inputs.unindexed.iter().map(RepoPath::as_str))
            .collect();
        universe.sort_unstable();
        universe.dedup();
        let mut map = Self::default();
        for config in used {
            map.add_config(config, &universe);
        }
        map
    }

    fn add_config(&mut self, config: &Config, universe: &[&str]) {
        let options = &config.options;
        if options.no_emit.unwrap_or(false) || options.emit_declaration_only.unwrap_or(false) {
            return;
        }
        let allow_js = options.allow_js.unwrap_or(false);
        let out_dir = config.path_option(options.out_dir.as_ref());
        let root_dir = config
            .path_option(options.root_dir.as_ref())
            .unwrap_or_else(|| config.dir.clone());
        let out_file = config.path_option(options.out_file.as_ref());
        let preserve_jsx = options.jsx == Some(Jsx::Preserve);
        let selection = Selection::new(config, out_dir.as_deref());
        for &member in universe {
            if !emits(member, allow_js) || !selection.selects(member) {
                continue;
            }
            let output = match &out_file {
                Some(bundle) => Some(bundle.clone()),
                None => output_path(member, &root_dir, out_dir.as_deref(), preserve_jsx),
            };
            let Some(output) = output.filter(|output| is_javascript(output)) else {
                continue;
            };
            let Ok(source) = RepoPath::new(member) else {
                continue;
            };
            match self.outputs.get(&output) {
                None => {
                    self.outputs.insert(output, EmitSource::Source(source));
                }
                Some(EmitSource::Source(previous)) if *previous != source => {
                    self.outputs.insert(output, EmitSource::Ambiguous);
                }
                Some(_) => {}
            }
        }
    }

    /// The source `output` comes from.
    pub fn source(&self, output: &RepoPath) -> EmitSource {
        self.outputs.get(output.as_str()).cloned().unwrap_or_default()
    }
}

/// Whether a file of this name produces JavaScript.
fn emits(path: &str, allow_js: bool) -> bool {
    let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| path.ends_with(suffix));
    if ends(&[".d.ts", ".d.mts", ".d.cts"]) {
        return false;
    }
    ends(&[".ts", ".tsx", ".mts", ".cts"]) || (allow_js && ends(&[".js", ".jsx", ".mjs", ".cjs"]))
}

fn is_javascript(path: &str) -> bool {
    [".js", ".mjs", ".cjs", ".jsx"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

/// Where `member` is emitted.
fn output_path(member: &str, root_dir: &str, out_dir: Option<&str>, preserve_jsx: bool) -> Option<String> {
    let relative = paths::strip_dir(member, root_dir)?;
    let (stem, extension) = relative.rsplit_once('.')?;
    let extension = match extension {
        "ts" | "js" => "js",
        "tsx" | "jsx" if preserve_jsx => "jsx",
        "tsx" | "jsx" => "js",
        "mts" | "mjs" => "mjs",
        "cts" | "cjs" => "cjs",
        _ => return None,
    };
    let base = out_dir.unwrap_or(root_dir);
    Some(paths::child(base, &format!("{stem}.{extension}")))
}

/// A configuration's file selectors, resolved to folder-relative patterns.
struct Selection {
    files: Vec<String>,
    include: Vec<Vec<String>>,
    exclude: Vec<Vec<String>>,
}

impl Selection {
    fn new(config: &Config, out_dir: Option<&str>) -> Self {
        let resolve = |placed: &Placed<Vec<String>>| -> Vec<String> {
            placed
                .value
                .iter()
                .filter_map(|spec| resolve_value(spec, &placed.dir, &config.dir))
                .collect()
        };
        let files = config.files.as_ref().map(resolve).unwrap_or_default();
        let include = match &config.include {
            Some(include) => resolve(include),
            None if config.files.is_some() => Vec::new(),
            None => vec![paths::child(&config.dir, "**/*")],
        };
        let exclude = match &config.exclude {
            Some(exclude) => resolve(exclude),
            None => ["node_modules", "bower_components", "jspm_packages"]
                .iter()
                .map(|name| paths::child(&config.dir, name))
                .chain(out_dir.map(str::to_owned))
                .collect(),
        };
        Self {
            files,
            include: include.iter().map(|spec| pattern(spec, true)).collect(),
            exclude: exclude.iter().map(|spec| pattern(spec, false)).collect(),
        }
    }

    fn selects(&self, path: &str) -> bool {
        if self.files.iter().any(|file| file == path) {
            return true;
        }
        let parts: Vec<&str> = path.split('/').collect();
        self.include.iter().any(|include| matches(include, &parts))
            && !self.exclude.iter().any(|exclude| {
                // An excluded folder excludes everything under it.
                (1..=parts.len()).any(|length| matches(exclude, &parts[..length]))
            })
    }
}

/// A selector's components; a last component without an extension or wildcard names a folder
/// (`**/*` beneath it for an include, the folder itself for an exclude).
fn pattern(spec: &str, include: bool) -> Vec<String> {
    let mut parts: Vec<String> = spec
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    let implicit = parts.last().is_some_and(|last| !last.contains(['.', '*', '?']));
    if implicit && include {
        parts.push("**".to_owned());
        parts.push("*".to_owned());
    }
    parts
}

/// Whether path components match pattern components (`**` spans folders, `*` and `?` stay in
/// one component, and wildcards never match a name starting with `.`).
fn matches(pattern: &[String], parts: &[&str]) -> bool {
    match pattern.split_first() {
        None => parts.is_empty(),
        Some((first, rest)) if first == "**" => (0..=parts.len()).any(|skip| {
            parts[..skip].iter().all(|part| !part.starts_with('.')) && matches(rest, &parts[skip..])
        }),
        Some((first, rest)) => match parts.split_first() {
            Some((part, others)) => component(first, part) && matches(rest, others),
            None => false,
        },
    }
}

fn component(pattern: &str, name: &str) -> bool {
    if pattern.starts_with(['*', '?']) && name.starts_with('.') {
        return false;
    }
    wildcard(pattern.as_bytes(), name.as_bytes())
}

/// `*` and `?` matching within one component, by backtracking on the last `*`.
fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, n));
            p += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&byte| byte == b'*')
}

#[cfg(test)]
mod tests;
