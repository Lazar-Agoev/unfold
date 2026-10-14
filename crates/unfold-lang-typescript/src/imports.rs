//! Which module syntax is a runtime dependency (an `imports` link).
//!
//! | Syntax | Runtime dependency |
//! | --- | --- |
//! | `import "x"`, or an import with a default or namespace binding or a value specifier | Yes |
//! | `import type …`, `export type … from` | No |
//! | Named `import { … } from` / `export { … } from` whose specifiers are all type-only, or empty | Only with `verbatimModuleSyntax` (C-12) |
//! | `export * from`, `export * as ns from` | Yes, unless `export type *` |
//! | `import x = require("x")` | Yes, unless `import type x = …`; `import x = A.B` is not a module |
//! | Dynamic `import(…)` | Yes (a non-literal specifier is `dynamic`) |

use oxc_ast::ast::{
    ExportAllDeclaration, ExportFromDeclaration, ImportDeclaration, ImportDeclarationSpecifier,
    ImportOrExportKind, TSImportEqualsDeclaration, TSModuleReference,
};

/// Whether an `import` declaration loads its module at run time.
pub(crate) fn import_declaration(it: &ImportDeclaration<'_>, verbatim: bool) -> bool {
    if it.import_kind != ImportOrExportKind::Value {
        return false;
    }
    let Some(specifiers) = &it.specifiers else {
        // `import "x"` always runs the module.
        return true;
    };
    let value = specifiers.iter().any(|specifier| match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(named) => named.import_kind == ImportOrExportKind::Value,
        ImportDeclarationSpecifier::ImportDefaultSpecifier(_)
        | ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => true,
    });
    value || verbatim
}

/// Whether an `export { … } from` declaration loads its module at run time.
pub(crate) fn export_from(it: &ExportFromDeclaration<'_>, verbatim: bool) -> bool {
    let value = it
        .specifiers
        .iter()
        .any(|specifier| specifier.export_kind == ImportOrExportKind::Value);
    it.export_kind == ImportOrExportKind::Value && (value || verbatim)
}

/// Whether an `export * from` declaration loads its module at run time.
pub(crate) fn export_all(it: &ExportAllDeclaration<'_>) -> bool {
    it.export_kind == ImportOrExportKind::Value
}

/// Whether `import x = …` loads a module at run time.
pub(crate) fn import_equals(it: &TSImportEqualsDeclaration<'_>) -> bool {
    it.import_kind == ImportOrExportKind::Value
        && matches!(it.module_reference, TSModuleReference::ExternalModuleReference(_))
}
