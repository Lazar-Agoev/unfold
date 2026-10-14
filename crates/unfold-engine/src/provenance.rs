//! Bounded import provenance: which external package export a value comes from.
//!
//! Module-label rule 2 ([`crate::labels`]) uses provenance to name a receiver's package, so
//! `app.get` on `const app = express()` is a call into `express`. It never decides whether
//! anything is callable. The walk spends at most [`MAX_STEPS`] steps, counting the
//! terminal import or annotation:
//!
//! | Value or declaration | Step |
//! | --- | --- |
//! | Property access | Trace the receiver with one fewer step, then append the member name |
//! | Imported identifier | An external package binding with no indexed, excluded, or file candidate: the binding's export, empty path |
//! | Identifier bound to one indexed constant initialized by a call or construction | Trace the initializer's callee with one fewer step, then append `()` |
//! | Identifier bound to one indexed declaration with a written annotation | The annotation's external import, path `[":type"]` |
//! | Anything else | No provenance |

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::evidence::{Evidence, Origin};
use crate::facts::ValueShape;
use crate::labels::external_package_binding;
use crate::language::Linked;
use crate::program::{DeclRef, FactsSet, FileId, ValueRef};

/// The most steps one provenance walk may take.
pub const MAX_STEPS: u32 = 3;

/// A proven import lineage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// The module label of the package the lineage starts at, such as `express`.
    pub module: String,
    /// The imported export: `default`, `*`, or a name.
    pub export: Box<str>,
    /// The steps after the import, in order: member names, `()` for a factory call, or
    /// `:type` for a written annotation.
    pub path: Vec<Box<str>>,
}

/// The provenance of `value`, if one is proven within [`MAX_STEPS`] steps.
pub fn provenance(program: &FactsSet, linked: &dyn Linked, value: ValueRef) -> Option<Provenance> {
    Walk {
        program,
        linked,
        values: FxHashSet::default(),
        declarations: FxHashSet::default(),
    }
    .trace(value, MAX_STEPS)
}

/// The provenance of a callee's receiver: for a property or element access, its receiver;
/// otherwise the value itself. The invoked member doesn't spend its receiver's budget.
pub fn callee_provenance(program: &FactsSet, linked: &dyn Linked, value: ValueRef) -> Option<Provenance> {
    let receiver = program.value(value)?.receiver.map_or(value, |receiver| ValueRef {
        file: value.file,
        value: receiver,
    });
    provenance(program, linked, receiver)
}

struct Walk<'a> {
    program: &'a FactsSet,
    linked: &'a dyn Linked,
    values: FxHashSet<ValueRef>,
    declarations: FxHashSet<DeclRef>,
}

impl Walk<'_> {
    fn trace(&mut self, value: ValueRef, remaining: u32) -> Option<Provenance> {
        if remaining == 0 || !self.values.insert(value) {
            return None;
        }
        let fact = self.program.value(value)?;
        if let (Some(member), Some(receiver)) = (&fact.member, fact.receiver) {
            let receiver = ValueRef {
                file: value.file,
                value: receiver,
            };
            let mut source = self.trace(receiver, remaining - 1)?;
            source.path.push(member.clone());
            return Some(source);
        }
        if fact.shape != ValueShape::Name {
            return None;
        }
        let evidence = self.linked.value_evidence(value);
        if evidence.ambiguous || evidence.unbound_this {
            return None;
        }
        if let Some(direct) = imported(self.linked, value.file, &evidence, Vec::new()) {
            return Some(direct);
        }
        let mut declarations: SmallVec<[DeclRef; 2]> = SmallVec::new();
        for candidate in evidence
            .candidates
            .iter()
            .filter(|candidate| !candidate.augmentation)
        {
            let declaration = candidate
                .declaration
                .filter(|_| candidate.origin == Origin::Indexed)?;
            if !declarations.contains(&declaration) {
                declarations.push(declaration);
            }
        }
        let [declaration] = declarations.as_slice() else {
            return None;
        };
        let declaration = *declaration;
        if !self.declarations.insert(declaration) {
            return None;
        }
        let facts = self.linked.origin_declaration(declaration);
        if let Some(factory) = facts.factory_callee
            && let Some(mut source) = self.trace(factory, remaining - 1)
        {
            source.path.push("()".into());
            return Some(source);
        }
        if !facts.annotation {
            return None;
        }
        let evidence = self.linked.annotation_evidence(declaration)?;
        imported(self.linked, declaration.file, &evidence, vec![":type".into()])
    }
}

/// A terminal import step: an external package binding that no indexed, excluded, or
/// repository-file candidate contradicts.
fn imported(
    linked: &dyn Linked,
    file: FileId,
    evidence: &Evidence,
    path: Vec<Box<str>>,
) -> Option<Provenance> {
    if evidence.ambiguous || evidence.unbound_this {
        return None;
    }
    let binding = external_package_binding(linked, file, evidence.import_binding.as_ref())?;
    let contradicted = evidence
        .effective()
        .any(|candidate| candidate.origin != Origin::External || candidate.is_unindexed_repository());
    if contradicted {
        return None;
    }
    Some(Provenance {
        module: linked.label_for_specifier(file, &binding.specifier),
        export: binding.imported.clone(),
        path,
    })
}
