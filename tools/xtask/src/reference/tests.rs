use std::fmt::Write as _;
use std::path::Path;

use unfold_core::{FileGraph, Graph, canonical};

use super::convert::{FileSource, convert};
use super::corpora::{Corpora, Corpus, PathMapping, SettingsSource};
use super::poc::Generation;
use super::*;

/// A TypeScript source with a two-byte and a four-byte (surrogate pair) character before most
/// ranges, two functions named `f`, and two unnamed `if`s.
const SOURCE: &str = "const é = \"𐐀\";\nfunction f() {}\nfunction f() {}\nif (é) {}\nif (é) {}\n";

const SETTINGS: &str = "version = 1\n\n[[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"src/a.ts#f\"\n\n\
                        [[entrance]]\nkind = \"command\"\ntrigger = \"go\"\nhandler = \"src/a.ts#f\"  # again\n";

/// The UTF-16 range of the `occurrence`-th match of `needle` in `SOURCE`, as `start,end`.
fn utf16(needle: &str, occurrence: usize) -> String {
    let start = SOURCE.match_indices(needle).nth(occurrence).unwrap().0;
    let units = |bytes: usize| SOURCE[..bytes].encode_utf16().count();
    format!("{},{}", units(start), units(start + needle.len()))
}

fn sha256(bytes: &[u8]) -> String {
    unfold_core::ContentHash::of(bytes).to_string()
}

/// Writes a folder and a raw generation describing it (PoC paths under `app/`, settings in
/// `app/codegraph.json`) and returns the corpus.
fn synthetic(root: &Path) -> Corpus {
    let folder = root.join("folder");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::create_dir_all(folder.join(".unfold")).unwrap();
    std::fs::write(folder.join("src/a.ts"), SOURCE).unwrap();
    std::fs::write(folder.join(".unfold/config.toml"), SETTINGS).unwrap();

    let raw = root.join("raw");
    std::fs::create_dir_all(&raw).unwrap();
    let manifest = format!(
        r#"{{"schema_version":3,"generated_at":"2026-10-08T00:00:00Z","offset_unit":"utf16","end_offset_exclusive":true,"line_base":1,"producers":[],"detectors":["manual"],"files":{{"app/src/a.ts":{{"sha256":"{}","language":"typescript"}}}},"inputs":{{"app/codegraph.json":{{"sha256":"{}"}}}}}}"#,
        sha256(SOURCE.as_bytes()),
        sha256(b"{}"),
    );
    std::fs::write(raw.join("manifest.json"), manifest).unwrap();

    let length = SOURCE.encode_utf16().count();
    let mut blocks =
        String::from("id,file_path,kind,name,parent_id,role,start_offset,end_offset,start_line,end_line\n");
    let rows = [
        format!("b1,app/src/a.ts,file,,,,0,{length},1,5"),
        format!("b2,app/src/a.ts,variable,é,b1,body,{},1,1", utf16("é = \"𐐀\"", 0)),
        format!(
            "b3,app/src/a.ts,function,f,b1,body,{},2,2",
            utf16("function f() {}", 0)
        ),
        format!(
            "b4,app/src/a.ts,function,f,b1,body,{},3,3",
            utf16("function f() {}", 1)
        ),
        format!("b5,app/src/a.ts,if,,b1,body,{},4,4", utf16("if (é) {}", 0)),
        format!("b6,app/src/a.ts,if,,b1,body,{},5,5", utf16("if (é) {}", 1)),
    ];
    for row in rows {
        let _ = writeln!(blocks, "{row}");
    }
    std::fs::write(raw.join("blocks.csv"), blocks).unwrap();

    let mut slots = String::from("block_id,slot,index,start_offset,end_offset\n");
    let _ = writeln!(slots, "b2,initializer,,{}", utf16("\"𐐀\"", 0));
    let _ = writeln!(slots, "b3,body,,{}", utf16("{}", 0));
    let _ = writeln!(slots, "b4,body,,{}", utf16("{}", 1));
    let _ = writeln!(slots, "b5,condition,,{}", utf16("é", 1));
    let _ = writeln!(slots, "b6,condition,,{}", utf16("é", 2));
    std::fs::write(raw.join("slots.csv"), slots).unwrap();
    std::fs::write(
        raw.join("edges.csv"),
        "source_id,target_id,kind,target_text,resolution,reason,module,position\n",
    )
    .unwrap();
    std::fs::write(
        raw.join("entrypoints.csv"),
        "id,kind,label,method,trigger,trigger_status,detector,evidence_block_id,evidence_file_path,evidence_start_offset,evidence_end_offset,evidence_pointer\n\
         e1,command,,,go,exact,manual,,app/codegraph.json,20,90,/entrypoints/0\n\
         e2,command,,,go,exact,manual,,app/codegraph.json,96,166,/entrypoints/1\n",
    )
    .unwrap();
    std::fs::write(
        raw.join("entrypoint_targets.csv"),
        "entrypoint_id,position,role,target_id,target_text,resolution,reason,module\n\
         e1,0,handler,b3,src/a.ts#f,resolved,,\n\
         e2,0,handler,b3,src/a.ts#f,resolved,,\n",
    )
    .unwrap();

    Corpus {
        name: "synthetic".to_owned(),
        raw: raw.display().to_string(),
        folder: folder.display().to_string(),
        poc_root: ".".to_owned(),
        languages: vec![unfold_core::vocab::Language::TypeScript],
        parity: true,
        paths: vec![PathMapping {
            poc: "app/".to_owned(),
            unfold: String::new(),
        }],
        settings: vec![SettingsSource {
            poc: "app/codegraph.json".to_owned(),
            first_entrance: 0,
        }],
    }
}

fn convert_synthetic(root: &Path) -> Graph {
    let corpus = synthetic(root);
    let generation = Generation::read(&root.join("raw")).unwrap();
    let source = FileSource {
        folder: root.join("folder"),
        poc_root: None,
    };
    convert(&corpus, &generation, &source).unwrap().graph
}

fn slice(text: &str, range: unfold_core::ByteRange) -> &str {
    &text[range.as_usize()]
}

#[test]
fn converts_utf16_offsets_paths_and_duplicate_sibling_keys() {
    let root = tempfile::tempdir().unwrap();
    let graph = convert_synthetic(root.path());
    let file: &FileGraph = graph.file("src/a.ts").unwrap();
    let keys: Vec<&str> = file.blocks.iter().map(|block| block.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "src/a.ts#",
            "src/a.ts#variable:é",
            "src/a.ts#function:f",
            "src/a.ts#function:f[1]",
            "src/a.ts#if[0]",
            "src/a.ts#if[1]",
        ]
    );
    assert_eq!(file.blocks[0].range.end as usize, SOURCE.len());
    assert_eq!(slice(SOURCE, file.blocks[1].range), "é = \"𐐀\"");
    // After the surrogate pair, byte and UTF-16 offsets differ by 1 + 2.
    assert_eq!(
        file.blocks[3].range.start as usize,
        SOURCE.match_indices("function").nth(1).unwrap().0
    );
    assert_eq!(slice(SOURCE, file.blocks[5].range), "if (é) {}");
    assert_eq!(slice(SOURCE, file.slots[0].range), "\"𐐀\"");
    assert_eq!(slice(SOURCE, file.slots[4].range), "é");
    assert_eq!(file.blocks[3].parent, Some(0));
}

#[test]
fn manual_entrances_move_to_the_settings_file_with_derived_keys() {
    let root = tempfile::tempdir().unwrap();
    let graph = convert_synthetic(root.path());
    let keys: Vec<&str> = graph
        .entrances
        .iter()
        .map(|entrance| entrance.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            r#"entrance:["manual","command","","go","src/a.ts#f"]"#,
            r#"entrance:["manual","command","","go","src/a.ts#f"]#1"#,
        ]
    );
    let first = &graph.entrances[0].evidence;
    assert_eq!(first.path.as_str(), ".unfold/config.toml");
    assert_eq!(first.pointer.as_deref(), Some("/entrance/0"));
    assert!(slice(SETTINGS, first.range).starts_with("[[entrance]]\nkind"));
    let second = &graph.entrances[1].evidence;
    assert_eq!(second.pointer.as_deref(), Some("/entrance/1"));
    assert!(slice(SETTINGS, second.range).ends_with("handler = \"src/a.ts#f\""));
    let target = &graph.entrances[0].targets[0];
    assert_eq!(&*target.text, "src/a.ts#f");
    assert_eq!(target.target.as_ref().unwrap().as_str(), "src/a.ts#function:f");
    assert_eq!(graph.inputs.len(), 1);
    assert_eq!(graph.inputs[0].path.as_str(), ".unfold/config.toml");
    // The export round-trips, at version 1 (references have no typed records).
    let mut text = Vec::new();
    canonical::write_version(&graph, 1, &mut text).unwrap();
    let text = String::from_utf8(text).unwrap();
    let (read_back, version) = canonical::read_versioned(&text).unwrap();
    let mut again = Vec::new();
    canonical::write_version(&read_back, version, &mut again).unwrap();
    assert_eq!(String::from_utf8(again).unwrap(), text);
}

#[test]
fn settings_that_disagree_with_the_generation_fail() {
    let root = tempfile::tempdir().unwrap();
    let corpus = synthetic(root.path());
    let settings = root.path().join("folder/.unfold/config.toml");
    std::fs::write(
        &settings,
        SETTINGS.replace("trigger = \"go\"\nhandler", "trigger = \"stop\"\nhandler"),
    )
    .unwrap();
    let generation = Generation::read(&root.path().join("raw")).unwrap();
    let source = FileSource {
        folder: root.path().join("folder"),
        poc_root: None,
    };
    let error = convert(&corpus, &generation, &source).unwrap_err();
    assert!(
        format!("{error:#}").contains("does not declare this entrance"),
        "{error:#}"
    );
}

#[test]
fn source_bytes_must_match_the_manifest_with_an_optional_poc_fallback() {
    let root = tempfile::tempdir().unwrap();
    let corpus = synthetic(root.path());
    let generation = Generation::read(&root.path().join("raw")).unwrap();
    let source_path = root.path().join("folder/src/a.ts");

    std::fs::write(&source_path, SOURCE.replace('f', "g")).unwrap();
    let folder_only = FileSource {
        folder: root.path().join("folder"),
        poc_root: None,
    };
    let error = convert(&corpus, &generation, &folder_only).unwrap_err();
    assert!(format!("{error:#}").contains("SHA-256"), "{error:#}");

    // Without the file in the folder, the PoC copy is used.
    std::fs::remove_file(&source_path).unwrap();
    assert!(convert(&corpus, &generation, &folder_only).is_err());
    let poc = root.path().join("poc");
    std::fs::create_dir_all(poc.join("app/src")).unwrap();
    std::fs::write(poc.join("app/src/a.ts"), SOURCE).unwrap();
    let with_poc = FileSource {
        folder: root.path().join("folder"),
        poc_root: Some(poc),
    };
    assert!(convert(&corpus, &generation, &with_poc).is_ok());
}

#[test]
fn python_unicode_offsets_become_bytes() {
    let corpora = Corpora::load(&Corpora::default_path()).unwrap();
    let corpus = corpora.get("python-playground").unwrap();
    let (text, counts) = import(corpus, None).unwrap();
    assert_eq!(counts.blocks, 888);
    let graph = canonical::read(&text).unwrap();
    let path = "src/orders_playground/features/unicode.py";
    let source = std::fs::read_to_string(corpus.folder_dir().join(path)).unwrap();
    let file = graph.file(path).unwrap();
    assert_eq!(file.blocks[0].range.end as usize, source.len());
    let block = |key: &str| {
        file.blocks
            .iter()
            .find(|block| block.key.as_str() == format!("{path}#{key}"))
            .unwrap_or_else(|| panic!("no block {key}"))
    };
    // κόστος (Greek, two bytes per letter) and 𐐀receipt (an astral letter, a surrogate pair).
    let greek = block("function:κόστος");
    assert!(slice(&source, greek.range).starts_with("def κόστος("));
    let astral = block("function:𐐀receipt");
    assert!(slice(&source, astral.range).starts_with("def 𐐀receipt("));
    // The PoC's UTF-16 offsets for these blocks are smaller than the byte offsets.
    assert!(greek.range.start > 66 && astral.range.start > 130);
}

#[test]
fn committed_references_are_up_to_date() {
    let corpora = Corpora::load(&Corpora::default_path()).unwrap();
    for corpus in &corpora.corpus {
        let (text, _) = import(corpus, None).unwrap();
        let committed = std::fs::read_to_string(corpus.canonical_path()).unwrap_or_default();
        assert!(
            committed == text,
            "{} is out of date; run `cargo xtask reference import`",
            corpus.name
        );
    }
}
