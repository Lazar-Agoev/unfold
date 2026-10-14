//! The short texts that explain where a link goes when it is not a project definition
//! (`docs/reader.md`, "Derived views"), and [`LinkEnd`], a link's target as payloads carry it.
//!
//! Library and builtin targets show their module; unknown ones show a compact reason. A guess
//! never replaces either: the reader never upgrades an unknown link because its text resembles a
//! known function. An argument that fills no proven parameter shows its pairing reason the same
//! way ([`pairing_text`]).

use serde::Serialize;
use unfold_core::vocab::{Outcome, PairingReason, Reason};
use unfold_core::{Key, LinkTarget, ModuleLabel};
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

/// The compact display text of why an argument fills no proven parameter.
pub const fn pairing_text(reason: PairingReason) -> &'static str {
    match reason {
        PairingReason::Spread => "spread argument",
        PairingReason::NoParameter => "no parameter takes it",
        PairingReason::Receiver => "receiver binding unknown",
        PairingReason::NoSignature => "no constructor signature",
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

/// The note shown with a link's target: its module for a library or builtin, its reason when
/// unknown, nothing for a project definition.
pub fn note(target: &LinkTarget) -> Option<String> {
    match target {
        LinkTarget::Project(_) => None,
        LinkTarget::Library(module) | LinkTarget::Builtin(module) => Some(module_text(module)),
        LinkTarget::Unknown(reason) => Some(reason_text(*reason).to_owned()),
    }
}

/// Where a link goes, as payloads carry it: its outcome, and the key, module, or reason that
/// outcome names, with the module or reason in words.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LinkEnd {
    /// How the link resolved.
    pub outcome: Outcome,
    /// The project definition, for `project`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<Key>,
    /// The module, for `library` and `builtin`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub module: Option<ModuleLabel>,
    /// Why nothing was proven, for `unknown`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reason: Option<Reason>,
    /// The module or reason in words ([`note`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
}

impl LinkEnd {
    /// The payload form of `target`.
    pub fn of(target: &LinkTarget) -> Self {
        let (key, module, reason) = match target {
            LinkTarget::Project(key) => (Some(key.clone()), None, None),
            LinkTarget::Library(module) | LinkTarget::Builtin(module) => (None, Some(module.clone()), None),
            LinkTarget::Unknown(reason) => (None, None, Some(*reason)),
        };
        Self {
            outcome: target.outcome(),
            key,
            module,
            reason,
            note: note(target),
        }
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
        for &reason in PairingReason::ALL {
            assert!(!pairing_text(reason).is_empty());
        }
    }

    #[test]
    fn notes_follow_the_outcome() {
        let express = ModuleLabel::single("express").unwrap();
        let pair = ModuleLabel::from_set(["client-b", "client-a"]).unwrap();
        assert_eq!(note(&LinkTarget::Library(express)).as_deref(), Some("express"));
        assert_eq!(
            note(&LinkTarget::Builtin(pair.clone())).as_deref(),
            Some("client-a or client-b")
        );
        assert_eq!(
            note(&LinkTarget::Unknown(Reason::Parameter)).as_deref(),
            Some("parameter")
        );
        let project = LinkTarget::Project(Key::from_text("a.py#f"));
        assert_eq!(note(&project), None);
        let end = LinkEnd::of(&project);
        assert_eq!(
            (end.outcome, end.key.as_ref().map(Key::as_str)),
            (Outcome::Project, Some("a.py#f"))
        );
        let end = LinkEnd::of(&LinkTarget::Builtin(pair));
        assert_eq!(end.outcome, Outcome::Builtin);
        assert_eq!(end.note.as_deref(), Some("client-a or client-b"));
    }
}
