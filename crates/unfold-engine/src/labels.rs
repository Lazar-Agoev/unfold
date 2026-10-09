//! Module labels for external outcomes.
//!
//! A label names an outcome the policy already classified as external; it never establishes a
//! target. Languages spell labels ([`Linked::label_for_specifier`] and friends); the order of
//! the rules is shared. The first rule that applies wins:
//!
//! 1. **Import-rooted:** the package an external, non-relative import binding of the access root
//!    names.
//! 2. **Origin-rooted:** the package the receiver's bounded provenance comes from
//!    ([`crate::provenance`]), so `app.get` on `const app = express()` names `express`.
//! 3. **Ambient global:** an undeclared, unimported identifier root, such as `console`.
//! 4. **Declaration locations:** the labels of the effective candidates' locations, as one plain
//!    label or a sorted JSON array.

use unfold_core::ModuleLabel;

use crate::error::{BackendError, path_of};
use crate::evidence::{Candidate, Evidence, ExternalLocation, ImportBinding, ImportTarget};
use crate::facts::ValueShape;
use crate::language::Linked;
use crate::program::{FactsSet, FileId, ValueRef};
use crate::provenance;
use crate::resolve::{Expr, Resolver};

/// The binding, if it is an external package import: resolved outside the repository, not at a
/// repository file, and written with a package specifier.
pub fn external_package_binding<'e>(
    linked: &dyn Linked,
    file: FileId,
    binding: Option<&'e ImportBinding>,
) -> Option<&'e ImportBinding> {
    binding.filter(|binding| {
        binding.target == ImportTarget::External
            && !matches!(binding.location, Some(ExternalLocation::File(_)))
            && linked.is_package_specifier(file, &binding.specifier)
    })
}

/// Rule 4 on its own: the label of a set of external candidates, from their locations. Every
/// candidate needs a location.
pub fn location_label<'c>(
    program: &FactsSet,
    linked: &dyn Linked,
    file: FileId,
    text: &str,
    candidates: impl IntoIterator<Item = &'c Candidate>,
) -> Result<ModuleLabel, BackendError> {
    let mut labels = Vec::new();
    for candidate in candidates {
        let Some(location) = &candidate.location else {
            return Err(missing(program, file, text));
        };
        let label = linked.label_for_location(file, location);
        checked(program, file, &label)?;
        labels.push(label);
    }
    ModuleLabel::from_set(labels.iter().map(String::as_str)).ok_or_else(|| missing(program, file, text))
}

/// The label of an external outcome for `expr` with `evidence`, by the four rules.
pub(crate) fn external_label(
    resolver: &Resolver<'_>,
    expr: Expr,
    evidence: &Evidence,
) -> Result<ModuleLabel, BackendError> {
    let program = resolver.program();
    let linked = resolver.linked();
    let file = expr.file;
    if let Some(binding) = external_package_binding(linked, file, evidence.import_binding.as_ref()) {
        return checked(
            program,
            file,
            &linked.label_for_specifier(file, &binding.specifier),
        );
    }
    if let Some(origin) = expr
        .value_ref()
        .and_then(|value| provenance::callee_provenance(program, linked, value))
    {
        return checked(program, file, &origin.module);
    }
    let root_is_name = expr
        .value_ref()
        .and_then(|value| access_root(program, value))
        .is_some_and(|root| root == ValueShape::Name);
    if root_is_name && evidence.import_binding.is_none() && evidence.root_is_indexed == Some(false) {
        return checked(program, file, &linked.global_label(file));
    }
    let text = expr.fact(resolver).map_or("(…)", |fact| &fact.label);
    location_label(program, linked, file, text, evidence.effective())
}

/// The shape of a value's access root: the value itself, or the innermost receiver of a chain
/// of property and element accesses.
fn access_root(program: &FactsSet, value: ValueRef) -> Option<ValueShape> {
    let mut fact = program.value(value)?;
    // Receivers precede their values (checked when facts are built), so this ends.
    while let Some(receiver) = fact.receiver {
        fact = program.value(ValueRef {
            file: value.file,
            value: receiver,
        })?;
    }
    Some(fact.shape)
}

fn missing(program: &FactsSet, file: FileId, text: &str) -> BackendError {
    BackendError::MissingModuleEvidence {
        path: path_of(program, file),
        text: text.into(),
    }
}

fn checked(program: &FactsSet, file: FileId, label: &str) -> Result<ModuleLabel, BackendError> {
    ModuleLabel::single(label).ok_or_else(|| BackendError::InvalidModuleLabel {
        path: path_of(program, file),
        label: label.into(),
    })
}
