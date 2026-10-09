//! The short texts that explain an external or unresolved occurrence (architecture §10.1).
//!
//! External occurrences show their module; unresolved ones show a compact reason. The wording
//! is the proof of concept's. A guess never replaces either: the reader never upgrades an
//! unresolved occurrence because its text resembles a known function.

use unfold_core::ModuleLabel;
use unfold_core::vocab::{Reason, Resolution};

/// The compact display text of an unresolved reason.
pub const fn reason_text(reason: Reason) -> &'static str {
    match reason {
        Reason::UnknownName => "unknown name",
        Reason::UnknownReceiver => "receiver type unknown",
        Reason::Parameter => "parameter",
        Reason::MutableBinding => "mutable binding",
        Reason::NotConcrete => "not a concrete function",
        Reason::Accessor => "accessor",
        Reason::Ambiguous => "ambiguous target",
        Reason::Overridden => "overridden by a subclass",
        Reason::UnboundThis => "unbound this",
        Reason::NotIndexed => "not indexed",
        Reason::NotFound => "not found",
        Reason::Dynamic => "dynamic call",
        Reason::Wrapper => "wrapper",
        Reason::Unsupported => "unsupported form",
    }
}

/// The display text of an external origin: the label itself, or several possible origins
/// joined with " or " (`client-a or client-b`).
pub fn module_text(module: &ModuleLabel) -> String {
    match module.labels() {
        Some(labels) if labels.len() > 1 => labels.join(" or "),
        // A malformed stored label is still shown as written rather than hidden.
        _ => module.as_str().to_owned(),
    }
}

/// The note shown with an occurrence or target: its module when external, its reason when
/// unresolved, nothing when resolved (or when the expected part is missing).
pub fn note(resolution: Resolution, reason: Option<Reason>, module: Option<&ModuleLabel>) -> Option<String> {
    match resolution {
        Resolution::Resolved => None,
        Resolution::External => module.map(module_text),
        Resolution::Unresolved => reason.map(|reason| reason_text(reason).to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_has_text() {
        for &reason in Reason::ALL {
            assert!(!reason_text(reason).is_empty());
        }
        assert_eq!(reason_text(Reason::UnknownReceiver), "receiver type unknown");
        assert_eq!(reason_text(Reason::NotConcrete), "not a concrete function");
    }

    #[test]
    fn notes_follow_the_resolution() {
        let express = ModuleLabel::single("express").unwrap();
        let pair = ModuleLabel::from_set(["client-b", "client-a"]).unwrap();
        assert_eq!(
            note(Resolution::External, None, Some(&express)).as_deref(),
            Some("express")
        );
        assert_eq!(
            note(Resolution::External, None, Some(&pair)).as_deref(),
            Some("client-a or client-b")
        );
        assert_eq!(
            note(Resolution::Unresolved, Some(Reason::Parameter), None).as_deref(),
            Some("parameter")
        );
        assert_eq!(note(Resolution::Resolved, None, None), None);
    }
}
