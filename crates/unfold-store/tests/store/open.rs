//! Opening: creation, reuse, and rebuilding on version changes and damage.

use std::path::Path;

use rusqlite::Connection;
use unfold_core::testkit::sample::sample;
use unfold_store::{Opened, RecreateReason, Store, StoreConfig};

use crate::common::{ENGINE, Prepared};

fn open(path: &Path, engine: &str) -> Store {
    Store::open(path, &StoreConfig::new(engine)).expect("open")
}

fn file_count(store: &Store) -> usize {
    store.readers.get().expect("reader").files().expect("files").len()
}

#[test]
fn creates_then_reuses_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    let mut store = open(&path, ENGINE);
    assert_eq!(store.opened, Opened::Created);
    let prepared = Prepared::new(sample().build());
    store.writer.commit(&prepared.first_compile()).unwrap();
    drop(store);

    let store = open(&path, ENGINE);
    assert_eq!(store.opened, Opened::Reused);
    assert_eq!(file_count(&store), 4);
    let journal: String = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
}

#[test]
fn fts5_and_trigrams_are_compiled_in() {
    let conn = Connection::open_in_memory().unwrap();
    let fts5: bool = conn
        .query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(fts5);
    conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x, tokenize = 'trigram')")
        .unwrap();
}

#[test]
fn an_engine_change_rebuilds_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    let mut store = open(&path, ENGINE);
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    drop(store);

    let store = open(&path, "engine-2");
    assert_eq!(
        store.opened,
        Opened::Recreated(RecreateReason::EngineChanged {
            found: Some(ENGINE.into())
        })
    );
    assert_eq!(file_count(&store), 0);
    assert_eq!(
        store.readers.get().unwrap().meta().unwrap().engine_version,
        "engine-2"
    );
}

#[test]
fn a_schema_change_rebuilds_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    drop(open(&path, ENGINE));
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE meta SET value = '0' WHERE key = 'schema_version'", [])
        .unwrap();
    let store = open(&path, ENGINE);
    assert_eq!(
        store.opened,
        Opened::Recreated(RecreateReason::SchemaChanged {
            found: Some("0".into())
        })
    );
}

#[test]
fn a_file_that_is_not_a_database_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    std::fs::write(&path, vec![0x5a; 8192]).unwrap();
    let mut store = open(&path, ENGINE);
    assert_eq!(store.opened, Opened::Recreated(RecreateReason::Corrupt));
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    assert_eq!(file_count(&store), 4);
}

#[test]
fn a_database_with_a_damaged_header_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    let mut store = open(&path, ENGINE);
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    drop(store);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[..16].copy_from_slice(b"definitely not!!");
    std::fs::write(&path, bytes).unwrap();
    let store = open(&path, ENGINE);
    assert_eq!(store.opened, Opened::Recreated(RecreateReason::Corrupt));
    assert_eq!(file_count(&store), 0);
}

#[test]
fn a_foreign_sqlite_database_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('hi');")
        .unwrap();
    let store = open(&path, ENGINE);
    assert_eq!(store.opened, Opened::Recreated(RecreateReason::Corrupt));
}

#[test]
fn an_empty_file_becomes_a_new_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    std::fs::write(&path, b"").unwrap();
    assert_eq!(open(&path, ENGINE).opened, Opened::Created);
}

#[test]
fn recreate_discards_everything() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.sqlite");
    let mut store = open(&path, ENGINE);
    store
        .writer
        .commit(&Prepared::new(sample().build()).first_compile())
        .unwrap();
    drop(store);
    let store = Store::recreate(&path, &StoreConfig::new(ENGINE)).unwrap();
    assert_eq!(store.opened, Opened::Recreated(RecreateReason::Requested));
    assert_eq!(file_count(&store), 0);
}

#[test]
fn a_missing_directory_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent").join("graph.sqlite");
    let error = Store::open(&path, &StoreConfig::new(ENGINE)).unwrap_err();
    assert!(!error.is_corrupt());
}

#[test]
fn a_foreign_database_with_a_meta_table_is_replaced() {
    for ddl in [
        "CREATE TABLE meta (k TEXT, v TEXT);",
        "CREATE TABLE meta (key TEXT, value BLOB); INSERT INTO meta VALUES ('schema_version', x'00ff');",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.sqlite");
        Connection::open(&path).unwrap().execute_batch(ddl).unwrap();
        let store = open(&path, ENGINE);
        assert_eq!(store.opened, Opened::Recreated(RecreateReason::Corrupt), "{ddl}");
    }
}
