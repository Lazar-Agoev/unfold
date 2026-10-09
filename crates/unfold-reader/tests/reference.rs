//! Parity with the proof of concept's reader: the reference canonical graphs are committed to a
//! temporary store (with the projections this crate computes), read back the way the workspace
//! reads them, and every derived view is compared with what the PoC's `reader/core` derived from
//! the same generation (`testdata/reference/poc-reader/`, exported by `tools/oracles`).
//!
//! The graphs are the PoC's own facts, so any difference is a difference in derivation logic.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{Differences, load, reference};
use serde_json::{Value, json};
use unfold_core::Graph;
use unfold_core::vocab::{EntranceKind, Resolution};
use unfold_reader::ReaderIndex;
use unfold_reader::relations::owned_edges;
use unfold_store::Reader;

fn sorted(values: impl IntoIterator<Item = String>) -> Value {
    let set: BTreeSet<String> = values.into_iter().collect();
    json!(set)
}

fn compare_catalog(reader: &Reader, expected: &Value, differences: &mut Differences) {
    let mut actual = BTreeMap::new();
    let mut cursor = None;
    loop {
        let page = reader.definitions(cursor, 500).expect("read definitions");
        for row in page.items {
            actual.insert(
                row.key.to_string(),
                json!({"key": row.key.as_str(), "kind": row.kind.as_str(), "name": &*row.label, "qualifiedName": &*row.qualified_name}),
            );
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let expected: BTreeMap<String, Value> = expected
        .as_array()
        .expect("catalog is an array")
        .iter()
        .map(|item| (item["key"].as_str().unwrap_or_default().to_owned(), item.clone()))
        .collect();
    for key in expected.keys().chain(actual.keys()).collect::<BTreeSet<_>>() {
        differences.compare(
            &format!("catalog {key}"),
            expected.get(key).unwrap_or(&Value::Null),
            actual.get(key).unwrap_or(&Value::Null),
        );
    }
}

fn compare_applications(reader: &Reader, graph: &Graph, expected: &Value, differences: &mut Differences) {
    let memberships = reader.memberships().expect("read memberships");
    let applications: Vec<Value> = reader
        .applications()
        .expect("read applications")
        .iter()
        .map(|application| {
            let launches = graph.entrances.iter().filter(|entrance| {
                entrance.kind == EntranceKind::Process
                    && entrance.targets.first().and_then(|target| target.target.as_ref()) == Some(&application.startup)
            });
            json!({
                "startup": application.startup.as_str(),
                "name": &*application.name,
                "launches": sorted(launches.map(|entrance| entrance.key.to_string())),
                "entrances": sorted(memberships.iter().filter(|m| m.application == application.startup).map(|m| m.entrance.to_string())),
            })
        })
        .collect();
    let placed: BTreeSet<&str> = memberships.iter().map(|m| m.entrance.as_str()).collect();
    let other = graph
        .entrances
        .iter()
        .filter(|entrance| entrance.kind != EntranceKind::Process && !placed.contains(entrance.key.as_str()))
        .map(|entrance| entrance.key.to_string());
    let unindexed = graph
        .entrances
        .iter()
        .filter(|entrance| {
            entrance.kind == EntranceKind::Process
                && entrance
                    .targets
                    .first()
                    .is_none_or(|target| target.resolution != Resolution::Resolved)
        })
        .map(|entrance| entrance.key.to_string());
    let actual = json!({"applications": applications, "otherEntrances": sorted(other), "unindexedLaunches": sorted(unindexed)});
    for part in ["applications", "otherEntrances", "unindexedLaunches"] {
        differences.compare(part, &expected[part], &actual[part]);
    }
}

fn compare_relations(reader: &Reader, index: &ReaderIndex, expected: &Value, differences: &mut Differences) {
    for (key, relations) in expected.as_object().expect("relations is an object") {
        let callers = index.callers(key).unwrap_or_default();
        let mut callers: Vec<Value> = callers
            .iter()
            .map(|caller| json!({"owner": caller.owner.as_str(), "source": caller.source.as_str(), "kind": caller.kind.as_str(), "position": caller.position}))
            .collect();
        callers.sort_by_key(Value::to_string);
        differences.compare(
            &format!("callers of {key}"),
            &relations["callers"],
            &json!(callers),
        );

        let block = reader.block(key).expect("read the block");
        let outgoing: Vec<Value> = match &block {
            Some(located) => {
                let file = reader
                    .file_graph(located.path.as_str())
                    .expect("read the file")
                    .expect("the block's file exists");
                owned_edges(&file, located.block.local_id)
                    .into_iter()
                    .map(|edge| {
                        json!({
                            "source": file.blocks[edge.source as usize].key.as_str(),
                            "kind": edge.kind.as_str(),
                            "target": edge.target.as_ref().map(|target| target.as_str()),
                            "resolution": edge.resolution.as_str(),
                            "reason": edge.reason.map(|reason| reason.as_str()),
                            "module": edge.module.as_ref().map(|module| module.as_str()),
                            "position": edge.position,
                            "text": edge.target.is_none().then_some(&*edge.text),
                        })
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        differences.compare(
            &format!("outgoing of {key}"),
            &relations["outgoing"],
            &json!(outgoing),
        );

        let reached = index.reached_from(key).unwrap_or_default();
        differences.compare(
            &format!("reached from {key}"),
            &relations["reachedFrom"],
            &sorted(reached.iter().map(ToString::to_string)),
        );
    }
}

fn compare_closures(index: &ReaderIndex, graph: &Graph, expected: &Value, differences: &mut Differences) {
    let expected = expected.as_object().expect("closures is an object");
    for entrance in &graph.entrances {
        let key = entrance.key.as_str();
        let closure = index.entrance_closure(key).unwrap_or_default();
        differences.compare(
            &format!("closure of {key}"),
            expected.get(key).unwrap_or(&Value::Null),
            &sorted(closure.iter().map(ToString::to_string)),
        );
    }
    if expected.len() != graph.entrances.len() {
        differences.0.push(format!(
            "closures: the reference has {} entrances, the graph {}",
            expected.len(),
            graph.entrances.len()
        ));
    }
}

fn check(playground: &str, corpus: &str) {
    let loaded = load(corpus);
    let reader = loaded.store.readers.get().expect("get a reader");
    let index = ReaderIndex::load(&reader).expect("load the reader index");
    let mut differences = Differences::default();
    compare_catalog(&reader, &reference(playground, "catalog"), &mut differences);
    compare_applications(
        &reader,
        &loaded.graph,
        &reference(playground, "applications"),
        &mut differences,
    );
    compare_relations(
        &reader,
        &index,
        &reference(playground, "relations"),
        &mut differences,
    );
    compare_closures(
        &index,
        &loaded.graph,
        &reference(playground, "closures"),
        &mut differences,
    );
    differences.assert_none(playground);
}

#[test]
fn typescript_playground_matches_the_poc_reader() {
    check("ts-orders", "playground-ts-native");
}

#[test]
fn python_playground_matches_the_poc_reader() {
    check("py-orders", "python-playground");
}
