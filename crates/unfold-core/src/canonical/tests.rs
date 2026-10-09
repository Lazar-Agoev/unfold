use super::*;
use crate::model::Graph;
use crate::testkit::sample::sample;
use crate::vocab::FileState;

fn sample_export() -> String {
    to_string(&sample().build())
}

#[test]
fn round_trips_to_identical_bytes_and_graph() {
    let graph = sample().build();
    let text = to_string(&graph);
    let parsed = read(&text).unwrap();
    assert_eq!(parsed, graph);
    assert_eq!(to_string(&parsed), text);
}

#[test]
fn writes_the_documented_records() {
    let text = sample_export();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], r#"{"t":"header","format":"unfold-graph","version":1}"#);
    assert!(
        lines[1].starts_with(
            r#"{"t":"file","path":"jobs/tasks.py","language":"python","state":"current","hash":""#
        )
    );
    let expected = [
        r#"{"t":"block","key":"jobs/tasks.py#","kind":"file","range":[0,32],"lines":[1,2]}"#,
        r#"{"t":"block","key":"jobs/tasks.py#function:run","kind":"function","role":"body","name":"run","range":[0,31],"lines":[1,2]}"#,
        r#"{"t":"slot","block":"jobs/tasks.py#function:run","slot":"parameter","index":0,"range":[8,15]}"#,
        r#"{"t":"edge","source":"jobs/tasks.py#function:run/expression_statement[0]/call[0]","seq":0,"kind":"calls","text":"handler","resolution":"unresolved","reason":"parameter","position":0}"#,
        r#"{"t":"edge","source":"src/orders.ts#import[0]","seq":0,"kind":"imports","target":"src/helper.ts#","text":"./helper","resolution":"resolved"}"#,
        r#"{"t":"edge","source":"src/orders.ts#function:createOrder/if[0]/throw[0]/new[0]","seq":0,"kind":"constructs","text":"Error","resolution":"external","module":"global"}"#,
        r#"{"t":"edge","source":"src/orders.ts#function:createOrder/return[0]/call[0]","seq":1,"kind":"references","target":"src/helper.ts#function:helper","text":"helper","resolution":"resolved","position":1}"#,
        r#"{"t":"entrance","key":"entrance:[\"express.route\",\"http\",\"POST\",\"/orders\",\"createOrder\"]","kind":"http","method":"POST","trigger":"/orders","trigger_status":"exact","detector":"express.route","evidence_path":"src/server.ts","evidence_range":[0,32],"evidence_block":"src/server.ts#expression_statement[0]/call[0]"}"#,
        r#"{"t":"target","entrance":"entrance:[\"express.route\",\"http\",\"POST\",\"/orders\",\"createOrder\"]","position":0,"role":"handler","target":"src/orders.ts#function:createOrder","text":"createOrder","resolution":"resolved"}"#,
        r#"{"t":"entrance","key":"entrance:[\"manual\",\"worker\",\"\",\"orders\",\"jobs/tasks.py#run\"]","kind":"worker","trigger":"orders","trigger_status":"exact","detector":"manual","evidence_path":".unfold/config.toml","evidence_range":[0,77],"evidence_pointer":"/entrance/0"}"#,
    ];
    for line in expected {
        assert!(lines.contains(&line), "missing {line}\nin:\n{text}");
    }
    let input = lines
        .iter()
        .position(|line| line.starts_with(r#"{"t":"input""#))
        .unwrap();
    let last_file_record = lines
        .iter()
        .rposition(|line| line.starts_with(r#"{"t":"edge""#))
        .unwrap();
    let first_entrance = lines
        .iter()
        .position(|line| line.starts_with(r#"{"t":"entrance""#))
        .unwrap();
    assert!(
        last_file_record < input && input < first_entrance,
        "sections are files, inputs, entrances"
    );
}

#[test]
fn output_does_not_depend_on_vector_order() {
    let expected = sample_export();
    let mut graph = sample().build();
    graph.files.reverse();
    graph.inputs.reverse();
    graph.entrances.reverse();
    for file in &mut graph.files {
        file.slots.reverse();
        file.edges.reverse();
    }
    assert_eq!(to_string(&graph), expected);
}

#[test]
fn optional_file_fields_round_trip() {
    let mut graph = sample().build();
    let file = &mut graph.files[0].file;
    file.state = FileState::Failed;
    file.hash = None;
    file.failure = Some("jobs/tasks.py:1:1: unexpected token".into());
    graph.files[0].blocks.clear();
    graph.files[0].slots.clear();
    graph.files[0].edges.clear();
    let text = to_string(&graph);
    assert!(text.contains(r#"{"t":"file","path":"jobs/tasks.py","language":"python","state":"failed","failure":"jobs/tasks.py:1:1: unexpected token"}"#));
    assert_eq!(read(&text).unwrap(), graph);
}

#[test]
fn an_empty_graph_is_just_a_header() {
    let text = to_string(&Graph::default());
    assert_eq!(
        text,
        "{\"t\":\"header\",\"format\":\"unfold-graph\",\"version\":1}\n"
    );
    assert_eq!(read(&text).unwrap(), Graph::default());
}

fn error(text: &str) -> CanonicalError {
    read(text).unwrap_err()
}

#[test]
fn rejects_malformed_exports_with_line_numbers() {
    let good = sample_export();
    let header = good.lines().next().unwrap();

    assert_eq!(error("").message, "missing header record");
    assert_eq!(
        error(&good[..good.len() - 1]).message,
        "the export must end with a newline"
    );
    assert_eq!(
        error(&good.replacen(
            header,
            "{\"t\":\"header\",\"format\":\"unfold-graph\",\"version\":2}",
            1
        ))
        .message,
        "unsupported export version 2"
    );

    let without_header: String = good.lines().skip(1).map(|line| format!("{line}\n")).collect();
    assert_eq!(
        error(&without_header).message,
        "the first record must be the header"
    );

    let unknown = good.replacen("\"kind\":\"function\"", "\"kind\":\"lambda\"", 1);
    let failure = error(&unknown);
    assert_eq!(failure.message, "unknown block kind `lambda`");
    assert_eq!(failure.line, 4);

    let extra = good.replacen("\"kind\":\"function\"", "\"kind\":\"function\",\"index\":3", 1);
    assert_eq!(
        error(&extra).message,
        "field `index` is not allowed on a `block` record"
    );

    let unknown_field = good.replacen("\"kind\":\"function\"", "\"kind\":\"function\",\"color\":3", 1);
    assert!(
        error(&unknown_field)
            .message
            .starts_with("invalid record: unknown field `color`")
    );

    let blank = good.replacen('\n', "\n\n", 1);
    assert!(error(&blank).message.starts_with("invalid record"));

    let mut lines: Vec<&str> = good.lines().collect();
    let input = lines
        .iter()
        .position(|line| line.starts_with(r#"{"t":"input""#))
        .unwrap();
    let moved = lines.remove(input);
    lines.insert(1, moved);
    let reordered: String = lines.iter().map(|line| format!("{line}\n")).collect();
    assert_eq!(
        error(&reordered).message,
        "a `file` record cannot follow later sections"
    );
}

#[test]
fn rejects_records_that_do_not_belong_to_the_current_file() {
    let good = sample_export();
    let foreign = good.replacen(
        r#""block":"jobs/tasks.py#function:run","slot":"name""#,
        r#""block":"src/orders.ts#function:save","slot":"name""#,
        1,
    );
    assert_eq!(
        error(&foreign).message,
        "`src/orders.ts#function:save` is not a block of the current file"
    );

    let orphan = good.replacen(
        r#""key":"jobs/tasks.py#function:run","#,
        r#""key":"jobs/tasks.py#function:run/if[0]","#,
        1,
    );
    assert!(
        error(&orphan)
            .message
            .contains("is not a block of the current file")
    );

    let target = good.replacen(
        r#""entrance":"entrance:[\"manual\""#,
        r#""entrance":"entrance:[\"other\""#,
        1,
    );
    assert!(error(&target).message.contains("does not follow that entrance"));
}
