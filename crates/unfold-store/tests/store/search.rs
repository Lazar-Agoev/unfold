//! The name search: definitions, apps, entrances, and files by substring, in relevance order.

use unfold_core::testkit::sample::shop;
use unfold_store::{CandidateKind, Reader};

use crate::common::{Prepared, temp_store};

fn keys(reader: &Reader, query: &str, kind: CandidateKind) -> Vec<String> {
    keys_of(reader, query, kind, &[])
}

fn keys_of(reader: &Reader, query: &str, kind: CandidateKind, item_kinds: &[&str]) -> Vec<String> {
    reader
        .search(query, kind, item_kinds, 20)
        .expect("search")
        .into_iter()
        .map(|candidate| {
            assert_eq!(candidate.kind, kind);
            candidate.key.to_string()
        })
        .collect()
}

fn committed() -> (tempfile::TempDir, unfold_store::Store, Prepared) {
    let (dir, mut store) = temp_store();
    let prepared = Prepared::new(shop().build());
    store
        .writer
        .commit(&prepared.first_compile())
        .expect("first compile");
    (dir, store, prepared)
}

#[test]
fn finds_definitions_apps_entrances_and_files_by_substring() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();

    // A substring inside a name, ignoring case.
    assert_eq!(
        keys(&reader, "Card", CandidateKind::Definition),
        ["shop/payments.py#charge_card"]
    );
    assert_eq!(
        keys(&reader, "loadorder", CandidateKind::Definition),
        ["src/orders.ts#loadOrder"]
    );
    // Several terms must all match, each in the name or the qualified name.
    let mut both = keys(&reader, "load order", CandidateKind::Definition);
    both.sort();
    assert_eq!(both, ["shop/store.py#load_order", "src/orders.ts#loadOrder"]);
    assert_eq!(
        keys(&reader, "payments", CandidateKind::File),
        ["shop/payments.py#"]
    );
    assert_eq!(keys(&reader, "shop", CandidateKind::App), ["entrance:shop"]);
    assert_eq!(
        keys(&reader, "python -m", CandidateKind::App),
        ["entrance:shop"],
        "an app is found by its run command"
    );
    // An entrance is found by its name, its app's name, or what starts it.
    assert_eq!(
        keys(&reader, "refund", CandidateKind::Entrance),
        ["entrance:shop/refund-an-order"]
    );
    assert_eq!(
        keys(&reader, "/api/", CandidateKind::Entrance),
        ["entrance:shop/orders-api"]
    );
    assert_eq!(keys(&reader, "shop", CandidateKind::Entrance).len(), 4);

    let candidate = &reader
        .search("charge_card", CandidateKind::Definition, &[], 5)
        .unwrap()[0];
    assert_eq!(&*candidate.name, "charge_card");
    assert_eq!(&*candidate.qualified_name, "charge_card");
    assert_eq!(&*candidate.path, "shop/payments.py");
    assert_eq!(&*candidate.item_kind, "function");
    let entrance = &reader.search("refund", CandidateKind::Entrance, &[], 5).unwrap()[0];
    assert_eq!(&*entrance.name, "Refund an order");
    // Its app and what starts it, as Add shows them.
    assert_eq!(&*entrance.qualified_name, "Shop · POST /orders/{order_id}/refund");
    assert_eq!(&*entrance.path, "");
    assert_eq!(&*entrance.item_kind, "entrance");
}

#[test]
fn definitions_match_by_name_and_files_by_path() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();
    // `store` is in the path of every definition of store.py, but in the name of its module
    // only (whose label is its path): a file's or folder's name finds no definition in it.
    assert_eq!(
        keys(&reader, "store", CandidateKind::Definition),
        ["shop/store.py#"]
    );
    assert_eq!(
        keys_of(&reader, "shop/", CandidateKind::Definition, &["function"]),
        Vec::<String>::new()
    );
    assert_eq!(
        keys_of(&reader, "sh", CandidateKind::Definition, &["function"]),
        Vec::<String>::new(),
        "short terms too"
    );
    // Files match by path.
    let mut files = keys(&reader, "shop/", CandidateKind::File);
    files.sort();
    assert_eq!(files, ["shop/payments.py#", "shop/service.py#", "shop/store.py#"]);
    assert_eq!(keys(&reader, "sh", CandidateKind::File).len(), 3);
    // A shorter name that matches outranks a longer one.
    let found = keys(&reader, "order", CandidateKind::Definition);
    assert!(
        found.iter().position(|key| key == "src/orders.ts#Order").unwrap()
            < found
                .iter()
                .position(|key| key == "src/orders.ts#loadOrder")
                .unwrap(),
        "`Order` is a closer name match than `loadOrder`: {found:?}"
    );
}

#[test]
fn short_terms_odd_characters_and_item_kinds() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();
    // Shorter than a trigram: answered by a scan, shortest name first.
    assert_eq!(
        keys(&reader, "re", CandidateKind::Definition)[0],
        "shop/payments.py#retry"
    );
    assert_eq!(keys(&reader, "rs", CandidateKind::File), ["src/orders.ts#"]);
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
    // Narrowed to item kinds, long and short terms alike.
    assert_eq!(
        keys_of(&reader, "order", CandidateKind::Definition, &["type"]),
        ["src/orders.ts#Order"]
    );
    assert_eq!(
        keys_of(&reader, "or", CandidateKind::Definition, &["type", "class"]),
        ["src/orders.ts#Order"]
    );
    assert_eq!(
        keys_of(
            &reader,
            "payment",
            CandidateKind::Definition,
            &["class", "module"]
        ),
        ["shop/payments.py#", "shop/payments.py#PaymentDeclined"],
        "the module matches by its name, which is its path"
    );
    assert!(keys_of(&reader, "order", CandidateKind::Definition, &["class"]).is_empty());
}

#[test]
fn search_rows_follow_updates() {
    let (_dir, mut store, before) = committed();
    let mut graph = before.graph.clone();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "src/orders.ts");
    graph.apps[0].entrances.remove(2);
    graph.apps[0].name = "Market".into();
    graph.apps[0].run = Some("python -m market".into());
    let after = Prepared::new(graph);
    store.writer.commit(&after.since(&before)).unwrap();
    let reader = store.readers.get().unwrap();
    assert!(keys(&reader, "loadOrder", CandidateKind::Definition).is_empty());
    assert!(keys(&reader, "orders.ts", CandidateKind::File).is_empty());
    assert!(keys(&reader, "refund", CandidateKind::Entrance).is_empty());
    assert!(keys(&reader, "shop", CandidateKind::App).is_empty());
    assert_eq!(keys(&reader, "market", CandidateKind::App), ["entrance:shop"]);
    assert_eq!(keys(&reader, "market", CandidateKind::Entrance).len(), 3);
    assert_eq!(keys(&reader, "store", CandidateKind::File), ["shop/store.py#"]);
}
