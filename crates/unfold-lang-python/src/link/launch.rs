//! Launch targets in an import context, after linking, and the preview of a draft context.
//!
//! A target resolves with the context's ordered import path ([`super::modules`]): a file is its
//! root block; a module is its file, and a package its `__main__.py`; a callable or ASGI target
//! is the block of the module's top-level binding of the attribute
//! ([`Binder::binding_block`]), so an ASGI target and the FastAPI routes bound to the same
//! application compare by key. A module outside the repository, or an attribute the module
//! doesn't bind, is `not_found`; a file that exists but isn't indexed is `not_indexed`.

use std::collections::BTreeSet;
use std::sync::Arc;

use unfold_core::vocab::Reason;
use unfold_engine::evidence::{ModuleTarget, Refusal};
use unfold_engine::launch::{LaunchTarget, PythonContext};
use unfold_engine::{BlockRef, ContextPreview, FileId, LaunchResolution, Outcome};

use super::PythonLinked;
use super::binder::{Ctx, Seen};
use super::modules::ModuleValue;

/// How many newly resolving modules a preview lists.
const PREVIEW_MODULES: usize = 5;

impl PythonLinked<'_> {
    /// Resolves `target` with `context`'s import path.
    pub(crate) fn launch(&self, target: &LaunchTarget, context: &PythonContext) -> LaunchResolution {
        let (module, attribute) = match target {
            LaunchTarget::File { path } => {
                return LaunchResolution {
                    target: self.file_root(path.as_str()),
                    seeds: Vec::new(),
                };
            }
            LaunchTarget::PythonModule { module } => (module, None),
            LaunchTarget::PythonCallable { module, attribute }
            | LaunchTarget::PythonAsgi {
                module, attribute, ..
            } => (module, Some(attribute)),
        };
        let modules = &self.binder.modules;
        let found = modules.launch_lookup(context, false, "", module);
        if found.external.is_some() {
            return unresolved(Reason::NotFound);
        }
        // Importing the module runs the package of each dotted prefix first.
        let parts: Vec<&str> = module.split('.').collect();
        let mut seeds: Vec<BlockRef> = (1..parts.len())
            .flat_map(|length| {
                modules
                    .launch_lookup(context, false, "", &parts[..length].join("."))
                    .paths
            })
            .filter_map(|path| self.root(&path))
            .collect();
        let Some(attribute) = attribute else {
            if found.directories.is_empty() {
                let target = match found.paths.as_slice() {
                    [path] => self.file_root(path),
                    _ => Outcome::unresolved(Reason::NotFound),
                };
                return LaunchResolution { target, seeds };
            }
            // `-m` on a package runs its `__main__`.
            seeds.extend(found.paths.iter().filter_map(|path| self.root(path)));
            let main = modules.launch_lookup(context, false, "", &format!("{module}.__main__"));
            let target = match main.paths.as_slice() {
                [path] if main.directories.is_empty() => self.file_root(path),
                _ => Outcome::unresolved(Reason::NotFound),
            };
            return LaunchResolution { target, seeds };
        };
        let [path] = found.paths.as_slice() else {
            // A namespace package binds no attributes.
            return unresolved(Reason::NotFound);
        };
        let Some(file) = modules.indexed(path) else {
            return unresolved(Reason::NotIndexed);
        };
        seeds.push(BlockRef { file, local: 0 });
        LaunchResolution {
            target: self.attribute(&found, file, attribute),
            seeds,
        }
    }

    /// The block of the top-level binding `attribute` of the module `found` (in `file`),
    /// following imports and aliases to the declaration that writes it.
    fn attribute(&self, found: &ModuleValue, file: FileId, attribute: &str) -> Outcome {
        let binder = &self.binder;
        let program = &binder.program;
        if attribute.contains('.') {
            return Outcome::unresolved(Reason::Unsupported);
        }
        let bound = program
            .names
            .get(attribute)
            .filter(|&member| program.module_binding(file, member).is_some());
        let Some(member) = bound else {
            return Outcome::unresolved(Reason::NotFound);
        };
        let mut ctx = Ctx::new();
        let module = Arc::new(found.clone());
        let value = binder.module_member(&module, member, &Seen::default(), &mut ctx);
        let reason = if value.ambiguous || value.overridden {
            Some(Reason::Ambiguous)
        } else if value.refusal == Some(Refusal::Wrapper) {
            Some(Reason::Wrapper)
        } else if value.refusal.is_some() || value.external.is_some() || value.module.is_some() {
            Some(Reason::Unsupported)
        } else if value.excluded.is_some() {
            Some(Reason::NotIndexed)
        } else if value.missing || value.declarations.is_empty() {
            Some(Reason::NotFound)
        } else if value
            .binding
            .is_some_and(|binding| !binder.stable(binding, &mut ctx))
        {
            Some(Reason::MutableBinding)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Outcome::unresolved(reason);
        }
        match value.declarations.as_slice() {
            [declaration] => binder
                .binding_block(*declaration)
                .map_or(Outcome::unresolved(Reason::Unsupported), Outcome::Resolved),
            _ => Outcome::unresolved(Reason::Ambiguous),
        }
    }

    /// The root block of the file at `path`: `not_indexed` when it exists outside the indexed
    /// files, `not_found` when it doesn't exist.
    fn file_root(&self, path: &str) -> Outcome {
        match self.root(path) {
            Some(root) => Outcome::Resolved(root),
            None if self.binder.modules.is_file(path) => Outcome::unresolved(Reason::NotIndexed),
            None => Outcome::unresolved(Reason::NotFound),
        }
    }

    /// The root block of an indexed file.
    fn root(&self, path: &str) -> Option<BlockRef> {
        self.binder
            .modules
            .indexed(path)
            .map(|file| BlockRef { file, local: 0 })
    }

    /// What `context` reaches and how it resolves the imports there, against the linked
    /// answers.
    pub(crate) fn preview(&self, context: &PythonContext) -> ContextPreview {
        let modules = &self.binder.modules;
        let program = self.binder.program.facts;
        let mut preview = ContextPreview::default();
        let mut newly: BTreeSet<&str> = BTreeSet::new();
        for (file, script) in modules.reached(program, context) {
            let Some(entry) = program.file(file) else {
                continue;
            };
            let Some(facts) = entry.facts.as_deref() else {
                continue;
            };
            preview.reached.push(entry.path.clone());
            let linked = self.dependencies.get(file.index()).map_or(&[][..], Vec::as_slice);
            for (index, import) in facts.graph.imports().iter().enumerate() {
                let Some(specifier) = import.specifier.as_deref() else {
                    continue;
                };
                let under = modules.launch_lookup(context, script, entry.path.as_str(), specifier);
                let resolves = matches!(under.paths.as_slice(), [path] if modules.indexed(path).is_some());
                let now = matches!(linked.get(index), Some(ModuleTarget::Indexed(_)));
                preview.resolved += u32::from(resolves);
                preview.resolved_now += u32::from(now);
                if resolves && !now {
                    newly.insert(specifier);
                }
            }
        }
        preview.reached.sort();
        preview.newly_resolved = newly.into_iter().take(PREVIEW_MODULES).map(Into::into).collect();
        preview
    }
}

fn unresolved(reason: Reason) -> LaunchResolution {
    LaunchResolution {
        target: Outcome::unresolved(reason),
        seeds: Vec::new(),
    }
}
