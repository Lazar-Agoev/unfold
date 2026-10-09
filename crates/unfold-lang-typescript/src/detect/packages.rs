//! `package.script` and `package.bin`: the processes `package.json` files launch.
//!
//! Every discovered `package.json` outside `node_modules` that scope doesn't exclude is read when
//! either detector is enabled, and fingerprinted even when it declares nothing. One that isn't
//! strict JSON, or isn't an object, is a problem that makes the graph incomplete.
//!
//! - **Scripts.** Each own member of `scripts`, except lifecycle scripts and `pre`/`post` hooks of
//!   another script, is interpreted with the engine's launch grammar
//!   ([`interpret_script`]); a script whose last step launches a module inside the folder is one
//!   `process` entrance named after the script.
//! - **Bins.** A string `bin` is one entrance named after the package (`@scope/tool` → `tool`);
//!   each string member of an object `bin` is one entrance named after its key. Paths are
//!   relative to the package's folder.
//!
//! The one startup target is the launch path, resolved later through the emit map
//! (`dist/server.js` → `src/server.ts`). Evidence is the member in `package.json`: its exact byte
//! range and its JSON pointer. Anything the grammar refuses, and malformed `scripts` or `bin`
//! values, produce no entrance.

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::RepoPath;
use unfold_core::vocab::{DetectorId, EntranceKind, InputRole};
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use unfold_engine::launch::{PackageScripts, ScriptLookup, ScriptOutcome, interpret_script, launch_file};
use unfold_engine::settings::ScopeStatus;
use unfold_engine::{LanguageProblem, ProjectInputs};

use super::json::{self, Member, Node, Value};

/// Scripts that installing, packing, or publishing runs, never enumerated as entrances.
const LIFECYCLE: &[&str] = &[
    "preinstall",
    "install",
    "postinstall",
    "prepublish",
    "preprepare",
    "prepare",
    "postprepare",
    "prepublishOnly",
    "prepack",
    "postpack",
    "dependencies",
];

/// What the package detectors found.
#[derive(Debug, Default)]
pub(crate) struct Packages {
    /// Every `package.json` read, sorted.
    pub(crate) read: Vec<RepoPath>,
    /// The `package.script` candidates, in package order and then written order.
    pub(crate) scripts: Vec<EntranceCandidate>,
    /// The `package.bin` candidates, in package order and then written order.
    pub(crate) bins: Vec<EntranceCandidate>,
    /// Packages that couldn't be read.
    pub(crate) problems: Vec<LanguageProblem>,
}

/// One readable `package.json`.
struct Manifest {
    path: RepoPath,
    root: Node,
}

/// The readable manifests, as the launch grammar looks scripts up.
struct Manifests {
    manifests: Vec<Manifest>,
    by_path: FxHashMap<Box<str>, usize>,
}

impl PackageScripts for Manifests {
    fn script(&self, package: &str, name: &str) -> ScriptLookup<'_> {
        let Some(&index) = self.by_path.get(package) else {
            return ScriptLookup::NoPackage;
        };
        let scripts = self.manifests[index].root.value.member("scripts");
        match scripts.and_then(|scripts| scripts.value.value.member(name)) {
            None => ScriptLookup::Missing,
            Some(member) => match member.value.value.as_str() {
                Some(text) => ScriptLookup::Script(text),
                None => ScriptLookup::NotString,
            },
        }
    }
}

impl Packages {
    /// Reads the packages and finds the enabled detectors' candidates.
    pub(crate) fn new(inputs: &ProjectInputs) -> Self {
        let settings = &inputs.settings;
        let scripts_on = settings.detector_enabled(DetectorId::PackageScript);
        let bins_on = settings.detector_enabled(DetectorId::PackageBin);
        let mut found = Self::default();
        if !scripts_on && !bins_on {
            return found;
        }
        let mut manifests = Manifests {
            manifests: Vec::new(),
            by_path: FxHashMap::default(),
        };
        for (path, input) in &inputs.inputs {
            let selected = input.role == InputRole::Package
                && path.file_name() == "package.json"
                && !path.as_str().split('/').any(|part| part == "node_modules")
                && settings.scope.status(path) != ScopeStatus::Excluded;
            if !selected {
                continue;
            }
            found.read.push(path.clone());
            let problem = |message: String| LanguageProblem {
                path: path.clone(),
                message: message.into(),
                incomplete: true,
            };
            match json::parse(input.snapshot.text()) {
                Ok(root) if root.value.members().is_some() => {
                    manifests
                        .by_path
                        .insert(path.as_str().into(), manifests.manifests.len());
                    manifests.manifests.push(Manifest {
                        path: path.clone(),
                        root,
                    });
                }
                Ok(_) => found
                    .problems
                    .push(problem("package.json must contain a JSON object".to_owned())),
                Err(message) => found.problems.push(problem(format!("invalid JSON: {message}"))),
            }
        }
        let root = inputs
            .root
            .as_deref()
            .map(|root| root.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let launches = Launches {
            root: &root,
            manifests: &manifests,
        };
        for manifest in &manifests.manifests {
            if scripts_on {
                launches.scripts(manifest, &mut found.scripts);
            }
            if bins_on {
                launches.bins(manifest, &mut found.bins);
            }
        }
        found
    }
}

struct Launches<'a> {
    root: &'a str,
    manifests: &'a Manifests,
}

impl Launches<'_> {
    /// The enumerated scripts of one package that launch a module.
    fn scripts(&self, manifest: &Manifest, out: &mut Vec<EntranceCandidate>) {
        let Some(scripts) = manifest.root.value.member("scripts") else {
            return;
        };
        let Some(members) = scripts.value.value.members() else {
            return;
        };
        let names: FxHashSet<&str> = members.iter().map(|member| &*member.key).collect();
        let own = |name: &str| names.contains(name);
        for member in members {
            let name = &*member.key;
            let hook = |prefix: &str| name.strip_prefix(prefix).is_some_and(own);
            if LIFECYCLE.contains(&name) || hook("pre") || hook("post") {
                continue;
            }
            if member.value.value.as_str().is_none() {
                continue;
            }
            let ScriptOutcome::Launch(launch) =
                interpret_script(self.manifests, self.root, manifest.path.as_str(), name)
            else {
                continue;
            };
            let pointer = format!("/scripts/{}", json::pointer_segment(name));
            if let Some(candidate) = self.candidate(
                DetectorId::PackageScript,
                manifest,
                name,
                member,
                pointer,
                (&launch.path, &launch.cwd),
            ) {
                out.push(candidate);
            }
        }
    }

    /// The bins of one package.
    fn bins(&self, manifest: &Manifest, out: &mut Vec<EntranceCandidate>) {
        let Some(bin) = manifest.root.value.member("bin") else {
            return;
        };
        let cwd = manifest
            .path
            .parent()
            .filter(|parent| !parent.is_empty())
            .unwrap_or(".");
        match &bin.value.value {
            Value::String(path) => {
                let Some(name) = manifest
                    .root
                    .value
                    .member("name")
                    .and_then(|name| name.value.value.as_str())
                else {
                    return;
                };
                if path.is_empty() {
                    return;
                }
                // `@scope/tool` installs the command `tool`.
                let command = match name.strip_prefix('@') {
                    Some(_) => name.split_once('/').map_or(name, |(_, command)| command),
                    None => name,
                };
                let candidate = self.candidate(
                    DetectorId::PackageBin,
                    manifest,
                    command,
                    bin,
                    "/bin".to_owned(),
                    (path, cwd),
                );
                out.extend(candidate);
            }
            Value::Object(members) => {
                for member in members {
                    let Some(path) = member.value.value.as_str().filter(|path| !path.is_empty()) else {
                        continue;
                    };
                    let pointer = format!("/bin/{}", json::pointer_segment(&member.key));
                    let candidate = self.candidate(
                        DetectorId::PackageBin,
                        manifest,
                        &member.key,
                        member,
                        pointer,
                        (path, cwd),
                    );
                    out.extend(candidate);
                }
            }
            _ => {}
        }
    }

    /// One `process` entrance launching `path` from `cwd`, unless the launch leaves the folder
    /// or passes through `node_modules`.
    fn candidate(
        &self,
        detector: DetectorId,
        manifest: &Manifest,
        trigger: &str,
        member: &Member,
        pointer: String,
        (path, cwd): (&str, &str),
    ) -> Option<EntranceCandidate> {
        launch_file(self.root, cwd, path)?;
        Some(EntranceCandidate {
            detector,
            kind: EntranceKind::Process,
            method: None,
            trigger: Trigger::Exact(trigger.into()),
            label: None,
            evidence: CandidateEvidence::Input {
                path: manifest.path.clone(),
                range: member.range,
                pointer: pointer.into(),
            },
            targets: vec![TargetCandidate::Launch {
                path: path.into(),
                cwd: cwd.into(),
            }],
        })
    }
}
