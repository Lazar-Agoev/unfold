//! `pyproject.script`: console scripts declared in `[project.scripts]`.
//!
//! Each Python file (parsed or not) belongs to its nearest `pyproject.toml`; an excluded nearest
//! one stops the search rather than handing the file to a parent project. Every owning input is
//! read and fingerprinted, even when it declares no scripts. Each declaration is one `process`
//! entrance with one startup target: the literal `module:callable` string resolved against the
//! owning project's source roots, never imported or run. Module and attribute names are literal
//! strings (`importlib` and `getattr` inputs), compared with the exact spelling of files and the
//! NFKC names of declarations. Only a concrete function or method (or a proven callable
//! instance's `__call__`) defined in the named module resolves; anything else is an explicit
//! failure.

use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::RepoPath;
use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind, InputRole, Reason};
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use unfold_engine::evidence::{Refusal, ValueContext};
use unfold_engine::settings::ScopeStatus;
use unfold_engine::{BlockRef, FileId, LanguageProblem, ProjectInputs};

use super::failure;
use super::pyproject::{self, ScriptValue};
use crate::link::answers::decl_of;
use crate::link::binder::{Binder, Ctx, Seen};
use crate::link::modules::{Module, ModuleValue, dirname, join};
use crate::link::program::NodeRef;
use crate::link::value::Value;

/// What the detector found: its candidates, the inputs it read, and their problems.
#[derive(Debug, Default)]
pub(crate) struct Scripts {
    pub(crate) candidates: Vec<EntranceCandidate>,
    /// Every owning `pyproject.toml`, sorted.
    pub(crate) inputs: Vec<RepoPath>,
    pub(crate) problems: Vec<LanguageProblem>,
}

/// Reads every owning `pyproject.toml` and resolves its scripts.
pub(crate) fn scripts(binder: &Binder<'_>, inputs: &ProjectInputs) -> Scripts {
    let mut found = Scripts::default();
    if !inputs.settings.detector_enabled(DetectorId::PyprojectScript) {
        return found;
    }
    for (owner, contexts) in owners(binder, inputs) {
        let Some(input) = inputs.inputs.get(&owner) else {
            continue;
        };
        found.inputs.push(owner.clone());
        let problem = |message: String| LanguageProblem {
            path: owner.clone(),
            message: message.into(),
            incomplete: true,
        };
        let declared = match pyproject::scripts(input.snapshot.text()) {
            Ok(declared) => declared,
            Err(message) => {
                found.problems.push(problem(message));
                continue;
            }
        };
        for script in declared {
            if script.name.is_empty() {
                found
                    .problems
                    .push(problem("a console script has an empty name".to_owned()));
                continue;
            }
            let target = match &script.value {
                ScriptValue::Text(text) => target(binder, &contexts, text),
                ScriptValue::Other(written) => failure(Reason::Unsupported, written.clone()),
            };
            found.candidates.push(EntranceCandidate {
                detector: DetectorId::PyprojectScript,
                kind: EntranceKind::Process,
                method: None,
                trigger: Trigger::Exact(script.name),
                label: None,
                evidence: CandidateEvidence::Input {
                    path: owner.clone(),
                    range: script.range,
                    pointer: script.pointer,
                },
                binding: None,
                targets: vec![target],
            });
        }
    }
    found
}

/// Each owning `pyproject.toml` with one file per distinct set of source roots among the files
/// it owns (the contexts its scripts resolve in).
fn owners(binder: &Binder<'_>, inputs: &ProjectInputs) -> BTreeMap<RepoPath, Vec<FileId>> {
    let mut owners: BTreeMap<RepoPath, Vec<FileId>> = BTreeMap::new();
    let mut seen: FxHashSet<(RepoPath, u32)> = FxHashSet::default();
    let mut nearest: FxHashMap<String, Option<RepoPath>> = FxHashMap::default();
    for (id, file) in binder.program.facts.iter() {
        if file.language != unfold_core::vocab::Language::Python {
            continue;
        }
        let folder = dirname(file.path.as_str());
        let owner = match nearest.get(folder) {
            Some(owner) => owner.clone(),
            None => {
                let owner = owner_of(folder, inputs);
                nearest.insert(folder.to_owned(), owner.clone());
                owner
            }
        };
        let Some(owner) = owner else {
            continue;
        };
        let roots = binder.modules.roots_of(id).unwrap_or_default();
        if seen.insert((owner.clone(), roots)) {
            owners.entry(owner).or_default().push(id);
        }
    }
    owners
}

/// The nearest `pyproject.toml` at or above `folder`, unless that one is out of scope.
fn owner_of(folder: &str, inputs: &ProjectInputs) -> Option<RepoPath> {
    let mut folder = folder;
    loop {
        let candidate = RepoPath::new(&join(folder, "pyproject.toml")).ok();
        if let Some(candidate) = candidate
            && inputs
                .inputs
                .get(&candidate)
                .is_some_and(|input| input.role == InputRole::Pyproject)
        {
            let in_scope = inputs.settings.scope.status(&candidate) == ScopeStatus::InScope;
            return in_scope.then_some(candidate);
        }
        if folder.is_empty() {
            return None;
        }
        folder = dirname(folder);
    }
}

/// One script's startup target, resolved in every context; contexts must agree.
fn target(binder: &Binder<'_>, contexts: &[FileId], text: &str) -> TargetCandidate {
    let fail = |reason| failure(reason, text.into());
    let Some((module, attribute)) = parse(text) else {
        return fail(Reason::Unsupported);
    };
    let outcomes: Vec<Result<BlockRef, Reason>> = contexts
        .iter()
        .map(|&context| resolve(binder, context, module, attribute))
        .collect();
    let Some(first) = outcomes.first() else {
        return fail(Reason::NotFound);
    };
    if outcomes.iter().any(|outcome| outcome != first) {
        return fail(Reason::Ambiguous);
    }
    match *first {
        Ok(block) => TargetCandidate::Block {
            block,
            text: Some(text.into()),
        },
        Err(reason) => fail(reason),
    }
}

/// `module:attribute`, both dotted Python identifiers as written. Call syntax, extras, and
/// whitespace are not entry points this detector proves.
fn parse(text: &str) -> Option<(&str, &str)> {
    let (module, attribute) = text.split_once(':')?;
    let dotted = |part: &str| part.split('.').all(identifier);
    (!attribute.contains(':') && dotted(module) && dotted(attribute)).then_some((module, attribute))
}

fn identifier(piece: &str) -> bool {
    let mut characters = piece.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || unicode_id_start::is_id_start(first))
        && characters.all(|next| next == '_' || unicode_id_start::is_id_continue(next))
}

fn resolve(binder: &Binder<'_>, context: FileId, module: &str, attribute: &str) -> Result<BlockRef, Reason> {
    let modules = &binder.modules;
    // An ordinary module at any prefix keeps importlib from treating a same-spelled folder as
    // a package, even when the leaf exists.
    let parts: Vec<&str> = module.split('.').collect();
    for length in 1..parts.len() {
        let prefix = exact(binder, modules.lookup(context, &parts[..length].join(".")));
        if prefix.ambiguous {
            return Err(Reason::Ambiguous);
        }
        if prefix.missing
            || prefix.external.is_some()
            || prefix.paths.iter().any(|path| !path.ends_with("/__init__.py"))
        {
            return Err(Reason::NotFound);
        }
    }
    let target = Arc::new(exact(binder, modules.lookup(context, module)));
    if target.ambiguous {
        return Err(Reason::Ambiguous);
    }
    if target.missing || target.external.is_some() {
        return Err(Reason::NotFound);
    }
    if target.paths.iter().any(|path| modules.indexed(path).is_none()) {
        return Err(Reason::NotIndexed);
    }
    let mut ctx = Ctx::new();
    let mut names = attribute.split('.');
    let first = names.next().unwrap_or_default();
    let mut value = bound_member(binder, &target, first, &mut ctx);
    for name in names {
        if let Some(class) = value.class
            && !binder.class_concrete(class, &mut ctx)
        {
            let refusal = if binder.wrapper(class, &mut ctx) {
                Refusal::Wrapper
            } else {
                Refusal::Unsupported
            };
            value = Value::refused(refusal);
            break;
        }
        value = match &value.module {
            Some(module) => bound_member(binder, module, name, &mut ctx),
            None => match binder.program.names.get(name) {
                Some(member) => binder.member_value(value, member, &mut Seen::default(), &mut ctx),
                None => Value::default(),
            },
        };
    }
    let block = callable(binder, value, &mut Vec::new(), &mut ctx)?;
    // The startup seeds its own module: a callable re-exported from another module would lose
    // the activation of the module the script imports, so it is withheld.
    let defined_here = match target.paths.as_slice() {
        [path] => binder
            .program
            .facts
            .file(block.file)
            .is_some_and(|file| file.path.as_str() == &**path),
        _ => false,
    };
    if defined_here {
        Ok(block)
    } else {
        Err(Reason::Unsupported)
    }
}

/// A module with only the paths that exist with exactly their spelling.
fn exact(binder: &Binder<'_>, module: ModuleValue) -> ModuleValue {
    let modules = &binder.modules;
    let paths: Vec<Box<str>> = module
        .paths
        .iter()
        .filter(|path| modules.exact(path))
        .cloned()
        .collect();
    let directories: Vec<Box<str>> = module
        .directories
        .iter()
        .filter(|path| modules.exact(path))
        .cloned()
        .collect();
    let found = !module.paths.is_empty() || !module.directories.is_empty();
    if found && paths.is_empty() && directories.is_empty() {
        return ModuleValue {
            paths,
            directories,
            missing: true,
            ..module
        };
    }
    ModuleValue {
        ambiguous: paths.len() > 1 || directories.len() > 1,
        paths,
        directories,
        ..module
    }
}

/// A module's attribute as `getattr` sees it: a binding at the top level of the module's file.
/// A child module that nothing imports is not an attribute, and a namespace package has none.
fn bound_member(binder: &Binder<'_>, module: &Module, name: &str, ctx: &mut Ctx) -> Value {
    let module = Arc::new(exact(binder, (**module).clone()));
    let member = binder.program.names.get(name);
    if module.external.is_none() && !module.ambiguous && !module.missing {
        if module.paths.is_empty() {
            return missing();
        }
        for path in &module.paths {
            let Some(file) = binder.modules.indexed(path) else {
                return Value {
                    excluded: Some(path.clone()),
                    ..Value::default()
                };
            };
            if member
                .and_then(|member| binder.program.module_binding(file, member))
                .is_none()
            {
                return missing();
            }
        }
    }
    match member {
        Some(member) => binder.module_member(&module, member, &Seen::default(), ctx),
        None => missing(),
    }
}

fn missing() -> Value {
    Value {
        missing: true,
        ..Value::default()
    }
}

/// The concrete function or method a value calls, or why there is none.
fn callable(
    binder: &Binder<'_>,
    value: Value,
    classes: &mut Vec<NodeRef>,
    ctx: &mut Ctx,
) -> Result<BlockRef, Reason> {
    if value.ambiguous || value.overridden {
        return Err(Reason::Ambiguous);
    }
    if let Some(refusal) = value.refusal {
        return Err(if refusal == Refusal::Wrapper {
            Reason::Wrapper
        } else {
            Reason::Unsupported
        });
    }
    if value.excluded.is_some() {
        return Err(Reason::NotIndexed);
    }
    if value.missing {
        return Err(Reason::NotFound);
    }
    if value.external.is_some() || value.module.is_some() || value.opaque_module {
        return Err(Reason::Unsupported);
    }
    if value.binding.is_some_and(|binding| !binder.stable(binding, ctx)) {
        return Err(Reason::Unsupported);
    }
    if value.instance
        && let Some(class) = value.class
    {
        if classes.contains(&class) {
            return Err(Reason::Unsupported);
        }
        classes.push(class);
        let call = binder.member_value(value, binder.program.known.call, &mut Seen::default(), ctx);
        return callable(binder, call, classes, ctx);
    }
    if value.declarations.is_empty() {
        return Err(Reason::NotFound);
    }
    let mut targets: Vec<BlockRef> = Vec::new();
    for &declaration in &value.declarations {
        if binder.wrapper(declaration, ctx) {
            return Err(Reason::Wrapper);
        }
        let block = binder
            .concrete_target(decl_of(declaration), ValueContext::Handler)
            .map(|concrete| concrete.target)
            .filter(|&block| {
                binder
                    .program
                    .facts
                    .block(block)
                    .is_some_and(|found| matches!(found.kind, BlockKind::Function | BlockKind::Method))
            })
            .ok_or(Reason::Unsupported)?;
        if !targets.contains(&block) {
            targets.push(block);
        }
    }
    match targets.as_slice() {
        [block] => Ok(*block),
        _ => Err(Reason::Ambiguous),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_points_are_dotted_identifiers() {
        assert_eq!(parse("app:main"), Some(("app", "main")));
        assert_eq!(parse("pkg.mod:Class.method"), Some(("pkg.mod", "Class.method")));
        assert_eq!(parse("app:K"), Some(("app", "K")));
        for invalid in [
            "app.main()",
            "app:main [extra]",
            "app",
            "app:",
            ":main",
            "a:b:c",
            "a :b",
            "1a:b",
        ] {
            assert_eq!(parse(invalid), None, "{invalid}");
        }
    }
}
