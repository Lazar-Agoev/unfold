//! Per-file indexes over a module's facts: its module-scope names, import bindings, explicit
//! export links, and star re-exports.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::payload::{DeclarationKind, ExportEntry, ModuleFacts};

/// One link of an export name.
pub(super) enum Link<'f> {
    /// The declarations of a module-scope name written with `export`.
    Declared(&'f str),
    /// A local name (`export { local as name }`).
    Local { local: &'f str, type_only: bool },
    /// An access path (`export default a.b`).
    Path(&'f [Box<str>]),
    /// One declaration (`export default <expression>`).
    Expression(u32),
    /// An export of another module, or the module itself (`*`).
    Module {
        name: &'f str,
        specifier: &'f str,
        type_only: bool,
    },
}

/// Per-file indexes over the module facts.
pub(super) struct Indexed<'f> {
    /// Module-scope name → declaration indices.
    pub(super) declarations: FxHashMap<&'f str, SmallVec<[u32; 2]>>,
    /// Local name → import binding index (the last one written wins).
    pub(super) imports: FxHashMap<&'f str, u32>,
    /// Export name → links.
    pub(super) explicit: FxHashMap<&'f str, Vec<Link<'f>>>,
    /// `export * from` specifiers that aren't type-only.
    pub(super) stars: Vec<&'f str>,
    /// Every `export * from` specifier, type-only ones included (for type lookups).
    pub(super) all_stars: Vec<&'f str>,
}

fn add<'f>(explicit: &mut FxHashMap<&'f str, Vec<Link<'f>>>, name: &'f str, link: Link<'f>) {
    explicit.entry(name).or_default().push(link);
}

/// Builds the per-file indexes of one module's facts.
pub(super) fn index(facts: &ModuleFacts) -> Indexed<'_> {
    let mut indexed = Indexed {
        declarations: FxHashMap::default(),
        imports: FxHashMap::default(),
        explicit: FxHashMap::default(),
        stars: Vec::new(),
        all_stars: Vec::new(),
    };
    for (position, declaration) in facts.declarations.iter().enumerate() {
        let position = position as u32;
        if declaration.kind == DeclarationKind::Module || declaration.kind == DeclarationKind::Expression {
            continue;
        }
        if !declaration.name.is_empty() {
            indexed
                .declarations
                .entry(&*declaration.name)
                .or_default()
                .push(position);
        }
    }
    // A name declared with `export` exports all its declarations; `export default function f`
    // exports `default` only (unless `f` is also exported by name).
    let mut declared: Vec<&str> = Vec::new();
    let mut defaults: Vec<(u32, &str)> = Vec::new();
    for (position, declaration) in facts.declarations.iter().enumerate() {
        if declaration.exported && !declared.contains(&&*declaration.name) {
            declared.push(&declaration.name);
        }
        if declaration.default && declaration.kind != DeclarationKind::Expression {
            defaults.push((position as u32, &declaration.name));
        }
    }
    for name in declared {
        add(&mut indexed.explicit, name, Link::Declared(name));
    }
    for (position, name) in defaults {
        let link = if name.is_empty() {
            Link::Expression(position)
        } else {
            Link::Declared(name)
        };
        add(&mut indexed.explicit, "default", link);
    }
    for (position, binding) in facts.imports.iter().enumerate() {
        indexed.imports.insert(&binding.local, position as u32);
        if binding.exported {
            add(
                &mut indexed.explicit,
                &binding.local,
                Link::Local {
                    local: &binding.local,
                    type_only: binding.type_only,
                },
            );
        }
    }
    for entry in &facts.exports {
        match entry {
            ExportEntry::Local {
                exported,
                local,
                type_only,
            } => add(
                &mut indexed.explicit,
                exported,
                Link::Local {
                    local,
                    type_only: *type_only,
                },
            ),
            ExportEntry::Value {
                exported,
                path,
                declaration,
            } => {
                let link = match (path, declaration) {
                    (Some(path), _) => Link::Path(path),
                    (None, Some(declaration)) => Link::Expression(*declaration),
                    (None, None) => continue,
                };
                add(&mut indexed.explicit, exported, link);
            }
            ExportEntry::Reexport {
                exported,
                imported,
                specifier,
                type_only,
            } => add(
                &mut indexed.explicit,
                exported,
                Link::Module {
                    name: imported,
                    specifier,
                    type_only: *type_only,
                },
            ),
            ExportEntry::Star { specifier, type_only } => {
                indexed.all_stars.push(specifier);
                if !type_only {
                    indexed.stars.push(specifier);
                }
            }
        }
    }
    indexed
}
