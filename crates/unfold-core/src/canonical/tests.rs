use super::*;
use crate::model::Graph;
use crate::testkit::{assert_golden, sample};
use crate::typed::FileTypes;
use crate::vocab::{Expansion, FileState};

fn sample_export() -> String {
    to_string(&sample::shop().build())
}

#[test]
fn round_trips_to_identical_bytes_and_graph() {
    let graph = sample::shop().build();
    let text = to_string(&graph);
    let parsed = read(&text).unwrap();
    assert_eq!(parsed, graph);
    assert_eq!(to_string(&parsed), text);
}

#[test]
fn the_sample_export_matches_its_golden() {
    assert_golden("goldens/sample.jsonl", &sample_export());
}

#[test]
fn an_empty_graph_is_just_a_header() {
    let text = to_string(&Graph::default());
    assert_eq!(
        text,
        "{\"t\":\"header\",\"format\":\"unfold-graph\",\"version\":2}\n"
    );
    assert_eq!(read(&text).unwrap(), Graph::default());
}

#[test]
fn output_does_not_depend_on_file_input_link_or_typed_record_order() {
    let expected = sample_export();
    assert_eq!(sample_export(), expected, "two writes are equal");
    let mut graph = sample::shop().build();
    graph.files.reverse();
    graph.inputs.reverse();
    for file in &mut graph.files {
        file.links.reverse();
        file.types.signatures.reverse();
        file.types.parameters.reverse();
        file.types.members.reverse();
        file.types.arguments.reverse();
    }
    assert_eq!(to_string(&graph), expected);
}

#[test]
fn optional_fields_and_flags_round_trip() {
    let mut graph = sample::shop().build();
    let file = &mut graph.files[0];
    file.file.state = FileState::Failed;
    file.file.hash = None;
    file.file.failure = Some("shop/payments.py:1:1: unexpected token".into());
    file.nodes.clear();
    file.links.clear();
    file.types = FileTypes::default();
    let orders = graph.files.last_mut().unwrap();
    orders.links[0].implements = true;
    let member = &mut orders.types.members[0];
    member.optional = true;
    member.readonly = true;
    member.default = true;
    member.value = Some("0".into());
    let parameter = &mut orders.types.parameters[0];
    parameter.optional = true;
    parameter.default = true;
    orders.types.arguments[0].expand = Some(Expansion::Spread);
    graph.apps[0].about = Some("The shop.".into());
    graph.apps[0].entrances[0].about = Some("Takes an order.".into());

    let text = to_string(&graph);
    assert!(text.contains(
        r#"{"t":"file","path":"shop/payments.py","language":"python","state":"failed","failure":"shop/payments.py:1:1: unexpected token"}"#
    ));
    assert!(text.contains(r#""outcome":"library","to":"express","implements":true}"#));
    assert!(text.contains(
        r#""kind":"property","name":"id","optional":true,"readonly":true,"default":true,"type":"string","type_range":[103,109],"value":"0"}"#
    ));
    assert!(text.contains(r#""about":"The shop."}"#));
    let parsed = read(&text).unwrap();
    assert_eq!(parsed, graph);
    assert_eq!(to_string(&parsed), text);
}

fn error(text: &str) -> CanonicalError {
    read(text).unwrap_err()
}

/// The sample export with the first occurrence of `from` replaced by `to`.
fn edited(from: &str, to: &str) -> String {
    let good = sample_export();
    assert!(good.contains(from), "the sample export contains {from}");
    good.replacen(from, to, 1)
}

/// The line number of the first line containing `needle` in the sample export.
fn line_of(needle: &str) -> usize {
    sample_export()
        .lines()
        .position(|line| line.contains(needle))
        .unwrap()
        + 1
}

#[test]
fn rejects_malformed_exports_with_line_numbers() {
    let good = sample_export();
    assert_eq!(error("").message, "missing header record");
    assert_eq!(
        error(&good[..good.len() - 1]).message,
        "the export must end with a newline"
    );
    let without_header: String = good.lines().skip(1).map(|line| format!("{line}\n")).collect();
    assert_eq!(
        error(&without_header).to_string(),
        "line 1: the first record must be the header"
    );
    let header = good.lines().next().unwrap();
    assert_eq!(
        error(&format!("{header}\n{header}\n")).to_string(),
        "line 2: repeated header"
    );
    let blank = good.replacen('\n', "\n\n", 1);
    let failure = error(&blank);
    assert_eq!(failure.line, 2);
    assert!(
        failure.message.starts_with("invalid record: EOF while parsing"),
        "{failure}"
    );
}

#[test]
fn rejects_other_versions_and_formats() {
    let version = edited(r#""version":2"#, r#""version":3"#);
    assert_eq!(
        error(&version).to_string(),
        "line 1: unsupported export version 3"
    );
    let format = edited(r#""format":"unfold-graph""#, r#""format":"unfold-board""#);
    assert_eq!(
        error(&format).to_string(),
        "line 1: unknown export format `unfold-board`"
    );
}

#[test]
fn rejects_unknown_records_and_fields() {
    let record = edited(r#"{"t":"signature""#, r#"{"t":"result""#);
    let failure = error(&record);
    assert_eq!(failure.line, line_of(r#"{"t":"signature""#));
    assert!(
        failure
            .message
            .starts_with("invalid record: unknown variant `result`, expected one of"),
        "{failure}"
    );

    let field = edited(r#""kind":"class""#, r#""kind":"class","colour":"red""#);
    let failure = error(&field);
    assert_eq!(failure.line, line_of(r#""kind":"class""#));
    assert!(
        failure
            .message
            .starts_with("invalid record: unknown field `colour`, expected one of"),
        "{failure}"
    );

    // A field of another record type is just as unknown.
    let foreign = edited(r#"{"t":"input","path""#, r#"{"t":"input","seq":0,"path""#);
    assert!(
        error(&foreign)
            .message
            .starts_with("invalid record: unknown field `seq`"),
        "{}",
        error(&foreign)
    );

    let missing = edited(r#","name":"Shop""#, "");
    assert!(
        error(&missing)
            .message
            .starts_with("invalid record: missing field `name`")
    );
}

#[test]
fn rejects_a_kind_that_does_not_match_the_form() {
    let text = edited(
        r#""kind":"function","form":"lambda""#,
        r#""kind":"call","form":"lambda""#,
    );
    let failure = error(&text);
    assert_eq!(failure.line, line_of(r#""kind":"function","form":"lambda""#));
    assert_eq!(
        failure.message,
        "kind `call` does not match form `lambda`, which is a `function`"
    );
}

#[test]
fn rejects_nodes_whose_parent_is_not_an_earlier_node() {
    let text = edited(
        r#""parent":"shop/payments.py#retry/return[0]""#,
        r#""parent":"shop/payments.py#retry/return[1]""#,
    );
    let failure = error(&text);
    assert_eq!(
        failure.line,
        line_of("shop/payments.py#retry/return[0]/lambda[0]")
    );
    assert_eq!(
        failure.message,
        "parent `shop/payments.py#retry/return[1]` is not an earlier node of the current file"
    );

    let orphan = edited(r#""parent":"shop/payments.py#retry/return[0]","#, "");
    assert_eq!(
        error(&orphan).message,
        "only a file's first node, its module, has no `parent`"
    );

    let duplicate = edited(
        r#""key":"shop/payments.py#charge_card","#,
        r#""key":"shop/payments.py#retry","#,
    );
    assert_eq!(
        error(&duplicate).message,
        "duplicate node key `shop/payments.py#retry`"
    );
}

#[test]
fn rejects_a_reference_to_a_node_the_file_does_not_have() {
    // `for` may name a later node, so it is checked when the file ends, on its own line.
    let text = edited(
        r#""for":"shop/payments.py#charge_card""#,
        r#""for":"shop/payments.py#refund""#,
    );
    let failure = error(&text);
    assert_eq!(failure.line, line_of(r#""role":"decorator""#));
    assert_eq!(
        failure.message,
        "`shop/payments.py#refund` is not a node of the current file"
    );

    let link = edited(
        r#"{"t":"link","from":"shop/payments.py#","#,
        r#"{"t":"link","from":"shop/service.py#","#,
    );
    assert_eq!(
        error(&link).message,
        "`shop/service.py#` is not a node of the current file"
    );
}

#[test]
fn rejects_records_out_of_order() {
    let good = sample_export();
    let lines: Vec<&str> = good.lines().collect();
    let link = lines
        .iter()
        .find(|line| line.starts_with(r#"{"t":"link""#))
        .unwrap();
    assert_eq!(
        error(&format!("{}\n{link}\n", lines[0])).to_string(),
        "line 2: `link` records must follow a `file` record"
    );

    let node = lines[2];
    let first_link = line_of(r#"{"t":"link""#);
    let mut moved: Vec<&str> = lines.clone();
    moved.insert(first_link, node);
    let failure = error(&(moved.join("\n") + "\n"));
    assert_eq!(failure.line, first_link + 1);
    assert_eq!(failure.message, "`node` records cannot follow `link` records");

    let input = lines
        .iter()
        .position(|line| line.starts_with(r#"{"t":"input""#))
        .unwrap();
    let mut late: Vec<&str> = lines.clone();
    late.push(lines[input]);
    assert_eq!(
        error(&(late.join("\n") + "\n")).message,
        "`input` records cannot follow `entrance_target` records"
    );
}

#[test]
fn rejects_targets_that_do_not_follow_their_app_or_entrance() {
    let text = edited(
        r#"{"t":"entrance_target","of":"entrance:shop/place-an-order""#,
        r#"{"t":"entrance_target","of":"entrance:shop/orders-api""#,
    );
    assert_eq!(
        error(&text).message,
        "the target of `entrance:shop/orders-api` does not follow that app or entrance"
    );

    let good = sample_export();
    let app_target = good
        .lines()
        .find(|line| line.contains(r#""of":"entrance:shop","#))
        .unwrap();
    let twice = good.replacen(app_target, &format!("{app_target}\n{app_target}"), 1);
    assert_eq!(error(&twice).message, "app `entrance:shop` has a second target");

    let lines: Vec<&str> = good.lines().collect();
    let entrance = lines
        .iter()
        .find(|line| line.starts_with(r#"{"t":"entrance""#))
        .unwrap();
    assert_eq!(
        error(&format!("{}\n{entrance}\n", lines[0])).message,
        "`entrance` records must follow an `app` record"
    );
}

#[test]
fn rejects_values_the_writer_never_writes() {
    let flags = edited(r#""flags":["exit_value"]"#, r#""flags":[]"#);
    assert_eq!(
        error(&flags).message,
        "`flags` are written only when there are some, once each, in vocabulary order"
    );
    let unordered = edited(r#""flags":["async"]"#, r#""flags":["generator","async"]"#);
    assert!(error(&unordered).message.starts_with("`flags` are written only"));
    let false_flag = edited(r#""bound":0}"#, r#""bound":0,"implements":false}"#);
    assert_eq!(
        error(&false_flag).message,
        "`implements` is written only when true"
    );
    let half_type = edited(r#","type_range":[345,351]"#, "");
    assert_eq!(
        error(&half_type).message,
        "`type` and `type_range` appear together"
    );
    let both = edited(r#""parameter":0}"#, r#""parameter":0,"unpaired":"spread"}"#);
    assert_eq!(
        error(&both).message,
        "`parameter` and `unpaired` exclude each other"
    );
    let reason = edited(r#""to":"unknown_receiver""#, r#""to":"no_idea""#);
    assert_eq!(error(&reason).message, "unknown reason `no_idea`");
}
