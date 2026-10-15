use std::sync::Arc;

use unfold_core::testkit::sample;
use unfold_core::vocab::{InputRole, Language, LinkKind, Reason};
use unfold_core::{ContentHash, Graph, Input, LinkTarget, RepoPath};

use super::compare::{Ratio, compare};
use super::corpora::{OracleCorpora, listing_sha256};
use super::file::{CallRow, EXTERNAL, FORMAT, Header, OracleFile, ReferenceRow, VERSION, corpus_sha256};
use super::floors::Floors;
use super::justified::Justified;

/// The sample graph. Its Python files pass a project function as an argument, so the tests use
/// Python's links.
fn graph() -> Graph {
    sample::shop().build()
}

/// The project target of a link, if it has one.
fn project(to: &LinkTarget) -> Option<Box<str>> {
    match to {
        LinkTarget::Project(key) => Some(key.as_str().into()),
        _ => None,
    }
}

/// An oracle that agrees with every Python link of `graph`: resolved sites name Unfold's target,
/// `library` and `builtin` sites are `external` in Unfold's module, and the rest have no target.
fn agreeing_oracle(graph: &Graph) -> OracleFile {
    let mut calls = Vec::new();
    let mut references = Vec::new();
    for file in graph
        .files
        .iter()
        .filter(|file| file.file.language == Language::Python)
    {
        for link in &file.links {
            let site: Box<str> = file.nodes[link.from as usize].key.as_str().into();
            let target = project(&link.to);
            match link.kind {
                LinkKind::Calls => calls.push(match &link.to {
                    LinkTarget::Project(_) => CallRow {
                        site: Some(site),
                        target,
                        ..CallRow::default()
                    },
                    LinkTarget::Library(label) | LinkTarget::Builtin(label) => CallRow {
                        site: Some(site),
                        outcome: Some(EXTERNAL.into()),
                        module: Some(label.as_str().into()),
                        ..CallRow::default()
                    },
                    LinkTarget::Unknown(_) => CallRow {
                        site: Some(site),
                        outcome: Some("unknown".into()),
                        ..CallRow::default()
                    },
                }),
                LinkKind::Passes => {
                    let argument = file
                        .types
                        .arguments
                        .iter()
                        .find(|argument| argument.call == link.from && argument.range.contains(link.range))
                        .unwrap();
                    references.push(ReferenceRow {
                        site,
                        position: argument.index,
                        target: target.unwrap(),
                        refused: None,
                    });
                }
                _ => {}
            }
        }
    }
    OracleFile {
        header: Header {
            format: FORMAT.to_owned(),
            version: VERSION,
            corpus: "sample".to_owned(),
            language: Language::Python,
            oracle: "pyright".to_owned(),
            oracle_version: "1.1.407".to_owned(),
            corpus_sha256: corpus_sha256(graph, Language::Python).unwrap(),
            export_sha256: String::new(),
            files: 0,
            sites: calls.len(),
            resolved: calls.iter().filter(|row| row.target.is_some()).count(),
            references: references.len(),
            unfold_sites_without_checker_site: None,
            source: None,
        },
        calls,
        references,
    }
}

/// The Python `calls` links of `graph` whose target `matches`.
fn python_calls(graph: &Graph, matches: impl Fn(&LinkTarget) -> bool) -> usize {
    graph
        .files
        .iter()
        .filter(|file| file.file.language == Language::Python)
        .flat_map(|file| &file.links)
        .filter(|link| link.kind == LinkKind::Calls && matches(&link.to))
        .count()
}

fn resolved_calls(graph: &Graph) -> usize {
    python_calls(graph, |to| matches!(to, LinkTarget::Project(_)))
}

fn external_calls(graph: &Graph) -> usize {
    python_calls(graph, |to| {
        matches!(to, LinkTarget::Library(_) | LinkTarget::Builtin(_))
    })
}

/// The oracle row at the first site whose Unfold call is `library` with `label`.
fn library_row<'a>(graph: &Graph, oracle: &'a mut OracleFile, label: &str) -> &'a mut CallRow {
    let site = graph
        .files
        .iter()
        .flat_map(|file| file.links.iter().map(move |link| (file, link)))
        .find(|(_, link)| {
            link.kind == LinkKind::Calls
                && matches!(&link.to, LinkTarget::Library(found) if found.as_str() == label)
        })
        .map(|(file, link)| file.nodes[link.from as usize].key.as_str().to_owned())
        .unwrap();
    oracle
        .calls
        .iter_mut()
        .find(|row| row.site.as_deref() == Some(site.as_str()))
        .unwrap()
}

#[test]
fn an_agreeing_oracle_gives_full_precision_and_recall() {
    let graph = graph();
    let oracle = agreeing_oracle(&graph);
    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
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
    assert_eq!(comparison.external.total, external_calls(&graph));
    assert_eq!(comparison.external.part, comparison.external.total);
    assert_eq!(comparison.precision().part, comparison.precision().total);
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
    let graph = graph();
    let mut oracle = agreeing_oracle(&graph);
    let row = oracle.calls.iter_mut().find(|row| row.target.is_some()).unwrap();
    let site = row.site.clone().unwrap();
    let unfold = row.target.take().unwrap();
    row.outcome = Some("not_concrete".into());

    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
    assert!(!comparison.precise());
    assert_eq!(comparison.unjustified.len(), 1);
    assert_eq!(comparison.unjustified[0].site, site);
    assert_eq!(&*comparison.unjustified[0].oracle, "no target (not_concrete)");
    // The oracle no longer resolves the site, so it leaves the recall denominator.
    assert_eq!(comparison.recall.total, oracle.header.resolved - 1);

    let text = format!(
        "[[justified]]\ncorpus = \"sample\"\nlanguage = \"python\"\nsite = \"{site}\"\ntarget = \"{unfold}\"\nreason = \"C-8\"\n"
    );
    let justified = Justified::parse(&text).unwrap();
    let comparison = compare("sample", Language::Python, &graph, &oracle, &justified);
    assert!(comparison.precise());
    assert_eq!(comparison.justified.len(), 1);

    // The same entry is stale once the oracle agrees again.
    let agreeing = agreeing_oracle(&graph);
    let comparison = compare("sample", Language::Python, &graph, &agreeing, &justified);
    assert!(!comparison.precise());
    assert_eq!(comparison.stale.len(), 1);
    // Entries for another corpus are not this corpus's business.
    let comparison = compare("other", Language::Python, &graph, &agreeing, &justified);
    assert!(comparison.precise());
}

#[test]
fn a_library_call_the_oracle_resolves_in_the_project_is_a_disagreement() {
    let graph = graph();
    let mut oracle = agreeing_oracle(&graph);
    let project = oracle.calls.iter().find_map(|row| row.target.clone()).unwrap();
    let row = library_row(&graph, &mut oracle, "python:logging");
    let site = row.site.clone().unwrap();
    row.outcome = None;
    row.module = None;
    row.target = Some(project.clone());

    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
    assert!(!comparison.precise());
    assert_eq!(comparison.unjustified.len(), 1);
    let disagreement = &comparison.unjustified[0];
    assert_eq!(disagreement.site, site);
    assert_eq!(&*disagreement.unfold, "library python:logging");
    assert_eq!(&*disagreement.oracle, format!("target {project}"));
    assert_eq!(
        disagreement.to_string(),
        format!("{site}: Unfold resolves library python:logging, the oracle says target {project}")
    );
    // The comparison counts against precision.
    assert_eq!(comparison.external.total, external_calls(&graph));
    assert_eq!(comparison.external.part, comparison.external.total - 1);
    assert_eq!(comparison.precision().part + 1, comparison.precision().total);

    // A justification names the claim as Unfold's target.
    let text = format!(
        "[[justified]]\ncorpus = \"sample\"\nlanguage = \"python\"\nsite = \"{site}\"\ntarget = \"library python:logging\"\nreason = \"r\"\n"
    );
    let justified = Justified::parse(&text).unwrap();
    assert!(compare("sample", Language::Python, &graph, &oracle, &justified).precise());
}

#[test]
fn a_library_or_builtin_label_must_name_the_oracles_module() {
    let graph = graph();
    let mut oracle = agreeing_oracle(&graph);
    let row = library_row(&graph, &mut oracle, "python:logging");
    let site = row.site.clone().unwrap();
    row.module = Some("python:typing".into());

    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
    assert!(!comparison.precise());
    assert_eq!(comparison.unjustified.len(), 1);
    assert_eq!(
        comparison.unjustified[0].to_string(),
        format!(
            "{site}: Unfold resolves library python:logging, the oracle says no target (external python:typing)"
        )
    );
    assert_eq!(comparison.external.part, comparison.external.total - 1);

    // An oracle that has no answer for the site neither confirms nor contradicts the label.
    let row = library_row(&graph, &mut oracle, "python:logging");
    row.outcome = Some("ambiguous_unknown_or_wrapped".into());
    row.module = None;
    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
    assert!(comparison.precise());
    assert_eq!(comparison.external.total, external_calls(&graph) - 1);
    assert_eq!(comparison.external.part, comparison.external.total);
}

#[test]
fn a_project_call_the_oracle_finds_outside_names_the_module() {
    let graph = graph();
    let mut oracle = agreeing_oracle(&graph);
    let row = oracle.calls.iter_mut().find(|row| row.target.is_some()).unwrap();
    row.target = None;
    row.outcome = Some(EXTERNAL.into());
    row.module = Some("python:builtins".into());
    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
    assert_eq!(comparison.unjustified.len(), 1);
    assert_eq!(
        &*comparison.unjustified[0].oracle,
        "no target (external python:builtins)"
    );
}

#[test]
fn reference_disagreements_and_recall_misses_are_counted() {
    let mut graph = graph();
    // Unfold leaves one Python call unresolved.
    let link = graph
        .files
        .iter_mut()
        .filter(|file| file.file.language == Language::Python)
        .flat_map(|file| &mut Arc::make_mut(file).links)
        .find(|link| link.kind == LinkKind::Calls && matches!(link.to, LinkTarget::Builtin(_)))
        .unwrap();
    link.to = LinkTarget::Unknown(Reason::UnknownReceiver);
    let mut oracle = agreeing_oracle(&graph);
    let removed = oracle.references.pop().unwrap();
    // A site Unfold leaves unresolved that the oracle resolves is a recall miss, not a
    // precision failure.
    let unresolved = oracle
        .calls
        .iter_mut()
        .find(|row| row.outcome.as_deref() == Some("unknown"))
        .unwrap();
    unresolved.outcome = None;
    unresolved.target = Some(removed.target.clone());
    let comparison = compare("sample", Language::Python, &graph, &oracle, &Justified::default());
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
    let graph = graph();
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
        // A module on an outcome other than `external`, or `external` without one.
        r#"{"t":"call","site":"a.ts#call[0]","outcome":"unknown","module":"ecmascript"}"#,
        r#"{"t":"call","site":"a.ts#call[0]","outcome":"external"}"#,
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
    let graph = graph();
    let typescript = corpus_sha256(&graph, Language::TypeScript).unwrap();
    assert_ne!(typescript, corpus_sha256(&graph, Language::Python).unwrap());
    let mut changed = graph;
    changed.inputs.push(Input {
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
    assert!(
        Floors::parse(&format!("{twice}poc = \"1 of 3\"\n")).is_err(),
        "a floor holds only its counts"
    );
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
