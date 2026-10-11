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
    assert_eq!(lines[0], r#"{"t":"header","format":"unfold-graph","version":2}"#);
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
    graph.files[0].types = crate::typed::FileTypes::default();
    let text = to_string(&graph);
    assert!(text.contains(r#"{"t":"file","path":"jobs/tasks.py","language":"python","state":"failed","failure":"jobs/tasks.py:1:1: unexpected token"}"#));
    assert_eq!(read(&text).unwrap(), graph);
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
            "{\"t\":\"header\",\"format\":\"unfold-graph\",\"version\":3}",
            1
        ))
        .message,
        "unsupported export version 3"
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

#[test]
fn typed_records_round_trip_to_identical_bytes() {
    let graph = crate::testkit::sample::typed().build();
    let text = to_string(&graph);
    for line in [
        r#"{"t":"signature","block":"src/charge.ts#function:charge","returns":"Promise<Receipt>","returns_range":[86,102]}"#,
        r#"{"t":"parameter","block":"src/charge.ts#function:charge","index":1,"kind":"rest","name":"items","type":"Item[]","type_range":[77,83]}"#,
        r#"{"t":"member","block":"src/cart.ts#interface:Cart","index":1,"kind":"property","name":"coupon","optional":true,"type":"string","type_range":[49,55]}"#,
        r#"{"t":"member","block":"src/cart.ts#enum:Status","index":0,"kind":"value","name":"Open","value":"\"open\""}"#,
        r#"{"t":"argument","block":"jobs/settle.py#function:settle","index":1,"kind":"keyword_only","name":"currency","default":true}"#,
        r#"{"t":"argument","call":"jobs/settle.py#expression_statement[0]/call[0]","index":1,"keyword":"currency","parameter":1}"#,
        r#"{"t":"result","call":"jobs/settle.py#expression_statement[0]/call[0]","binding":"discard","bound":0}"#,
        r#"{"t":"type_link","block":"src/cart.ts#interface:Cart","range":[30,36],"text":"CartId","resolution":"resolved","target":"src/cart.ts#type_alias:CartId"}"#,
        r#"{"t":"type_link","block":"src/charge.ts#function:charge","range":[86,93],"text":"Promise","resolution":"external","module":"global"}"#,
    ] {
        let line = line.replace(r#"{"t":"argument","block""#, r#"{"t":"parameter","block""#);
        assert!(
            text.lines().any(|written| written == line),
            "missing {line}\nin:\n{text}"
        );
    }
    let parsed = read(&text).unwrap();
    assert_eq!(parsed, graph);
    assert_eq!(to_string(&parsed), text);
}

#[test]
fn heritage_links_round_trip_and_reject_unknown_values() {
    let good = to_string(&crate::testkit::sample::typed().build());
    let plain = r#""target":"src/cart.ts#type_alias:CartId"}"#;
    let text = good.replacen(
        plain,
        r#""target":"src/cart.ts#type_alias:CartId","heritage":"extends"}"#,
        1,
    );
    let parsed = read(&text).unwrap();
    let cart = parsed.file("src/cart.ts").unwrap();
    let link = &cart.types.type_links[0];
    assert_eq!(link.heritage, Some(crate::vocab::Heritage::Extends));
    assert_eq!(to_string(&parsed), text);

    let unknown = good.replacen(
        plain,
        r#""target":"src/cart.ts#type_alias:CartId","heritage":"mixes"}"#,
        1,
    );
    assert_eq!(error(&unknown).message, "unknown heritage `mixes`");
    let misplaced = good.replacen(
        r#""text":"./cart","resolution":"resolved"}"#,
        r#""text":"./cart","resolution":"resolved","heritage":"extends"}"#,
        1,
    );
    assert_eq!(
        error(&misplaced).message,
        "field `heritage` is not allowed on a `edge` record"
    );
}

#[test]
fn typed_records_follow_edges_in_their_file() {
    let text = to_string(&crate::testkit::sample::typed().build());
    let lines: Vec<&str> = text.lines().collect();
    let position = |prefix: &str| lines.iter().position(|line| line.starts_with(prefix)).unwrap();
    let kinds = [
        "edge",
        "signature",
        "parameter",
        "member",
        "argument",
        "result",
        "type_link",
    ];
    let start = position(r#"{"t":"file","path":"src/charge.ts""#);
    let order: Vec<&str> = lines[start + 1..]
        .iter()
        .take_while(|line| !line.starts_with(r#"{"t":"input""#) && !line.starts_with(r#"{"t":"file""#))
        .map(|line| {
            let kind = &line[6..];
            &kind[..kind.find('"').unwrap()]
        })
        .filter(|kind| kinds.contains(kind))
        .collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|kind| kinds.iter().position(|known| known == kind));
    assert_eq!(order, sorted);
}

#[test]
fn version_one_exports_read_without_typed_records() {
    let mut graph = sample().build();
    for file in &mut graph.files {
        file.types = crate::typed::FileTypes::default();
    }
    let mut bytes = Vec::new();
    write_version(&graph, 1, &mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with(r#"{"t":"header","format":"unfold-graph","version":1}"#));
    let (read_back, version) = read_versioned(&text).unwrap();
    assert_eq!((read_back, version), (graph, 1));

    // Typed records written at version 1 are refused, and writing at version 1 drops them.
    let typed = to_string(&crate::testkit::sample::typed().build());
    let downgraded = typed.replacen(r#""version":2"#, r#""version":1"#, 1);
    let failure = read(&downgraded).unwrap_err();
    assert!(failure.message.contains("needs export version 2"), "{failure}");
    let mut bytes = Vec::new();
    write_version(&crate::testkit::sample::typed().build(), 1, &mut bytes).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains(r#""t":"signature""#));
}

#[test]
fn rejects_malformed_typed_records() {
    let good = to_string(&crate::testkit::sample::typed().build());
    let falsy = good.replacen(r#""optional":true"#, r#""optional":false"#, 1);
    assert_eq!(error(&falsy).message, "`optional` is written only when true");
    let both = good.replacen(r#""parameter":1}"#, r#""parameter":1,"unpaired":"spread"}"#, 1);
    assert_eq!(
        error(&both).message,
        "`parameter` and `unpaired` exclude each other"
    );
    let half = good.replacen(r#","returns_range":[86,102]"#, "", 1);
    assert_eq!(
        error(&half).message,
        "`returns` and `returns_range` appear together"
    );
    let foreign = good.replacen(
        r#""binding":"discard""#,
        r#""binding":"discard","target":"src/cart.ts#interface:Cart""#,
        1,
    );
    assert!(
        error(&foreign)
            .message
            .contains("is not a block of the current file")
    );
    let unknown = good.replacen(r#""kind":"rest""#, r#""kind":"varargs""#, 1);
    assert_eq!(error(&unknown).message, "unknown parameter kind `varargs`");
    let wrong = good.replacen(r#""binding":"discard""#, r#""binding":"discard","name":"x""#, 1);
    assert_eq!(
        error(&wrong).message,
        "field `name` is not allowed on a `result` record"
    );
    let wrong = good.replacen(r#""binding":"discard""#, r#""binding":"discard","bound":"x""#, 1);
    assert!(error(&wrong).message.starts_with("invalid record"));
}
