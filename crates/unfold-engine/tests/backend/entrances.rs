//! Entrance assembly: detector candidates through provenance, handler resolution, evidence,
//! trigger status, target order, sorting, keys with `#n` duplicates, and manual locators.

use unfold_core::Entrance;
use unfold_core::vocab::{
    BlockKind, DetectorId, EntranceKind, HttpMethod, Reason, Resolution, TargetRole, TriggerStatus,
};

use crate::alien::{CONFIG, Project};
use crate::compile;

const APP: &str = "(use express \"express\" :export=default) (const app (call (ref express)))\n";

/// `(text, resolution, reason, target key)` of every target.
fn targets(entrance: &Entrance) -> Vec<(String, Resolution, Option<Reason>, Option<String>)> {
    entrance
        .targets
        .iter()
        .map(|target| {
            (
                target.text.to_string(),
                target.resolution,
                target.reason,
                target.target.as_ref().map(|key| key.as_str().to_owned()),
            )
        })
        .collect()
}

#[test]
fn routes_resolve_handlers_in_declaration_order() {
    let source = format!(
        "{APP}(fn requireUser) (fn createOrder) (class Job)
(stmt (call (get (ref app) post) \"/orders\" (ref requireUser) (ref createOrder)))
(stmt (call (get (ref app) get) (ref path) (call (ref wrap)) (ref Job) (lambda) (spread (ref xs)) (ref nothing)))"
    );
    let compiled = compile(&source);
    let [post, get] = compiled.entrances() else {
        panic!("two routes: {:?}", compiled.entrances());
    };
    assert_eq!(
        (
            post.kind,
            post.method,
            post.trigger.as_deref(),
            post.trigger_status,
            post.detector
        ),
        (
            EntranceKind::Http,
            Some(HttpMethod::Post),
            Some("/orders"),
            TriggerStatus::Exact,
            DetectorId::ExpressRoute
        )
    );
    assert_eq!(
        post.key.as_str(),
        r#"entrance:["express.route","http","POST","/orders","requireUser"]"#
    );
    assert_eq!(
        post.evidence.block.as_ref().map(|key| key.as_str()),
        Some("a.alien#expression_statement[0]/call[0]")
    );
    let evidence_block = compiled.block("a.alien#expression_statement[0]/call[0]");
    assert_eq!(post.evidence.range, evidence_block.range);
    assert!(
        post.targets
            .iter()
            .all(|target| target.role == TargetRole::Handler)
    );
    assert_eq!(
        targets(post),
        [
            (
                "requireUser".into(),
                Resolution::Resolved,
                None,
                Some("a.alien#function:requireUser".into())
            ),
            (
                "createOrder".into(),
                Resolution::Resolved,
                None,
                Some("a.alien#function:createOrder".into())
            ),
        ]
    );
    // A non-literal path leaves the trigger unknown; every handler stays, resolved or not.
    assert_eq!(
        (get.trigger.as_deref(), get.trigger_status),
        (None, TriggerStatus::Unknown)
    );
    assert_eq!(
        get.key.as_str(),
        r#"entrance:["express.route","http","GET","","wrap()"]"#
    );
    assert_eq!(
        targets(get),
        [
            (
                "wrap()".into(),
                Resolution::Unresolved,
                Some(Reason::Wrapper),
                None
            ),
            (
                "Job".into(),
                Resolution::Unresolved,
                Some(Reason::Unsupported),
                None
            ),
            (
                "(…)".into(),
                Resolution::Resolved,
                None,
                Some("a.alien#expression_statement[1]/call[0]/function[0]".into())
            ),
            (
                "(…)".into(),
                Resolution::Unresolved,
                Some(Reason::Unsupported),
                None
            ),
            (
                "nothing".into(),
                Resolution::Unresolved,
                Some(Reason::UnknownName),
                None
            ),
        ]
    );
    assert_eq!(
        get.targets
            .iter()
            .map(|target| target.position)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
}

#[test]
fn constructors_are_not_handlers() {
    let compiled = compile(&format!(
        "{APP}(class K (ctor)) (stmt (call (get (ref app) post) \"/k\" (get (ref K) constructor)))"
    ));
    let target = &compiled.entrances()[0].targets[0];
    assert_eq!(
        (target.resolution, target.reason),
        (Resolution::Unresolved, Some(Reason::Unsupported))
    );
}

#[test]
fn lookalikes_are_not_routes() {
    let compiled = compile(
        "(fn express) (const app (call (ref express))) (fn h) (stmt (call (get (ref app) post) \"/x\" (ref h)))
         (use real \"express\" :export=Router) (const other (call (ref real))) (stmt (call (get (ref other) post) \"/y\" (ref h)))",
    );
    assert!(compiled.entrances().is_empty(), "{:?}", compiled.entrances());
}

#[test]
fn entrances_sort_by_evidence_and_number_duplicates() {
    let route = format!("{APP}(fn h) (stmt (call (get (ref app) post) \"/same\" (ref h)))\n");
    let both = format!("{route}(stmt (call (get (ref app) post) \"/same\" (ref h)))");
    let compiled = Project::new()
        .file("b.alien", &route)
        .file("a.alien", &both)
        .compile()
        .valid();
    let summary: Vec<_> = compiled
        .entrances()
        .iter()
        .map(|entrance| {
            (
                entrance.evidence.path.as_str(),
                entrance.key.as_str().ends_with("#1"),
                entrance.key.as_str().ends_with("#2"),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("a.alien", false, false),
            ("a.alien", true, false),
            ("b.alien", false, true)
        ]
    );
}

#[test]
fn manual_locators_resolve_against_declarations() {
    let source = "(fn work)\n(class Base (method ping))\n(if (ref flag) (ref a))\n(use helper \"lodash\")\n(sig over) (fn over)\n";
    let config = r#"[[entrance]]
kind = "command"
trigger = "work"
handler = "a.alien#work"

[[entrance]]
kind = "command"
trigger = "many"
label = "Many targets"
handler = [
  "a.alien#Base.ping",
  "a.alien:3",
  "missing.alien#x",
  "skipped.alien#x",
  "a.alien#nothing",
  "a.alien#Base.nothing",
  "a.alien#helper",
  "a.alien",
  "a.alien:4",
  "a.alien#over",
]

[[entrance]]
kind = "process"
trigger = "serve"
startup = "a.alien"
"#;
    let compiled = Project::new()
        .file("a.alien", source)
        .unindexed("skipped.alien")
        .config(config)
        .compile()
        .valid();
    let [work, many, serve] = compiled.entrances() else {
        panic!("three manual entrances");
    };
    assert_eq!(work.evidence.pointer.as_deref(), Some("/entrance/0"));
    assert_eq!(work.evidence.path.as_str(), CONFIG);
    assert_eq!(targets(work)[0].3.as_deref(), Some("a.alien#function:work"));
    assert_eq!(many.label.as_deref(), Some("Many targets"));
    let outcomes: Vec<_> = targets(many)
        .into_iter()
        .map(|(_, resolution, reason, key)| (resolution, reason, key))
        .collect();
    let resolved = |key: &str| (Resolution::Resolved, None, Some(key.to_owned()));
    let failed = |reason| (Resolution::Unresolved, Some(reason), None);
    assert_eq!(
        outcomes,
        [
            resolved("a.alien#class:Base/method:ping"),
            resolved("a.alien#if[0]"),
            failed(Reason::NotFound),
            failed(Reason::NotIndexed),
            failed(Reason::NotFound),
            failed(Reason::NotFound),
            (Resolution::External, None, None),
            failed(Reason::Unsupported),
            failed(Reason::Unsupported),
            resolved("a.alien#function:over"),
        ]
    );
    assert_eq!(
        many.targets[6].module.as_ref().map(|module| module.as_str()),
        Some("lodash")
    );
    assert_eq!(many.targets[1].text.as_ref(), "a.alien:3");
    assert_eq!(serve.targets[0].role, TargetRole::Startup);
    assert_eq!(targets(serve)[0].3.as_deref(), Some("a.alien#"));
    assert_eq!(compiled.block("a.alien#").kind, BlockKind::File);
}
