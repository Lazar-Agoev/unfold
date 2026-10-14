//! The lowering, row by row of the TypeScript lowering table, and on the worked examples: each
//! source is compiled into a valid graph (checked against its census) and its node tree printed.
//!
//! - Each source in `testdata/fixtures/typescript-structure/` is compared with the golden
//!   `.tree` beside it: every field of every node ([`outline`]). Run with `UNFOLD_BLESS=1` to
//!   rewrite them after an intended change, and review them.
//! - The row tests print trees ([`crate::support::tree`]): each node's key step, form, flags,
//!   role and `for`, `binds`, target, and head.

use unfold_core::testkit::{assert_golden, testdata_dir};

use crate::support::{compile_source, linked_tree_of, outline};

const FOLDER: &str = "fixtures/typescript-structure";

fn check(name: &str) {
    let path = testdata_dir().join(FOLDER).join(format!("{name}.ts"));
    let source = std::fs::read_to_string(&path).expect("read the case");
    let compiled = compile_source("a.ts", &source).valid();
    let mut text = outline(compiled.file("a.ts"), &source).join("\n");
    text.push('\n');
    assert_golden(format!("{FOLDER}/{name}.tree"), &text);
}

#[test]
fn structure_imports() {
    check("imports");
}

#[test]
fn structure_classes() {
    check("classes");
}

#[test]
fn structure_statements() {
    check("statements");
}

#[test]
fn structure_destructuring() {
    check("destructuring");
}

#[test]
fn structure_names() {
    check("names");
}

#[test]
fn structure_calls() {
    check("calls");
}

#[test]
fn structure_ambient() {
    check("ambient");
}

#[test]
fn structure_types() {
    check("types");
}

#[track_caller]
fn assert_tree_at(path: &str, source: &str, expected: &[&str]) {
    let tree = linked_tree_of(path, source);
    assert!(tree == expected, "tree:\n{}", tree.join("\n"));
}

#[test]
fn module_and_docs() {
    assert_tree_at(
        "a.ts",
        r#"/** Orders: placing and charging them. */

import { save } from "./store";

/** Charges a card. */
export function charge(card: string) {
  return save(card);
}

boot();
"#,
        &[
            "(module) doc=`/** Orders: placing and charging them. */`",
            "charge function head=`function charge(card: string)` doc=`/** Charges a card. */`",
            "  /return[0] return",
            "    /call:save call exit_value head=`save`",
            "      ↳ calls `save` → unknown not_found",
            "/call:boot call head=`boot`",
            "  ↳ calls `boot` → unknown unknown_name",
            "↳ imports `./store` → unknown not_found",
        ],
    );
}

/// The `export` and the JSDoc of `export let sys = (() => { … })();` belong to the
/// declaration: a function declared inside the arrow starts at its own `function` keyword, and
/// its doc is only a JSDoc written directly before it.
#[test]
fn an_exported_declaration_ends_at_nested_functions() {
    let source = r#"/** The system. */
export let sys: System = (() => {
    function getNodeSystem() {
        return load();
    }

    /** Wraps the node system. */
    function wrap() {}

    return wrap();
})();
"#;
    let compiled = compile_source("a.ts", source).valid();
    let lines = outline(compiled.file("a.ts"), source);
    // A node's outline: its key line and the indented lines below it.
    let node = |key: &str| -> Vec<&str> {
        let start = lines
            .iter()
            .position(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key} in:\n{}", lines.join("\n")));
        let end = lines[start + 1..]
            .iter()
            .position(|line| !line.starts_with(' '))
            .map_or(lines.len(), |offset| start + 1 + offset);
        lines[start..end].iter().map(String::as_str).collect()
    };
    assert_eq!(
        node("a.ts#/call:(…)/lambda[0].getNodeSystem "),
        [
            "a.ts#/call:(…)/lambda[0].getNodeSystem function",
            "  range `function getNodeSystem() {⏎        return load();⏎    }`",
            "  head `function getNodeSystem()`",
            "  body `{⏎        return load();⏎    }`",
            "  stmt `function getNodeSystem() {⏎        return load();⏎    }`",
            "  name `getNodeSystem`",
        ],
    );
    assert_eq!(
        node("a.ts#/call:(…)/lambda[0].wrap "),
        [
            "a.ts#/call:(…)/lambda[0].wrap function",
            "  range `function wrap() {}`",
            "  head `function wrap()`",
            "  body `{}`",
            "  doc `/** Wraps the node system. */`",
            "  stmt `function wrap() {}`",
            "  name `wrap`",
        ],
    );
}

#[test]
fn functions_in_every_form() {
    assert_tree_at(
        "a.ts",
        r#"function declared() {}
async function* stream() {}
class Repo {
  constructor() {}
  save() {}
  get size() { return 0; }
  set size(value: number) {}
  static create() {}
}
const literal = { method() {}, get value() { return 1; }, set value(v) {} };
"#,
        &[
            "declared function head=`function declared()`",
            "stream function async generator head=`async function* stream()`",
            "Repo class head=`class Repo`",
            "  .constructor constructor head=`constructor()`",
            "  .save method head=`save()`",
            "  .size getter head=`get size()`",
            "    /return[0] return",
            "  .size[1] setter head=`set size(value: number)`",
            "  .create method head=`static create()`",
            "literal.method method head=`method()`",
            "literal.value getter head=`get value()`",
            "  /return[0] return",
            "literal.value[1] setter head=`set value(v)`",
        ],
    );
}

#[test]
fn inline_functions() {
    assert_tree_at(
        "a.ts",
        r#"app.get("/health", (req, res) => res.send("ok"));
useMutation({ onSuccess: () => refresh(), onError(error) { report(error); } });
items.map(convert).forEach(function (item) { print(item); });
(() => boot())();
(async function main() { await run(); })();
const handler = async () => {};
const handlers = { onCreate: () => create(), onDelete() {} };
export default () => start();
const list = [() => first()];
schedule("0 2 * * *", nightly);
on(`ready`, () => ready());
on(event, () => other());
"#,
        &[
            "/call:app.get(\"/health\") call head=`app.get`",
            "  ↳ calls `app.get` → unknown unknown_receiver",
            "  /callback[0] lambda head=`(req, res) =>`",
            "    /call:res.send call head=`res.send`",
            "      ↳ calls `res.send` → unknown unknown_receiver",
            "/call:useMutation call head=`useMutation`",
            "  ↳ calls `useMutation` → unknown unknown_name",
            "  /callback[0] lambda head=`() =>`",
            "    /call:refresh call head=`refresh`",
            "      ↳ calls `refresh` → unknown unknown_name",
            "  /callback[1] lambda head=`onError(error)`",
            "    /call:report call head=`report`",
            "      ↳ calls `report` → unknown unknown_name",
            "/call:items.map().forEach call head=`items.map(convert).forEach`",
            "  ↳ calls `items.map().forEach` → unknown unknown_receiver",
            "  /call:items.map call head=`items.map`",
            "    ↳ calls `items.map` → unknown unknown_receiver",
            "  /callback[0] lambda head=`function (item)`",
            "    /call:print call head=`print`",
            "      ↳ calls `print` → library global",
            "/call:(…) call head=`(() => boot())`",
            "  ↳ calls `(…)` → project a.ts#/call:(…)/lambda[0]",
            "  /lambda[0] lambda head=`() =>`",
            "    /call:boot call head=`boot`",
            "      ↳ calls `boot` → unknown unknown_name",
            "/call:(…)[1] call head=`(async function main() { await run(); })`",
            "  ↳ calls `(…)` → project a.ts#/call:(…)[1]/lambda[0]",
            "  /lambda[0] lambda async head=`async function main()`",
            "    /call:run call awaited head=`run`",
            "      ↳ calls `run` → unknown unknown_name",
            "handler lambda async head=`async () =>`",
            "handlers.onCreate lambda head=`() =>`",
            "  /call:create call head=`create`",
            "    ↳ calls `create` → unknown unknown_name",
            "handlers.onDelete method head=`onDelete()`",
            "default lambda head=`() =>`",
            "  /call:start call head=`start`",
            "    ↳ calls `start` → unknown unknown_name",
            "/lambda[0] lambda head=`() =>`",
            "  /call:first call head=`first`",
            "    ↳ calls `first` → unknown unknown_name",
            "/call:schedule call head=`schedule`",
            "  ↳ calls `schedule` → unknown unknown_name",
            "/call:on(\"ready\") call head=`on`",
            "  ↳ calls `on` → unknown unknown_name",
            "  /callback[0] lambda head=`() =>`",
            "    /call:ready call head=`ready`",
            "      ↳ calls `ready` → unknown unknown_name",
            "/call:on call head=`on`",
            "  ↳ calls `on` → unknown unknown_name",
            "  /callback[0] lambda head=`() =>`",
            "    /call:other call head=`other`",
            "      ↳ calls `other` → unknown unknown_name",
        ],
    );
}

#[test]
fn classes_and_types() {
    assert_tree_at(
        "a.ts",
        r#"class Order extends Base implements Payable {}
interface Payable { pay(): void }
type Id = string;
enum Status { New, Paid }
export const enum Mode { On }
declare class Ambient {}
"#,
        &[
            "Order class head=`class Order extends Base implements Payable`",
            "  ↳ extends `Base` → unknown unknown_name",
            "  ↳ extends `Payable` → project a.ts#Payable (implements)",
            "Payable interface head=`interface Payable`",
            "Id type_alias head=`type Id`",
            "Status enum head=`enum Status`",
            "Mode enum head=`const enum Mode`",
        ],
    );
}

#[test]
fn decorators() {
    assert_tree_at(
        "a.ts",
        r#"@Injectable()
export class Service {
  @Inject(TOKEN) private store!: Store;
  @Get("/orders")
  list(@Query() query: string) { return query; }
  @Field() handler = () => handle();
  @log
  static create() {}
}
"#,
        &[
            "/call:Injectable call role=decorator for=Service head=`Injectable`",
            "  ↳ calls `Injectable` → unknown unknown_name",
            "Service class head=`class Service`",
            "  ↳ types `Store` → unknown unknown_name",
            "  /call:Inject call head=`Inject`",
            "    ↳ calls `Inject` → unknown unknown_name",
            "  /call:Get call role=decorator for=Service.list head=`Get`",
            "    ↳ calls `Get` → unknown unknown_name",
            "  /call:Query call role=decorator for=Service.list head=`Query`",
            "    ↳ calls `Query` → unknown unknown_name",
            "  .list method head=`list(@Query() query: string)`",
            "    /return[0] return",
            "  /call:Field call role=decorator for=Service.handler head=`Field`",
            "    ↳ calls `Field` → unknown unknown_name",
            "  .handler lambda head=`() =>`",
            "    /call:handle call head=`handle`",
            "      ↳ calls `handle` → unknown unknown_name",
            "  /call:log call role=decorator for=Service.create head=`log`",
            "    ↳ calls `log` → unknown unknown_name",
            "  .create method head=`static create()`",
        ],
    );
}

/// Each node's key with the text of its `stmt`, for a source that compiles into a valid graph.
fn statements_of(source: &str) -> Vec<(String, Option<&str>)> {
    let compiled = compile_source("a.ts", source).valid();
    compiled
        .file("a.ts")
        .nodes
        .iter()
        .skip(1)
        .map(|node| {
            let stmt = node
                .stmt
                .map(|stmt| &source[stmt.start as usize..stmt.end as usize]);
            (node.key.to_string(), stmt)
        })
        .collect()
}

/// A decorator is part of its definition's statement: its calls begin none, and the definition
/// is the statement's first node, from its first decorator to its end. A class member is a
/// statement of the class body.
#[test]
fn a_definition_begins_its_statement_from_its_first_decorator() {
    let source = r#"@first() @second
class Plain extends base() {}
@dec export class Exported {}
export @dec class Inner {
  @route("/a") @auth
  handle() {}
  helper() {}
}
class Bare extends mixin() {}
"#;
    assert_eq!(
        statements_of(source),
        [
            ("a.ts#/call:first".to_owned(), None),
            ("a.ts#/call:second".to_owned(), None),
            ("a.ts#/call:base".to_owned(), None),
            (
                "a.ts#Plain".to_owned(),
                Some("@first() @second\nclass Plain extends base() {}")
            ),
            ("a.ts#/call:dec".to_owned(), None),
            ("a.ts#Exported".to_owned(), Some("@dec export class Exported {}")),
            ("a.ts#/call:dec[1]".to_owned(), None),
            (
                "a.ts#Inner".to_owned(),
                Some(&source[source.find("export @dec").unwrap()..source.find("\nclass Bare").unwrap()])
            ),
            ("a.ts#Inner/call:route".to_owned(), None),
            ("a.ts#Inner/call:auth".to_owned(), None),
            (
                "a.ts#Inner.handle".to_owned(),
                Some("@route(\"/a\") @auth\n  handle() {}")
            ),
            ("a.ts#Inner.helper".to_owned(), Some("helper() {}")),
            ("a.ts#/call:mixin".to_owned(), None),
            ("a.ts#Bare".to_owned(), Some("class Bare extends mixin() {}")),
        ],
    );
}

#[test]
fn parameter_defaults() {
    assert_tree_at(
        "a.ts",
        r#"function page(size = defaultSize(), { offset = start() } = {}, fallback = () => none()) {
  return fetchPage(size, offset);
}
const arrow = (limit = max()) => limit;
"#,
        &[
            "page function head=`function page(size = defaultSize(), { offset = start() } = {}, fallback = () => none())`",
            "  /call:defaultSize call role=default head=`defaultSize`",
            "    ↳ calls `defaultSize` → unknown unknown_name",
            "  /call:start call role=default head=`start`",
            "    ↳ calls `start` → unknown unknown_name",
            "  /lambda[0] lambda head=`() =>`",
            "    /call:none call head=`none`",
            "      ↳ calls `none` → unknown unknown_name",
            "  /return[0] return",
            "    /call:fetchPage call exit_value head=`fetchPage`",
            "      ↳ calls `fetchPage` → unknown unknown_name",
            "arrow lambda head=`(limit = max()) =>`",
            "  /call:max call role=default head=`max`",
            "    ↳ calls `max` → unknown unknown_name",
        ],
    );
}

#[test]
fn decisions() {
    assert_tree_at(
        "a.ts",
        r#"function route(order) {
  if (order.paid) {
    ship(order);
  } else if (isDraft(order)) {
    return;
  } else {
    queue(order);
  }
  const label = order.rush ? priority(order) : "normal";
  const plain = order.rush ? 1 : 2;
  return order.vip ? vip(order) : standard(order);
}
"#,
        &[
            "route function head=`function route(order)`",
            "  /if[0] if",
            "    /then then head=`order.paid`",
            "      /call:ship call head=`ship`",
            "        ↳ calls `ship` → unknown unknown_name",
            "    /elif[0] elif head=`isDraft(order)`",
            "      /call:isDraft call role=test head=`isDraft`",
            "        ↳ calls `isDraft` → unknown unknown_name",
            "      /return[0] return",
            "    /else else",
            "      /call:queue call head=`queue`",
            "        ↳ calls `queue` → unknown unknown_name",
            "  /ternary[0] ternary binds=`label`",
            "    /then then head=`order.rush`",
            "      /call:priority call head=`priority`",
            "        ↳ calls `priority` → unknown unknown_name",
            "    /else else",
            "  /return[0] return",
            "    /ternary[0] ternary exit_value",
            "      /then then head=`order.vip`",
            "        /call:vip call head=`vip`",
            "          ↳ calls `vip` → unknown unknown_name",
            "      /else else",
            "        /call:standard call head=`standard`",
            "          ↳ calls `standard` → unknown unknown_name",
        ],
    );
}

#[test]
fn switches() {
    assert_tree_at(
        "a.ts",
        r#"function status(order) {
  switch (classify(order)) {
    case "paid":
      return order;
    case "new":
    case "open": {
      notify(order);
      break;
    }
    case "late":
      warn(order);
    default:
      if (order.held) break;
      fallback(order);
  }
}
"#,
        &[
            "status function head=`function status(order)`",
            "  /switch[0] switch head=`classify(order)`",
            "    /call:classify call head=`classify`",
            "      ↳ calls `classify` → unknown unknown_name",
            "    /case[0] case head=`\"paid\"`",
            "      /return[0] return",
            "    /case[1] case falls_through head=`\"new\"`",
            "    /case[2] case head=`\"open\"`",
            "      /call:notify call head=`notify`",
            "        ↳ calls `notify` → unknown unknown_name",
            "    /case[3] case falls_through head=`\"late\"`",
            "      /call:warn call head=`warn`",
            "        ↳ calls `warn` → unknown unknown_name",
            "    /default default",
            "      /if[0] if",
            "        /then then head=`order.held`",
            "          /break[0] break target=status/switch[0]",
            "      /call:fallback call head=`fallback`",
            "        ↳ calls `fallback` → unknown unknown_name",
        ],
    );
}

#[test]
fn loops() {
    assert_tree_at(
        "a.ts",
        r#"async function each(items) {
  for (let i = 0; i < count(); i++) step(i);
  for (const key in lookup()) use(key);
  for (const item of items) handle(item);
  for await (const page of pages()) save(page);
  while (more()) work();
  do retry(); while (failing());
}
"#,
        &[
            "each function async head=`async function each(items)`",
            "  /for[0] for head=`for (let i = 0; i < count(); i++)`",
            "    /call:count call head=`count`",
            "      ↳ calls `count` → unknown unknown_name",
            "    /body body",
            "      /call:step call head=`step`",
            "        ↳ calls `step` → unknown unknown_name",
            "  /for[1] for head=`for (const key in lookup())`",
            "    /call:lookup call head=`lookup`",
            "      ↳ calls `lookup` → unknown unknown_name",
            "    /body body",
            "      /call:use call head=`use`",
            "        ↳ calls `use` → unknown unknown_name",
            "  /for[2] for head=`for (const item of items)`",
            "    /body body",
            "      /call:handle call head=`handle`",
            "        ↳ calls `handle` → unknown unknown_name",
            "  /for[3] for head=`for await (const page of pages())`",
            "    /call:pages call head=`pages`",
            "      ↳ calls `pages` → unknown unknown_name",
            "    /body body",
            "      /call:save call head=`save`",
            "        ↳ calls `save` → unknown unknown_name",
            "  /while[0] while head=`while (more())`",
            "    /call:more call head=`more`",
            "      ↳ calls `more` → unknown unknown_name",
            "    /body body",
            "      /call:work call head=`work`",
            "        ↳ calls `work` → unknown unknown_name",
            "  /do[0] do head=`while (failing())`",
            "    /body body",
            "      /call:retry call head=`retry`",
            "        ↳ calls `retry` → unknown unknown_name",
            "    /call:failing call head=`failing`",
            "      ↳ calls `failing` → unknown unknown_name",
        ],
    );
}

#[test]
fn try_catch_finally() {
    assert_tree_at(
        "a.ts",
        r#"function guarded() {
  try {
    risky();
  } catch (error: unknown) {
    report(error);
  } finally {
    cleanup();
  }
  try { other(); } catch { ignore(); }
}
"#,
        &[
            "guarded function head=`function guarded()`",
            "  /try[0] try",
            "    /body body",
            "      /call:risky call head=`risky`",
            "        ↳ calls `risky` → unknown unknown_name",
            "    /catch catch head=`catch (error: unknown)`",
            "      /call:report call head=`report`",
            "        ↳ calls `report` → unknown unknown_name",
            "    /finally finally",
            "      /call:cleanup call head=`cleanup`",
            "        ↳ calls `cleanup` → unknown unknown_name",
            "  /try[1] try",
            "    /body body",
            "      /call:other call head=`other`",
            "        ↳ calls `other` → unknown unknown_name",
            "    /catch catch head=`catch`",
            "      /call:ignore call head=`ignore`",
            "        ↳ calls `ignore` → unknown unknown_name",
        ],
    );
}

#[test]
fn using_blocks() {
    assert_tree_at(
        "a.ts",
        r#"async function useResources() {
  using file = open("a.txt");
  await using lock = acquire();
  read(file);
}
function lastStatement() {
  using connection = connect();
}
"#,
        &[
            "useResources function async head=`async function useResources()`",
            "  /using[0] using head=`using file = open(\"a.txt\");`",
            "    /call:open call binds=`file` head=`open`",
            "      ↳ calls `open` → library global",
            "    /body body",
            "      /using[0] using async head=`await using lock = acquire();`",
            "        /call:acquire call binds=`lock` head=`acquire`",
            "          ↳ calls `acquire` → unknown unknown_name",
            "        /body body",
            "          /call:read call head=`read`",
            "            ↳ calls `read` → unknown unknown_name",
            "lastStatement function head=`function lastStatement()`",
            "  /using[0] using head=`using connection = connect();`",
            "    /call:connect call binds=`connection` head=`connect`",
            "      ↳ calls `connect` → unknown unknown_name",
            "    /body body",
        ],
    );
}

#[test]
fn exits() {
    assert_tree_at(
        "a.ts",
        r#"function* exits(items) {
  outer: for (const item of items) {
    for (const part of item.parts) {
      if (skip(part)) continue outer;
      if (stop(part)) break outer;
      if (done(part)) break;
    }
  }
  yield produce();
  const reply = yield;
  if (!reply) throw new Error("no reply");
  return finish(reply);
}
"#,
        &[
            "exits function generator head=`function* exits(items)`",
            "  /for[0] for head=`for (const item of items)`",
            "    /body body",
            "      /for[0] for head=`for (const part of item.parts)`",
            "        /body body",
            "          /if[0] if",
            "            /then then head=`skip(part)`",
            "              /call:skip call role=test head=`skip`",
            "                ↳ calls `skip` → unknown unknown_name",
            "              /continue[0] continue target=exits/for[0]",
            "          /if[1] if",
            "            /then then head=`stop(part)`",
            "              /call:stop call role=test head=`stop`",
            "                ↳ calls `stop` → library global",
            "              /break[0] break target=exits/for[0]",
            "          /if[2] if",
            "            /then then head=`done(part)`",
            "              /call:done call role=test head=`done`",
            "                ↳ calls `done` → unknown unknown_name",
            "              /break[0] break target=exits/for[0]/body/for[0]",
            "  /yield[0] yield",
            "    /call:produce call exit_value head=`produce`",
            "      ↳ calls `produce` → unknown unknown_name",
            "  /yield[1] yield",
            "  /if[0] if",
            "    /then then head=`!reply`",
            "      /throw[0] throw",
            "        /call:Error new exit_value head=`Error`",
            "          ↳ calls `Error` → builtin ecmascript",
            "  /return[0] return",
            "    /call:finish call exit_value head=`finish`",
            "      ↳ calls `finish` → unknown unknown_name",
        ],
    );
}

#[test]
fn calls_of_every_form() {
    assert_tree_at(
        "a.ts",
        r#"save(order);
res.status(500).json({ error: String(error) });
handlers[kind](event);
", ".join(names);
(row.get("x") || "").strip();
new Date();
sql`select ${columns()}`;
const module = await import("./lazy.js");
super_.call(this);
"#,
        &[
            "/call:save call head=`save`",
            "  ↳ calls `save` → unknown unknown_name",
            "/call:res.status().json call head=`res.status(500).json`",
            "  ↳ calls `res.status().json` → unknown unknown_receiver",
            "  /call:res.status call head=`res.status`",
            "    ↳ calls `res.status` → unknown unknown_receiver",
            "  /call:String call head=`String`",
            "    ↳ calls `String` → builtin ecmascript",
            "/call:handlers%5B%5D call head=`handlers[kind]`",
            "  ↳ calls `handlers[…]` → unknown dynamic",
            "/call:\"\".join call head=`\", \".join`",
            "  ↳ calls `(…).join` → unknown unknown_receiver",
            "/call:(…).strip call head=`(row.get(\"x\") || \"\").strip`",
            "  ↳ calls `(…).strip` → unknown unknown_receiver",
            "  /call:row.get call head=`row.get`",
            "    ↳ calls `row.get` → unknown unknown_receiver",
            "/call:Date new head=`Date`",
            "  ↳ calls `Date` → builtin ecmascript",
            "/call:sql call head=`sql`",
            "  ↳ calls `sql` → unknown unknown_name",
            "  /call:columns call head=`columns`",
            "    ↳ calls `columns` → unknown unknown_name",
            "/call:import call awaited binds=`module` head=`import`",
            "  ↳ calls `import` → unknown dynamic",
            "  ↳ imports `./lazy.js` → unknown not_found",
            "/call:super_.call call head=`super_.call`",
            "  ↳ calls `super_.call` → unknown unknown_receiver",
        ],
    );
}

#[test]
fn conditional_calls() {
    assert_tree_at(
        "a.ts",
        r#"const name = user && user.name();
const label = user?.label() ?? fallback();
config.retries ||= defaults();
maybe?.(arg());
a?.b.c(d());
x().y?.z();
"#,
        &[
            "/call:user.name call conditional head=`user.name`",
            "  ↳ calls `user.name` → unknown unknown_receiver",
            "/call:user.label call conditional head=`user?.label`",
            "  ↳ calls `user.label` → unknown unknown_receiver",
            "/call:fallback call conditional head=`fallback`",
            "  ↳ calls `fallback` → unknown unknown_name",
            "/call:defaults call conditional head=`defaults`",
            "  ↳ calls `defaults` → unknown unknown_name",
            "/call:maybe call conditional head=`maybe`",
            "  ↳ calls `maybe` → unknown unknown_name",
            "  /call:arg call conditional head=`arg`",
            "    ↳ calls `arg` → unknown unknown_name",
            "/call:a.b.c call conditional head=`a?.b.c`",
            "  ↳ calls `a.b.c` → unknown unknown_receiver",
            "  /call:d call conditional head=`d`",
            "    ↳ calls `d` → unknown unknown_name",
            "/call:x().y.z call conditional head=`x().y?.z`",
            "  ↳ calls `x().y.z` → unknown unknown_receiver",
            "  /call:x call head=`x`",
            "    ↳ calls `x` → unknown unknown_name",
        ],
    );
}

#[test]
fn code_is_not_nodes() {
    assert_tree_at(
        "a.ts",
        r#"import { helper } from "./helper";
import type { Shape } from "./shape";
let total = 0;
total += 1;
const shape: Shape = { kind: "square" };
export { helper };
type Alias = Shape;
"#,
        &[
            "Alias type_alias head=`type Alias`",
            "  ↳ types `Shape` → unknown not_found",
            "↳ imports `./helper` → unknown not_found",
            "↳ types `Shape` → unknown not_found",
        ],
    );
}

#[test]
fn jsx_is_code() {
    assert_tree_at(
        "a.tsx",
        r#"export function Page({ items }) {
  return (
    <List onSelect={(item) => select(item)}>
      {items.map((item) => <Row key={item.id} label={format(item)} />)}
    </List>
  );
}
"#,
        &[
            "Page function head=`function Page({ items })`",
            "  /return[0] return",
            "    /lambda[0] lambda head=`(item) =>`",
            "      /call:select call head=`select`",
            "        ↳ calls `select` → unknown unknown_name",
            "    /call:items.map call head=`items.map`",
            "      ↳ calls `items.map` → unknown unknown_receiver",
            "      /callback[0] lambda head=`(item) =>`",
            "        /call:format call head=`format`",
            "          ↳ calls `format` → unknown unknown_name",
        ],
    );
}

#[test]
fn binds_exit_values_and_statements() {
    assert_tree_at(
        "a.ts",
        r#"async function flow(order) {
  const receipt = await charge(order);
  order.receipt = receipt;
  order.total = sum(order.items);
  const { id, status } = await load(order.id);
  log(id, status), audit(id);
  return save(order);
}
"#,
        &[
            "flow function async head=`async function flow(order)`",
            "  /call:charge call awaited binds=`receipt` head=`charge`",
            "    ↳ calls `charge` → unknown unknown_name",
            "  /call:sum call binds=`order.total` head=`sum`",
            "    ↳ calls `sum` → unknown unknown_name",
            "  /call:load call awaited binds=`{ id, status }` head=`load`",
            "    ↳ calls `load` → unknown unknown_name",
            "  /call:log call head=`log`",
            "    ↳ calls `log` → unknown unknown_name",
            "  /call:audit call head=`audit`",
            "    ↳ calls `audit` → unknown unknown_name",
            "  /return[0] return",
            "    /call:save call exit_value head=`save`",
            "      ↳ calls `save` → unknown unknown_name",
        ],
    );
}

/// A chain of plain assignments binds all its written targets, as Python does; a target written
/// inside the value doesn't rebind it.
#[test]
fn chained_assignments_bind_every_target() {
    assert_tree_at(
        "a.ts",
        r#"async function flow() {
  a = b = first();
  const x = y = await second();
  total += third();
  cache ??= fourth();
}
"#,
        &[
            "flow function async head=`async function flow()`",
            "  /call:first call binds=`a = b` head=`first`",
            "    ↳ calls `first` → unknown unknown_name",
            "  /call:second call awaited binds=`x = y` head=`second`",
            "    ↳ calls `second` → unknown unknown_name",
            "  /call:third call head=`third`",
            "    ↳ calls `third` → unknown unknown_name",
            "  /call:fourth call conditional head=`fourth`",
            "    ↳ calls `fourth` → unknown unknown_name",
        ],
    );
}

/// `yield` is an exit only as its statement or the whole value its statement assigns; in a
/// conditional branch, a test, an argument, or an exit's value, its calls are code where they
/// are written, and the graph is valid.
#[test]
fn yield_is_an_exit_only_as_a_statement_or_an_assigned_value() {
    let source = r#"function* steps(c) {
  yield start();
  const v = c ? yield pick() : other();
  if (c ? yield probe() : fallback()) {}
  use(c ? yield : none);
  x = y = yield both();
  return yield last();
}
"#;
    assert_tree_at(
        "a.ts",
        source,
        &[
            "steps function generator head=`function* steps(c)`",
            "  /yield[0] yield",
            "    /call:start call exit_value head=`start`",
            "      ↳ calls `start` → unknown unknown_name",
            "  /ternary[0] ternary binds=`v`",
            "    /then then head=`c`",
            "      /call:pick call head=`pick`",
            "        ↳ calls `pick` → unknown unknown_name",
            "    /else else",
            "      /call:other call head=`other`",
            "        ↳ calls `other` → unknown unknown_name",
            "  /if[0] if",
            "    /then then head=`c ? yield probe() : fallback()`",
            "      /ternary[0] ternary",
            "        /then then head=`c`",
            "          /call:probe call head=`probe`",
            "            ↳ calls `probe` → unknown unknown_name",
            "        /else else",
            "          /call:fallback call head=`fallback`",
            "            ↳ calls `fallback` → unknown unknown_name",
            "  /call:use call head=`use`",
            "    ↳ calls `use` → unknown unknown_name",
            "  /yield[1] yield",
            "    /call:both call exit_value head=`both`",
            "      ↳ calls `both` → unknown unknown_name",
            "  /return[0] return",
            "    /call:last call head=`last`",
            "      ↳ calls `last` → unknown unknown_name",
        ],
    );
    let statements = statements_of(source);
    let stmt = |key: &str| {
        statements
            .iter()
            .find(|(own, _)| own == key)
            .and_then(|(_, stmt)| *stmt)
    };
    assert_eq!(stmt("a.ts#steps/yield[0]"), Some("yield start();"));
    assert_eq!(stmt("a.ts#steps/yield[1]"), Some("x = y = yield both();"));
}

#[test]
fn labeled_plain_blocks_are_truncated() {
    assert_tree_at(
        "a.ts",
        r#"function labeled() {
  done: {
    if (ready()) break done;
    work();
  }
  after();
}
"#,
        &[
            "labeled function head=`function labeled()`",
            "  /if[0] if",
            "    /then then truncated head=`ready()`",
            "      /call:ready call role=test head=`ready`",
            "        ↳ calls `ready` → unknown unknown_name",
            "  /call:work call head=`work`",
            "    ↳ calls `work` → unknown unknown_name",
            "  /call:after call head=`after`",
            "    ↳ calls `after` → unknown unknown_name",
        ],
    );
}

#[test]
fn an_early_break_leaves_its_switch() {
    assert_tree_at(
        "a.ts",
        r#"function pick(kind, item) {
  switch (kind) {
    case "a":
      if (item.skip) break;
      use(item);
      break;
    default:
      other(item);
  }
  after();
}
"#,
        &[
            "pick function head=`function pick(kind, item)`",
            "  /switch[0] switch head=`kind`",
            "    /case[0] case head=`\"a\"`",
            "      /if[0] if",
            "        /then then head=`item.skip`",
            "          /break[0] break target=pick/switch[0]",
            "      /call:use call head=`use`",
            "        ↳ calls `use` → unknown unknown_name",
            "    /default default",
            "      /call:other call head=`other`",
            "        ↳ calls `other` → unknown unknown_name",
            "  /call:after call head=`after`",
            "    ↳ calls `after` → unknown unknown_name",
        ],
    );
}

#[test]
fn a_labeled_break_leaves_an_enclosing_switch() {
    assert_tree_at(
        "a.ts",
        r#"function scan(kind, rows) {
  choose: switch (kind) {
    case "rows":
      for (const row of rows) {
        if (row.last) break choose;
        read(row);
      }
      finish();
  }
}
"#,
        &[
            "scan function head=`function scan(kind, rows)`",
            "  /switch[0] switch head=`kind`",
            "    /case[0] case head=`\"rows\"`",
            "      /for[0] for head=`for (const row of rows)`",
            "        /body body",
            "          /if[0] if",
            "            /then then head=`row.last`",
            "              /break[0] break target=scan/switch[0]",
            "          /call:read call head=`read`",
            "            ↳ calls `read` → unknown unknown_name",
            "      /call:finish call head=`finish`",
            "        ↳ calls `finish` → unknown unknown_name",
        ],
    );
}

// --- The worked examples ----------------------------------------------------------------------

/// `src/service.ts`: example 02, and the second half of the TypeScript route example.
const SERVICE: &str = r#"import { chargeCard, loadOrder, saveOrder } from "./store";

export async function placeOrder(id: string) {
  const order = await loadOrder(id);
  switch (order.status) {
    case "paid":
      return order;
    case "cancelled":
      throw new Error("cancelled");
  }
  order.receipt = await chargeCard(order.card, order.total);
  await saveOrder(order);
  return order;
}
"#;

/// `src/routes/orders.ts`: example 09, and the first half of the worked example.
const ROUTES: &str = r#"import { Router } from "express";
import { placeOrder } from "../service";

export const ordersRouter = Router();

ordersRouter.post("/orders/:id", async (req, res) => {
  try {
    const order = await placeOrder(req.params.id);
    res.json(order);
  } catch (error) {
    res.status(500).json({ error: String(error) });
  }
});
"#;

const STORE: &str = r#"export async function loadOrder(id: string) {
  return { id, status: "new", card: "", total: 0, receipt: "" };
}
export async function chargeCard(card: string, total: number) {
  return `${card}:${total}`;
}
export async function saveOrder(order: unknown) {}
"#;

/// The node trees of the examples' project, with their links.
fn example_trees() -> (Vec<String>, Vec<String>) {
    let project = crate::support::Project::new()
        .source("src/service.ts", SERVICE)
        .source("src/routes/orders.ts", ROUTES)
        .source("src/store.ts", STORE);
    let compiled = project.compile();
    let routes = crate::support::tree(compiled.file("src/routes/orders.ts"), ROUTES);
    let service = crate::support::tree(compiled.file("src/service.ts"), SERVICE);
    (routes, service)
}

/// The registration call is keyed by its literal path, so its callback's key (the entrance's
/// code) holds when another route is added above it. `String` and `Error` are ECMAScript's
/// standard objects: `builtin`.
#[test]
fn the_typescript_route() {
    let (routes, _) = example_trees();
    assert_eq!(
        routes,
        [
            "/call:Router call binds=`ordersRouter` head=`Router`",
            "  ↳ calls `Router` → library express",
            "/call:ordersRouter.post(\"/orders/:id\") call head=`ordersRouter.post`",
            "  ↳ calls `ordersRouter.post` → unknown unknown_receiver",
            "  /callback[0] lambda async head=`async (req, res) =>`",
            "    /try[0] try",
            "      /body body",
            "        /call:placeOrder call awaited binds=`order` head=`placeOrder`",
            "          ↳ calls `placeOrder` → project src/service.ts#placeOrder",
            "        /call:res.json call head=`res.json`",
            "          ↳ calls `res.json` → unknown unknown_receiver",
            "      /catch catch head=`catch (error)`",
            "        /call:res.status().json call head=`res.status(500).json`",
            "          ↳ calls `res.status().json` → unknown unknown_receiver",
            "          /call:res.status call head=`res.status`",
            "            ↳ calls `res.status` → unknown unknown_receiver",
            "          /call:String call head=`String`",
            "            ↳ calls `String` → builtin ecmascript",
            "↳ imports `express` → library express",
            "↳ imports `../service` → project src/service.ts#",
        ]
    );
    let handler = "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]";
    let added = ROUTES.replace(
        "ordersRouter.post(\"/orders/:id\"",
        "ordersRouter.post(\"/orders\", (req, res) => res.json([]));\n\nordersRouter.post(\"/orders/:id\"",
    );
    for source in [ROUTES, added.as_str()] {
        let compiled = compile_source("src/routes/orders.ts", source).valid();
        assert!(compiled.file("src/routes/orders.ts").local_id(handler).is_some());
    }
}

/// No case matching continues to `chargeCard`: the `switch` is an ordinary decision.
#[test]
fn place_order_is_an_ordinary_decision() {
    let (_, service) = example_trees();
    assert_eq!(
        service,
        [
            "placeOrder function async head=`async function placeOrder(id: string)`",
            "  /call:loadOrder call awaited binds=`order` head=`loadOrder`",
            "    ↳ calls `loadOrder` → project src/store.ts#loadOrder",
            "  /switch[0] switch head=`order.status`",
            "    /case[0] case head=`\"paid\"`",
            "      /return[0] return",
            "    /case[1] case head=`\"cancelled\"`",
            "      /throw[0] throw",
            "        /call:Error new exit_value head=`Error`",
            "          ↳ calls `Error` → builtin ecmascript",
            "  /call:chargeCard call awaited binds=`order.receipt` head=`chargeCard`",
            "    ↳ calls `chargeCard` → project src/store.ts#chargeCard",
            "  /call:saveOrder call awaited head=`saveOrder`",
            "    ↳ calls `saveOrder` → project src/store.ts#saveOrder",
            "  /return[0] return",
            "↳ imports `./store` → project src/store.ts#",
        ]
    );
}
