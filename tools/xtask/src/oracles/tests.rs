use unfold_core::testkit::sample::sample;
use unfold_core::vocab::{EdgeKind, InputRole, Language, Resolution};
use unfold_core::{ContentHash, Graph, InputRecord, RepoPath};

use super::compare::{Ratio, compare};
use super::corpora::{OracleCorpora, listing_sha256};
use super::file::{CallRow, FORMAT, Header, OracleFile, ReferenceRow, corpus_sha256};
use super::floors::Floors;
use super::justified::Justified;

/// An oracle that agrees with every TypeScript edge of `graph`: resolved sites name Unfold's
/// target, the rest have no target.
fn agreeing_oracle(graph: &Graph) -> OracleFile {
    let mut calls = Vec::new();
    let mut references = Vec::new();
    for file in graph
        .files
        .iter()
        .filter(|file| file.file.language == Language::TypeScript)
    {
        for edge in &file.edges {
            let site: Box<str> = file.blocks[edge.source as usize].key.as_str().into();
            let target = edge.target.as_ref().map(|key| Box::<str>::from(key.as_str()));
            match edge.kind {
                EdgeKind::Calls | EdgeKind::Constructs if edge.seq == 0 => calls.push(CallRow {
                    site: Some(site),
                    outcome: target.is_none().then(|| "unknown".into()),
                    target,
                    ..CallRow::default()
                }),
                EdgeKind::References => references.push(ReferenceRow {
                    site,
                    position: edge.position.unwrap(),
                    target: target.unwrap(),
                    refused: None,
                }),
                _ => {}
            }
        }
    }
    OracleFile {
        header: Header {
            format: FORMAT.to_owned(),
            version: 1,
            corpus: "sample".to_owned(),
            language: Language::TypeScript,
            oracle: "typescript".to_owned(),
            oracle_version: "6.0.3".to_owned(),
            corpus_sha256: corpus_sha256(graph, Language::TypeScript).unwrap(),
            export_sha256: String::new(),
            files: 0,
            sites: calls.len(),
            resolved: calls.iter().filter(|row| row.target.is_some()).count(),
            references: references.len(),
            unfold_sites_without_checker_site: Some(0),
            source: None,
        },
        calls,
        references,
    }
}

fn resolved_calls(graph: &Graph) -> usize {
    graph
        .files
        .iter()
        .filter(|file| file.file.language == Language::TypeScript)
        .flat_map(|file| &file.edges)
        .filter(|edge| {
            matches!(edge.kind, EdgeKind::Calls | EdgeKind::Constructs)
                && edge.resolution == Resolution::Resolved
        })
        .count()
}

#[test]
fn an_agreeing_oracle_gives_full_precision_and_recall() {
    let graph = sample().build();
    let oracle = agreeing_oracle(&graph);
    let comparison = compare(
        "sample",
        Language::TypeScript,
        &graph,
        &oracle,
        &Justified::default(),
    );
    assert!(comparison.precise());
    assert_eq!(comparison.calls.part, resolved_calls(&graph));
    assert_eq!(
        comparison.calls,
        Ratio {
            part: comparison.calls.total,
            total: comparison.calls.total
        }
    );
    assert!(comparison.references.total > 0);
    assert_eq!(
        comparison.recall,
        Ratio {
            part: oracle.header.resolved,
            total: oracle.header.resolved
        }
    );
}

#[test]
fn a_different_or_missing_target_is_a_disagreement_until_justified() {
    let graph = sample().build();
    let mut oracle = agreeing_oracle(&graph);
    let row = oracle.calls.iter_mut().find(|row| row.target.is_some()).unwrap();
    let site = row.site.clone().unwrap();
    let unfold = row.target.take().unwrap();
    row.outcome = Some("not_concrete".into());

    let comparison = compare(
        "sample",
        Language::TypeScript,
        &graph,
        &oracle,
        &Justified::default(),
    );
    assert!(!comparison.precise());
    assert_eq!(comparison.unjustified.len(), 1);
    assert_eq!(comparison.unjustified[0].site, site);
    assert_eq!(&*comparison.unjustified[0].oracle, "no target (not_concrete)");
    // The oracle no longer resolves the site, so it leaves the recall denominator.
    assert_eq!(comparison.recall.total, oracle.header.resolved - 1);

    let text = format!(
        "[[justified]]\ncorpus = \"sample\"\nlanguage = \"typescript\"\nsite = \"{site}\"\ntarget = \"{unfold}\"\nreason = \"C-8\"\n"
    );
    let justified = Justified::parse(&text).unwrap();
    let comparison = compare("sample", Language::TypeScript, &graph, &oracle, &justified);
    assert!(comparison.precise());
    assert_eq!(comparison.justified.len(), 1);

    // The same entry is stale once the oracle agrees again.
    let agreeing = agreeing_oracle(&graph);
    let comparison = compare("sample", Language::TypeScript, &graph, &agreeing, &justified);
    assert!(!comparison.precise());
    assert_eq!(comparison.stale.len(), 1);
    // Entries for another corpus are not this corpus's business.
    let comparison = compare("other", Language::TypeScript, &graph, &agreeing, &justified);
    assert!(comparison.precise());
}

#[test]
fn reference_disagreements_and_recall_misses_are_counted() {
    let graph = sample().build();
    let mut oracle = agreeing_oracle(&graph);
    let removed = oracle.references.pop().unwrap();
    // A site Unfold leaves unresolved that the oracle resolves is a recall miss, not a
    // precision failure.
    let unresolved = oracle.calls.iter_mut().find(|row| row.target.is_none()).unwrap();
    unresolved.outcome = None;
    unresolved.target = Some(removed.target.clone());
    let comparison = compare(
        "sample",
        Language::TypeScript,
        &graph,
        &oracle,
        &Justified::default(),
    );
    assert_eq!(comparison.unjustified.len(), 1);
    assert_eq!(comparison.unjustified[0].position, Some(removed.position));
    assert_eq!(comparison.recall.total, oracle.header.resolved + 1);
    assert_eq!(comparison.recall.part, oracle.header.resolved);
}

#[test]
fn ratios_compare_exactly() {
    let floor = Ratio { part: 46, total: 71 };
    assert!(Ratio { part: 46, total: 71 }.at_least(floor));
    assert!(Ratio { part: 71, total: 71 }.above(floor));
    assert!(!Ratio { part: 45, total: 71 }.at_least(floor));
    assert!(Ratio { part: 0, total: 0 }.at_least(Ratio { part: 3, total: 3 }));
    // 70,720 of 73,649 is above 96.0%.
    assert!(
        Ratio {
            part: 70_720,
            total: 73_649
        }
        .at_least(Ratio {
            part: 960,
            total: 1000
        })
    );
    assert_eq!(Ratio { part: 1, total: 3 }.to_string(), "1/3 (33.33%)");
}

#[test]
fn oracle_files_round_trip_and_reject_malformed_rows() {
    let graph = sample().build();
    let oracle = agreeing_oracle(&graph);
    let text = oracle.render().unwrap();
    let parsed = OracleFile::parse(&text).unwrap();
    assert_eq!(parsed.calls, oracle.calls);
    assert_eq!(parsed.references, oracle.references);
    assert_eq!(parsed.render().unwrap(), text);

    let header = text.lines().next().unwrap();
    let bad = [
        // Both a target and an outcome.
        r#"{"t":"call","site":"a.ts#call[0]","target":"a.ts#function:f","outcome":"unknown"}"#,
        // Neither a site nor a file and range.
        r#"{"t":"call","target":"a.ts#function:f"}"#,
        // A refusal without a target.
        r#"{"t":"call","site":"a.ts#call[0]","outcome":"unknown","refused":"overridden"}"#,
        // An unknown field.
        r#"{"t":"call","site":"a.ts#call[0]","outcome":"unknown","why":"x"}"#,
        // A second header.
        header,
    ];
    for line in bad {
        assert!(
            OracleFile::parse(&format!("{header}\n{line}\n")).is_err(),
            "{line}"
        );
    }
    let twice = r#"{"t":"call","site":"a.ts#call[0]","outcome":"unknown"}"#;
    assert!(OracleFile::parse(&format!("{header}\n{twice}\n{twice}\n")).is_err());
    assert!(OracleFile::parse(twice).is_err(), "no header");
}

#[test]
fn the_corpus_hash_covers_sources_of_the_language_and_inputs() {
    let graph = sample().build();
    let typescript = corpus_sha256(&graph, Language::TypeScript).unwrap();
    assert_ne!(typescript, corpus_sha256(&graph, Language::Python).unwrap());
    let mut changed = graph;
    changed.inputs.push(InputRecord {
        path: RepoPath::new("tsconfig.json").unwrap(),
        role: InputRole::Tsconfig,
        hash: ContentHash::of(b"{}"),
    });
    assert_ne!(typescript, corpus_sha256(&changed, Language::TypeScript).unwrap());
}

#[test]
fn listings_hash_like_sha256sum() {
    // `printf 'x' | sha256sum` names the file; the listing is sorted by path bytes.
    let x = ContentHash::of(b"x");
    let y = ContentHash::of(b"y");
    let expected = ContentHash::of(format!("{x}  B\n{y}  a\n").as_bytes()).to_string();
    assert_eq!(listing_sha256([("a", y), ("B", x)]), expected);
}

#[test]
fn configuration_files_are_checked() {
    assert!(
        Floors::parse("[[floor]]\ncorpus = \"a\"\nlanguage = \"python\"\nmatched = 4\nresolved = 3\n")
            .is_err()
    );
    let twice = "[[floor]]\ncorpus = \"a\"\nlanguage = \"python\"\nmatched = 1\nresolved = 3\n";
    assert!(Floors::parse(&format!("{twice}{twice}")).is_err());
    assert!(Justified::parse("[[justified]]\ncorpus = \"a\"\nlanguage = \"python\"\nsite = \"s\"\ntarget = \"t\"\nreason = \" \"\n").is_err());
    assert!(
        OracleCorpora::parse("[[corpus]]\nname = \"a\"\nlanguages = [\"typescript\"]\n").is_err(),
        "no source"
    );
    assert!(
        OracleCorpora::parse("[[corpus]]\nname = \"a\"\nfolder = \"x\"\nlanguages = [\"python\"]\n").is_err(),
        "Python without rows"
    );
}

#[test]
fn the_committed_configuration_parses() {
    let corpora = OracleCorpora::load(&OracleCorpora::default_path()).unwrap();
    let floors = Floors::load(&Floors::default_path()).unwrap();
    Justified::load(&Justified::default_path()).unwrap();
    for corpus in &corpora.corpus {
        for &language in &corpus.languages {
            assert!(
                floors.get(&corpus.name, language).is_some(),
                "{} {language} has no floor",
                corpus.name
            );
            if corpus.committed {
                let path = corpus.oracle_path(language);
                let oracle = OracleFile::load(&path).unwrap();
                assert_eq!(oracle.header.corpus, corpus.name);
                assert_eq!(oracle.header.language, language);
            }
        }
    }
}
