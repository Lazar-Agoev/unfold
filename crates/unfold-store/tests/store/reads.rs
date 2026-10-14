//! Reader queries: nodes, children, typed records, the reverse indexes with paging, apps, the
//! definition catalog, and statistics.

use std::collections::BTreeSet;

use unfold_core::testkit::sample::shop;
use unfold_core::vocab::{Form, Language, LinkKind, MemberKind, Reason};
use unfold_core::{Graph, Key, LinkTarget, Pairing};
use unfold_store::{Cursor, IncomingLink, Reader};

use crate::common::{Prepared, temp_store};

const PLACE_ORDER: &str = "shop/service.py#place_order";
const SAVE_ORDER: &str = "shop/store.py#save_order";
const ORDER: &str = "src/orders.ts#Order";
const DECLINED: &str = "shop/payments.py#PaymentDeclined";

/// A store holding the sample's first compile.
fn committed() -> (tempfile::TempDir, unfold_store::Store, Prepared) {
    let (dir, mut store) = temp_store();
    let prepared = Prepared::new(shop().build());
    store
        .writer
        .commit(&prepared.first_compile())
        .expect("first compile");
    (dir, store, prepared)
}

/// Every item of a paged query, `limit` at a time, checking that no item repeats.
fn all_pages(
    mut query: impl FnMut(Option<&Cursor>) -> unfold_store::Page<IncomingLink>,
) -> (Vec<IncomingLink>, usize) {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut after: Option<Cursor> = None;
    let mut pages = 0;
    loop {
        let page = query(after.as_ref());
        pages += 1;
        for item in page.items {
            assert!(
                seen.insert((item.from.clone(), item.link.seq)),
                "repeated {}",
                item.from
            );
            items.push(item);
        }
        match page.next {
            Some(next) => after = Some(next),
            None => return (items, pages),
        }
    }
}

/// The source keys of the callers of `key`.
fn caller_keys(reader: &Reader, key: &str) -> Vec<String> {
    let page = reader.callers(key, None, 100).expect("callers");
    assert_eq!(page.next, None);
    page.items.iter().map(|item| item.from.to_string()).collect()
}

#[test]
fn nodes_children_and_files() {
    let (_dir, store, prepared) = committed();
    let reader = store.readers.get().unwrap();
    let service = prepared.graph.file("shop/service.py").unwrap();

    let located = reader.node(PLACE_ORDER).unwrap().unwrap();
    assert_eq!(located.path.as_str(), "shop/service.py");
    assert_eq!(located.language, Language::Python);
    assert_eq!(located.node.form, Form::Function);
    assert_eq!(&service.nodes[located.id as usize], &located.node);
    assert!(reader.node("shop/service.py#absent").unwrap().is_none());

    let children: Vec<String> = reader
        .children(&format!("{PLACE_ORDER}/try[0]"))
        .unwrap()
        .into_iter()
        .map(|child| child.node.key.to_string())
        .collect();
    assert_eq!(
        children,
        [
            "shop/service.py#place_order/try[0]/body",
            "shop/service.py#place_order/try[0]/except[0]"
        ]
    );
    let expected: Vec<&Key> = service
        .nodes
        .iter()
        .filter(|node| node.parent == Some(located.id))
        .map(|node| &node.key)
        .collect();
    let children = reader.children(PLACE_ORDER).unwrap();
    assert_eq!(
        children.iter().map(|child| &child.node.key).collect::<Vec<_>>(),
        expected
    );
    assert!(children.iter().all(|child| child.node.parent == Some(located.id)));
    assert!(reader.children("nothing#").unwrap().is_empty());

    let files = reader.files().unwrap();
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "shop/payments.py",
            "shop/service.py",
            "shop/store.py",
            "src/orders.ts"
        ]
    );
    let row = reader.file("shop/service.py").unwrap().unwrap();
    assert_eq!(row.facts_hash, row.disk.hash);
    assert_eq!(row.disk.mtime_ns, Some(1_700_000_000_000_000_000));
    assert_eq!(row.record(), service.file);
    assert!(reader.file("shop/absent.py").unwrap().is_none());
}

#[test]
fn typed_records_by_node() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();

    let charge = reader.typed_records("shop/payments.py#charge_card").unwrap();
    let names: Vec<_> = charge
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_deref().unwrap())
        .collect();
    assert_eq!(names, ["card", "amount"]);
    assert!(charge.members.is_empty() && charge.arguments.is_empty());

    let call = reader
        .typed_records("shop/store.py#mark_failed/call:save_order")
        .unwrap();
    assert_eq!(call.arguments.len(), 1);
    assert_eq!(call.arguments[0].pairing, Some(Pairing::Parameter(0)));
    assert!(call.parameters.is_empty() && call.signatures.is_empty());

    let order = reader.typed_records(ORDER).unwrap();
    let members: Vec<_> = order
        .members
        .iter()
        .map(|member| (member.name.as_deref().unwrap(), member.kind))
        .collect();
    assert_eq!(
        members,
        [("id", MemberKind::Property), ("total", MemberKind::Property)]
    );

    assert_eq!(
        reader.typed_records("shop/absent.py#x").unwrap(),
        Default::default()
    );
}

#[test]
fn callers_are_calls_and_passes_links_in_path_order() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();
    assert_eq!(
        caller_keys(&reader, SAVE_ORDER),
        [
            "shop/service.py#place_order/call:save_order",
            "shop/store.py#mark_failed/call:save_order",
            "shop/store.py#save_all/call:list/call:map",
        ]
    );
    let page = reader.callers(SAVE_ORDER, None, 10).unwrap();
    let kinds: Vec<LinkKind> = page.items.iter().map(|item| item.link.kind).collect();
    assert_eq!(kinds, [LinkKind::Calls, LinkKind::Calls, LinkKind::Passes]);
    let passes = &page.items[2];
    assert_eq!(passes.path.as_str(), "shop/store.py");
    assert_eq!(passes.language, Language::Python);
    assert_eq!(passes.link.seq, 1);
    assert_eq!(&*passes.link.text, "save_order");
    assert_eq!(passes.link.to, LinkTarget::Project(Key::from_text(SAVE_ORDER)));
    assert_eq!(reader.caller_count(SAVE_ORDER).unwrap(), 3);

    // A construction calls its class; naming the class in a clause is a type use, not a call.
    assert_eq!(
        caller_keys(&reader, DECLINED),
        ["shop/payments.py#charge_card/if[0]/then/raise[0]/call:PaymentDeclined"]
    );
    assert!(caller_keys(&reader, "shop/absent.py#x").is_empty());
    assert_eq!(reader.caller_count("shop/absent.py#x").unwrap(), 0);
    // Imports reach modules, but are neither callers nor type uses.
    assert_eq!(reader.caller_count("shop/store.py#").unwrap(), 0);
    assert_eq!(reader.type_use_count("shop/store.py#").unwrap(), 0);
}

#[test]
fn reverse_indexes_page_without_gaps_or_repeats() {
    let (_dir, store, _) = committed();
    let reader = store.readers.get().unwrap();
    let (items, pages) = all_pages(|after| reader.callers(SAVE_ORDER, after, 1).unwrap());
    assert_eq!(pages, 3);
    let paged: Vec<String> = items.iter().map(|item| item.from.to_string()).collect();
    assert_eq!(paged, caller_keys(&reader, SAVE_ORDER));
    let (items, pages) = all_pages(|after| reader.type_uses(ORDER, after, 1).unwrap());
    assert_eq!((items.len(), pages), (2, 2));
    let (items, pages) = all_pages(|after| reader.callers(SAVE_ORDER, after, 2).unwrap());
    assert_eq!((items.len(), pages), (3, 2));
}

#[test]
fn type_uses_are_types_and_extends_links() {
    let mut graph = shop().build();
    // Make PaymentDeclined's base a project interface it implements, so an `extends` link
    // reaches `Order`.
    let payments = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "shop/payments.py")
        .unwrap();
    let base = payments
        .links
        .iter_mut()
        .find(|link| link.kind == LinkKind::Extends)
        .unwrap();
    base.to = LinkTarget::Project(Key::from_text(ORDER));
    base.implements = true;
    let (_dir, mut store) = temp_store();
    store
        .writer
        .commit(&Prepared::new(graph).first_compile())
        .unwrap();
    let reader = store.readers.get().unwrap();

    let uses = reader.type_uses(ORDER, None, 10).unwrap();
    let places: Vec<(&str, LinkKind, bool, u32)> = uses
        .items
        .iter()
        .map(|item| {
            (
                item.from.as_str(),
                item.link.kind,
                item.link.implements,
                item.link.range.start,
            )
        })
        .collect();
    assert_eq!(
        places,
        [
            (DECLINED, LinkKind::Extends, true, 99),
            (
                "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]",
                LinkKind::Types,
                false,
                240
            ),
            ("src/orders.ts#loadOrder", LinkKind::Types, false, 362),
        ]
    );
    assert_eq!(reader.type_use_count(ORDER).unwrap(), 3);
    assert_eq!(
        reader
            .type_uses(DECLINED, None, 10)
            .unwrap()
            .items
            .iter()
            .map(|item| item.from.as_str())
            .collect::<Vec<_>>(),
        ["shop/service.py#place_order/try[0]/except[0]"]
    );
    assert_eq!(reader.type_use_count("shop/absent.py#x").unwrap(), 0);
}

#[test]
fn links_into_a_replaced_file_stay_and_the_reverse_indexes_follow_link_changes() {
    let (_dir, mut store, before) = committed();
    let reader = store.readers.get().unwrap();

    // Replace store.py's nodes (a new definition renumbers nothing before it, but every row is
    // rewritten): the links from service.py into it stay.
    let mut graph = before.graph.clone();
    let store_py = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "shop/store.py")
        .unwrap();
    let mut node = store_py
        .nodes
        .iter()
        .find(|node| node.form == Form::Function)
        .unwrap()
        .clone();
    node.parent = Some(0);
    node.name = Some("forget_order".into());
    node.key = Key::from_text("shop/store.py#forget_order");
    store_py.nodes.push(node);
    let replaced = Prepared::new(graph);
    let update = replaced.since(&before);
    assert_eq!(update.files.len(), 1);
    assert!(update.files[0].nodes.is_some());
    store.writer.commit(&update).unwrap();
    assert_eq!(reader.caller_count(SAVE_ORDER).unwrap(), 3);
    assert_eq!(
        caller_keys(&reader, "shop/store.py#load_order"),
        ["shop/service.py#place_order/call:load_order"]
    );

    // service.py's call to save_order stops resolving: one caller fewer.
    let mut graph = replaced.graph.clone();
    let service = graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == "shop/service.py")
        .unwrap();
    let call = service
        .links
        .iter_mut()
        .find(|link| link.to == LinkTarget::Project(Key::from_text(SAVE_ORDER)))
        .unwrap();
    call.to = LinkTarget::Unknown(Reason::Ambiguous);
    let unlinked = Prepared::new(graph);
    store.writer.commit(&unlinked.since(&replaced)).unwrap();
    assert_eq!(
        caller_keys(&reader, SAVE_ORDER),
        [
            "shop/store.py#mark_failed/call:save_order",
            "shop/store.py#save_all/call:list/call:map",
        ]
    );

    // Removing service.py removes every link it held from the indexes.
    let mut graph = unlinked.graph.clone();
    graph
        .files
        .retain(|file| file.file.path.as_str() != "shop/service.py");
    let removed = Prepared::new(graph);
    store.writer.commit(&removed.since(&unlinked)).unwrap();
    assert_eq!(reader.caller_count("shop/store.py#load_order").unwrap(), 0);
    assert_eq!(reader.type_use_count(DECLINED).unwrap(), 0);
    assert_eq!(reader.caller_count(DECLINED).unwrap(), 1);
}

#[test]
fn apps_keep_their_order_targets_and_states() {
    let (_dir, store, prepared) = committed();
    let reader = store.readers.get().unwrap();
    let apps = reader.apps().unwrap();
    assert_eq!(apps, prepared.graph.apps);
    let shop = &apps[0];
    assert_eq!(shop.key.as_str(), "entrance:shop");
    assert_eq!(shop.code.as_ref().unwrap().code.as_str(), "shop/service.py#");
    let names: Vec<&str> = shop.entrances.iter().map(|entrance| &*entrance.name).collect();
    assert_eq!(
        names,
        ["Place an order", "Orders API", "Refund an order", "Charge a card"]
    );
    assert_eq!(shop.entrances[3].code.len(), 2);

    // A second app, written before the first, keeps file order; an app without `code` has none.
    let mut graph = prepared.graph;
    let mut first = graph.apps[0].clone();
    first.key = Key::from_text("entrance:admin");
    first.name = "Admin".into();
    first.code = None;
    first.entrances.clear();
    graph.apps.insert(0, first);
    let (_dir2, mut other) = temp_store();
    other
        .writer
        .commit(&Prepared::new(graph.clone()).first_compile())
        .unwrap();
    assert_eq!(other.readers.get().unwrap().apps().unwrap(), graph.apps);
    assert_eq!(other.readers.get().unwrap().graph().unwrap(), graph);
}

#[test]
fn the_definition_catalog() {
    let (_dir, store, prepared) = committed();
    let reader = store.readers.get().unwrap();

    let definitions = reader.definitions_in("shop/payments.py").unwrap();
    let names: Vec<(&str, &str)> = definitions
        .iter()
        .map(|definition| (&*definition.label, &*definition.qualified_name))
        .collect();
    assert_eq!(
        names,
        [
            ("payments.py", "payments.py"),
            ("PaymentDeclined", "PaymentDeclined"),
            ("retry", "retry"),
            ("lambda", "retry.lambda"),
            ("charge_card", "charge_card"),
        ]
    );
    let save = reader.definition(SAVE_ORDER).unwrap().unwrap();
    assert_eq!(save.path.as_str(), "shop/store.py");
    assert_eq!(save.kind, unfold_core::vocab::NodeKind::Function);
    assert!(reader.definition("shop/absent.py#x").unwrap().is_none());

    let mut all = Vec::new();
    let mut after: Option<Cursor> = None;
    loop {
        let page = reader.definitions(after.as_ref(), 2).unwrap();
        all.extend(page.items.into_iter().map(|definition| definition.key));
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let expected: Vec<Key> = prepared
        .definitions
        .iter()
        .flatten()
        .map(|definition| definition.key.clone())
        .collect();
    assert_eq!(all, expected, "every definition, in path and then source order");

    // Indexed files count their definitions; other paths are left out.
    let counts = reader
        .definition_counts(&["shop/store.py", "README.md", "src/orders.ts"])
        .unwrap();
    let counts: Vec<(&str, u32)> = counts
        .iter()
        .map(|(path, count)| (path.as_str(), *count))
        .collect();
    let expected = |path: &str| {
        let index = prepared
            .graph
            .files
            .iter()
            .position(|file| file.file.path.as_str() == path)
            .unwrap();
        u32::try_from(prepared.definitions[index].len()).unwrap()
    };
    assert_eq!(
        counts,
        [
            ("shop/store.py", expected("shop/store.py")),
            ("src/orders.ts", expected("src/orders.ts"))
        ]
    );
    assert!(reader.definition_counts(&[]).unwrap().is_empty());
}

#[test]
fn stats_count_rows() {
    let (_dir, store, prepared) = committed();
    let stats = store.readers.get().unwrap().stats().unwrap();
    let graph: &Graph = &prepared.graph;
    let count = |f: fn(&unfold_core::FileGraph) -> usize| graph.files.iter().map(f).sum::<usize>() as u64;
    assert_eq!(stats.files_current, 4);
    assert_eq!(stats.files_failed + stats.files_last_good, 0);
    assert_eq!(stats.nodes, count(|file| file.nodes.len()));
    assert_eq!(stats.links_project, 15);
    assert_eq!(
        stats.links_project + stats.links_library + stats.links_builtin + stats.links_unknown,
        count(|file| file.links.len())
    );
    assert!(stats.links_library > 0 && stats.links_builtin > 0 && stats.links_unknown > 0);
    assert_eq!(stats.apps, 1);
    assert_eq!(stats.entrances, 4);
    assert_eq!(stats.inputs, 1);
    assert_eq!(
        stats.definitions,
        prepared.definitions.iter().flatten().count() as u64
    );
    assert!(stats.database_bytes > 0);
}

#[test]
fn readers_work_from_other_threads() {
    let (_dir, store, _) = committed();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let pool = store.readers.clone();
            std::thread::spawn(move || pool.get().unwrap().files().unwrap().len())
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap(), 4);
    }
    let reader = store.readers.get().unwrap();
    let moved = std::thread::spawn(move || reader.apps().unwrap().len());
    assert_eq!(moved.join().unwrap(), 1);
}
