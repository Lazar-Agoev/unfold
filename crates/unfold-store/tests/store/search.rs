//! Search candidates for definitions, entrances, and files.

use unfold_core::testkit::sample::{sample, typed};
use unfold_store::{CandidateKind, Reader};

use crate::common::{Prepared, temp_store};

fn keys(reader: &Reader, query: &str, kind: CandidateKind) -> Vec<String> {
    reader
        .search(query, kind, &[], 20)
        .expect("search")
        .into_iter()
        .map(|candidate| {
            assert_eq!(candidate.kind, kind);
            candidate.key.to_string()
        })
        .collect()
}

#[test]
fn finds_definitions_entrances_and_files_by_substring() {
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();

    // A substring inside a camel-case name.
    assert_eq!(
        keys(&reader, "Order", CandidateKind::Definition),
        [
            "src/orders.ts#function:createOrder",
            "src/orders.ts#function:save"
        ]
    );
    assert_eq!(
        keys(&reader, "createorder", CandidateKind::Definition),
        ["src/orders.ts#function:createOrder"]
    );
    // Several terms must all match, in any searched column.
    assert_eq!(
        keys(&reader, "save orders.ts", CandidateKind::Definition),
        ["src/orders.ts#function:save"]
    );
    assert_eq!(keys(&reader, "orders", CandidateKind::File), ["src/orders.ts#"]);
    let entrances = keys(&reader, "orders", CandidateKind::Entrance);
    assert_eq!(entrances.len(), 2, "{entrances:?}");
    assert_eq!(
        keys(&reader, "POST", CandidateKind::Entrance),
        [r#"entrance:["express.route","http","POST","/orders","createOrder"]"#]
    );
    let candidate = &reader
        .search("helper", CandidateKind::Definition, &[], 5)
        .unwrap()[0];
    assert_eq!(&*candidate.name, "helper");
    assert_eq!(&*candidate.path, "src/helper.ts");
    assert_eq!(&*candidate.item_kind, "function");
}

#[test]
fn short_terms_and_odd_characters() {
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();
    // Shorter than a trigram: answered by a scan.
    assert_eq!(
        keys(&reader, "ru", CandidateKind::Definition),
        ["jobs/tasks.py#function:run"]
    );
    assert_eq!(keys(&reader, "py", CandidateKind::File), ["jobs/tasks.py#"]);
    assert!(keys(&reader, "%", CandidateKind::File).is_empty());
    assert!(keys(&reader, "\"quoted\" OR NEAR(", CandidateKind::Definition).is_empty());
    assert!(keys(&reader, "\"quoted\" NEAR(abc", CandidateKind::Definition).is_empty());
    assert!(keys(&reader, "   ", CandidateKind::File).is_empty());
    // Kinds never match.
    assert!(keys(&reader, "function", CandidateKind::Definition).is_empty());
    assert_eq!(
        reader
            .search("e", CandidateKind::Definition, &[], 2)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn definitions_narrow_to_item_kinds() {
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(typed().build()).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();
    let narrowed = |query: &str, item_kinds: &[&str]| -> Vec<(String, String)> {
        let mut found: Vec<(String, String)> = reader
            .search(query, CandidateKind::Definition, item_kinds, 20)
            .unwrap()
            .into_iter()
            .map(|candidate| (candidate.key.to_string(), candidate.item_kind.to_string()))
            .collect();
        found.sort();
        found
    };
    let pair = |key: &str, kind: &str| (key.to_owned(), kind.to_owned());
    // `cart` matches the declarations of `src/cart.ts` by name or path.
    assert_eq!(
        narrowed("cart", &[]),
        [
            pair("src/cart.ts#enum:Status", "enum"),
            pair("src/cart.ts#interface:Cart", "interface"),
            pair("src/cart.ts#type_alias:CartId", "type_alias"),
        ]
    );
    assert_eq!(
        narrowed("cart", &["interface", "enum", "class"]),
        [
            pair("src/cart.ts#enum:Status", "enum"),
            pair("src/cart.ts#interface:Cart", "interface"),
        ]
    );
    // A short term is answered by a scan, narrowed the same way.
    assert_eq!(
        narrowed("ca", &["type_alias"]),
        [pair("src/cart.ts#type_alias:CartId", "type_alias")]
    );
    assert!(narrowed("cart", &["function"]).is_empty());
}

#[test]
fn search_rows_follow_updates() {
    let (_dir, mut store) = temp_store();
    let before = Prepared::new(sample().build());
    store.writer.commit(&before.first_compile()).unwrap();
    let mut graph = before.graph.clone();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "jobs/tasks.py");
    graph
        .entrances
        .retain(|entrance| entrance.evidence.path.as_str() != ".unfold/config.toml");
    graph.normalize();
    let after = Prepared::new(graph);
    store.writer.commit(&after.since(&before)).unwrap();
    let reader = store.readers.get().unwrap();
    assert!(keys(&reader, "run", CandidateKind::Definition).is_empty());
    assert!(keys(&reader, "tasks", CandidateKind::File).is_empty());
    assert!(keys(&reader, "jobs", CandidateKind::Entrance).is_empty());
    assert_eq!(keys(&reader, "helper", CandidateKind::File), ["src/helper.ts#"]);
}
