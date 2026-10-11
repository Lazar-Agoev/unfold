//! The typed-record invariants, each with a failing case.

use std::collections::BTreeMap;

use super::*;
use crate::key::Key;
use crate::model::{Graph, ModuleLabel};
use crate::testkit::GraphBuilder;
use crate::testkit::sample::{heritage, typed};
use crate::typed::{FileTypes, Pairing, WrittenType};
use crate::vocab::{Expansion, MemberKind, PairingReason, ParameterKind, Reason, Resolution, ResultBinding};

struct Tokens(Language);

impl Census for Tokens {
    fn language(&self) -> Language {
        self.0
    }
    fn is_token_range(&self, _path: &RepoPath, range: ByteRange) -> bool {
        // Byte 1001 is "inside a token", so a test can move a range off a boundary.
        range.start != 1001
    }
}

fn sources(builder: &GraphBuilder) -> BTreeMap<RepoPath, Snapshot> {
    builder
        .snapshots()
        .into_iter()
        .map(|snapshot| (snapshot.path().clone(), snapshot))
        .collect()
}

fn violations(graph: &Graph) -> Vec<String> {
    let builder = typed();
    let sources = sources(&builder);
    let typescript = Tokens(Language::TypeScript);
    let python = Tokens(Language::Python);
    Validator::new()
        .with_sources(&sources)
        .with_census(&typescript)
        .with_census(&python)
        .validate(graph)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[track_caller]
fn assert_violation(graph: &Graph, needle: &str) {
    let found = violations(graph);
    assert!(
        found.iter().any(|message| message.contains(needle)),
        "no violation contains `{needle}`; found {found:#?}"
    );
}

fn types<'g>(graph: &'g mut Graph, path: &str) -> &'g mut FileTypes {
    &mut graph
        .files
        .iter_mut()
        .find(|file| file.file.path.as_str() == path)
        .expect("the typed sample has this file")
        .types
}

const CART: &str = "src/cart.ts";
const CHARGE: &str = "src/charge.ts";
const SETTLE: &str = "jobs/settle.py";

#[test]
fn the_typed_sample_is_valid() {
    assert_eq!(violations(&typed().build()), Vec::<String>::new());
}

#[test]
fn every_function_like_block_has_one_signature() {
    let mut graph = typed().build();
    types(&mut graph, CHARGE).signatures.remove(1);
    assert_violation(
        &graph,
        "function:pay: a function-like block needs exactly one signature",
    );

    let mut graph = typed().build();
    let duplicate = types(&mut graph, CHARGE).signatures[0].clone();
    types(&mut graph, CHARGE).signatures.insert(0, duplicate);
    assert_violation(&graph, "signatures are not in block order, one per block");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).signatures[0].block = 3;
    assert_violation(&graph, "a return has no signature");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).signatures[0].returns = None;
    assert_violation(&graph, "the return_type slot's type is not recorded");

    let mut graph = typed().build();
    if let Some(returns) = &mut types(&mut graph, CHARGE).signatures[0].returns {
        returns.range.start -= 2;
    }
    assert_violation(&graph, "returns range [84, 102) is outside [86, 102)");

    // Version 1 graphs don't need typed records at all.
    let mut graph = typed().build();
    types(&mut graph, CHARGE).signatures.clear();
    assert!(
        Validator::new()
            .without_typed_records()
            .validate(&graph)
            .is_empty()
    );
}

#[test]
fn parameters_follow_their_slots_and_the_language() {
    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters.remove(2);
    assert_violation(
        &graph,
        "slot parameter[0] of src/charge.ts#function:pay: the parameter slot has no parameter record",
    );

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[2].index = 4;
    assert_violation(&graph, "no parameter slot has this index");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[0].kind = ParameterKind::KeywordOnly;
    assert_violation(&graph, "kind keyword_only does not occur in typescript files");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[1].kind = ParameterKind::This;
    assert_violation(&graph, "a this parameter is the first parameter");

    let mut graph = typed().build();
    types(&mut graph, SETTLE).parameters[1].kind = ParameterKind::PositionalOnly;
    assert_violation(&graph, "Python parameters are not in the language's order");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[0].kind = ParameterKind::Rest;
    assert_violation(&graph, "a second rest parameter");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[0].name = Some("".into());
    assert_violation(&graph, "name is empty");

    let mut graph = typed().build();
    types(&mut graph, SETTLE).parameters[0].optional = true;
    assert_violation(&graph, "only TypeScript parameters are optional");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[0].ty = Some(WrittenType {
        text: "Cart".into(),
        range: ByteRange { start: 147, end: 151 },
    });
    assert_violation(&graph, "type range [147, 151) is outside [55, 65)");
}

#[test]
fn members_belong_to_type_declarations_in_order() {
    let mut graph = typed().build();
    types(&mut graph, CART).members[1].index = 2;
    assert_violation(&graph, "member indices must be contiguous from zero; expected 1");

    let mut graph = typed().build();
    types(&mut graph, CART).members[0].block = 2;
    types(&mut graph, CART).members[1].block = 2;
    types(&mut graph, CART).members.swap(0, 2);
    assert_violation(&graph, "members are not in block, then index, order");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).members.push(crate::typed::Member {
        block: 2,
        index: 0,
        kind: MemberKind::Property,
        name: Some("x".into()),
        optional: false,
        readonly: false,
        default: false,
        ty: None,
        value: None,
    });
    assert_violation(&graph, "a function has no members");

    let mut graph = typed().build();
    types(&mut graph, CART).members[2].kind = MemberKind::Property;
    assert_violation(&graph, "value members are exactly the members of an enum");
    assert_violation(&graph, "only a value member has a value");

    let mut graph = typed().build();
    types(&mut graph, CART).members[0].ty = Some(WrittenType {
        text: "x".into(),
        range: ByteRange { start: 70, end: 71 },
    });
    assert_violation(&graph, "type range [70, 71) is outside [0, 58)");
}

#[test]
fn arguments_follow_their_slots_and_pair_only_with_resolved_targets() {
    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments.remove(1);
    assert_violation(&graph, "the argument slot has no argument record");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments[0].expand = Some(Expansion::Keywords);
    assert_violation(&graph, "expansion keywords does not occur in typescript files");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments[0].keyword = Some("cart".into());
    assert_violation(&graph, "only Python arguments have keywords");

    let mut graph = typed().build();
    types(&mut graph, SETTLE).arguments[1].expand = Some(Expansion::Keywords);
    assert_violation(&graph, "an expanded argument has no keyword");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments[1].pairing = Some(Pairing::Unpaired(PairingReason::Spread));
    assert_violation(&graph, "only an argument of a resolved occurrence is paired");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments[0].pairing = None;
    assert_violation(&graph, "needs a parameter or an unpaired reason");

    let mut graph = typed().build();
    types(&mut graph, SETTLE).arguments[1].pairing = Some(Pairing::Parameter(2));
    assert_violation(&graph, "the target has no parameter 2");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).parameters[2].kind = ParameterKind::This;
    assert_violation(&graph, "parameter 0 of the target is a this parameter");
}

#[test]
fn every_call_has_one_result_with_a_fitting_target() {
    let mut graph = typed().build();
    types(&mut graph, CHARGE).results.remove(1);
    assert_violation(&graph, "new[0]: a call or new needs exactly one result");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).results[0].binding = ResultBinding::Variable;
    assert_violation(
        &graph,
        "a target is present exactly for a variable or argument binding",
    );

    let mut graph = typed().build();
    let result = &mut types(&mut graph, CHARGE).results[0];
    result.binding = ResultBinding::Variable;
    result.target = Some(2);
    assert_violation(&graph, "is not an enclosing block of the binding's kind");

    let mut graph = typed().build();
    let result = &mut types(&mut graph, CHARGE).results[0];
    result.binding = ResultBinding::Argument;
    result.target = Some(7);
    assert_violation(&graph, "a position is present exactly for an argument binding");
    assert_violation(&graph, "is not an enclosing block of the binding's kind");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).results[1].call = 3;
    assert_violation(&graph, "a return has no result");
}

#[test]
fn type_links_are_ordered_inside_their_innermost_block_with_edge_outcomes() {
    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links.swap(0, 1);
    assert_violation(&graph, "type links are not in block, then range, order");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[4].block = 2;
    types(&mut graph, CHARGE).type_links[4].range = ByteRange { start: 147, end: 151 };
    assert_violation(&graph, "is empty or outside the block's [32, 126)");

    let mut graph = typed().build();
    let link = &mut types(&mut graph, CHARGE).type_links[3];
    link.range = ByteRange { start: 114, end: 117 };
    link.text = "pay".into();
    assert_violation(&graph, "is a nearer block containing the name");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[1].range.end = 90;
    assert_violation(&graph, "type links overlap");

    let mut graph = typed().build();
    let link = &mut types(&mut graph, CHARGE).type_links[0];
    link.resolution = Resolution::Unresolved;
    assert_violation(&graph, "a reason is present exactly when unresolved");
    assert_violation(&graph, "a target is present exactly when resolved");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[2].module = None;
    assert_violation(&graph, "a module is present exactly when external");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[2].module = Some(ModuleLabel::from_text("[\"b\",\"a\"]"));
    assert_violation(&graph, "is not a label or a sorted set of labels");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[0].target = Some(Key::from_text("src/charge.ts#function:pay"));
    assert_violation(&graph, "a type link cannot target a function");

    let mut graph = typed().build();
    let link = &mut types(&mut graph, SETTLE).type_links[0];
    link.resolution = Resolution::Resolved;
    link.module = None;
    link.target = Some(Key::from_text("src/cart.ts#interface:Cart"));
    assert_violation(&graph, "a resolved type link cannot cross languages");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[0].text = "x".repeat(121).into();
    assert_violation(&graph, "text is empty or exceeds 120 UTF-16 code units");

    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[0].reason = Some(Reason::Ambiguous);
    assert_violation(&graph, "a reason is present exactly when unresolved");
}

fn heritage_violations(graph: &Graph) -> Vec<String> {
    let builder = heritage();
    let sources = sources(&builder);
    Validator::new()
        .with_sources(&sources)
        .validate(graph)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn heritage_links_name_bases_of_classes_and_interfaces() {
    use crate::vocab::Heritage;
    assert_eq!(heritage_violations(&heritage().build()), Vec::<String>::new());

    // Only a TypeScript class implements.
    let mut graph = heritage().build();
    types(&mut graph, "src/shop.ts").type_links[0].heritage = Some(Heritage::Implements);
    let found = heritage_violations(&graph);
    assert!(
        found
            .iter()
            .any(|message| message.contains("`implements` heritage is not allowed on this interface")),
        "{found:#?}"
    );
    let mut graph = heritage().build();
    types(&mut graph, "jobs/models.py").type_links[0].heritage = Some(Heritage::Implements);
    let found = heritage_violations(&graph);
    assert!(
        found
            .iter()
            .any(|message| message.contains("`implements` heritage is not allowed on this class")),
        "{found:#?}"
    );

    // An `extends` name lies in the heritage slot; a class's `implements` clause is no slot.
    let mut graph = heritage().build();
    types(&mut graph, "src/shop.ts").type_links[1].heritage = Some(Heritage::Extends);
    let found = heritage_violations(&graph);
    assert!(
        found
            .iter()
            .any(|message| message.contains("an `extends` link lies outside its block's heritage slot")),
        "{found:#?}"
    );

    // A function has no heritage.
    let mut graph = typed().build();
    types(&mut graph, CHARGE).type_links[0].heritage = Some(Heritage::Extends);
    assert_violation(&graph, "`extends` heritage is not allowed on this function");
}

#[test]
fn written_ranges_lie_on_character_and_token_boundaries() {
    let mut builder = typed();
    let file = builder.file("src/name.ts", Language::TypeScript, "type Café = string;\n");
    let alias = builder.block(
        file,
        BlockKind::TypeAlias,
        Some(crate::vocab::Role::Body),
        Some("Café"),
        builder.find(file, "type Café = string;"),
    );
    builder.slot(alias, SlotName::Name, None, builder.find(file, "Café"));
    builder.slot(alias, SlotName::Type, None, builder.find(file, "string"));
    let mut graph = builder.build();
    let sources = sources(&builder);
    assert!(
        Validator::new()
            .with_sources(&sources)
            .validate(&graph)
            .is_empty()
    );
    types(&mut graph, "src/name.ts")
        .type_links
        .push(crate::typed::TypeLink {
            block: 1,
            range: ByteRange { start: 5, end: 9 },
            text: "Caf".into(),
            resolution: Resolution::Unresolved,
            target: None,
            reason: Some(Reason::UnknownName),
            module: None,
            heritage: None,
        });
    let found: Vec<String> = Validator::new()
        .with_sources(&sources)
        .validate(&graph)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        found
            .iter()
            .any(|message| message.contains("type link `Caf` at [5, 9) is not on character boundaries")),
        "{found:#?}"
    );

    let mut graph = typed().build();
    if let Some(ty) = &mut types(&mut graph, CHARGE).parameters[0].ty {
        ty.range = ByteRange {
            start: 1001,
            end: 1002,
        };
    }
    let census = Tokens(Language::TypeScript);
    let found: Vec<String> = Validator::new()
        .with_census(&census)
        .validate(&graph)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        found
            .iter()
            .any(|message| message.contains("is not on character and token boundaries")),
        "{found:#?}"
    );
}

#[test]
fn bound_counts_need_a_resolved_function_like_target() {
    // A result's count needs its call resolved to a function-like block.
    let mut graph = typed().build();
    types(&mut graph, CHARGE).results[1].bound = Some(0);
    assert_violation(
        &graph,
        "only a call resolved to a function-like block has a bound count",
    );

    // An argument's count needs a passed callable at its position.
    let mut graph = typed().build();
    types(&mut graph, CHARGE).arguments[0].bound = Some(1);
    assert_violation(
        &graph,
        "only an argument that passes a function-like callable has a bound count",
    );
}
