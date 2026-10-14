//! Lowering to nodes, checked as readable trees ([`crate::common::tree`]): one test per row of
//! diagram-blocks' Python mapping table, `place_order` exactly as the design's 21 blocks (and as
//! `unfold-core`'s sample graph), the worked examples of diagram-blocks-examples.html, and a
//! function shaped like `_capture`, each compiled, linked, and validated with the census.

use unfold_core::LinkTarget;
use unfold_core::testkit::sample;

use crate::common::{compile, compile_folder, testdata};

/// Asserts that `source`, compiled as `pkg/main.py`, lowers to `expected`.
#[track_caller]
fn lowers(source: &str, expected: &str) {
    let path = "pkg/main.py";
    let compiled = compile(&[(path, source)]);
    assert_eq!(compiled.tree(path).trim_end(), expected.trim_end(), "{source}");
}

/// The module: its docstring is `doc`; its own code sits under `#/`; its imports are links.
#[test]
fn module_docstring_and_top_level_code() {
    lowers(
        "\"\"\"The module's docstring.\"\"\"\n\nimport os\n\nsetup()\n",
        r#"
pkg/main.py# module doc [imports os → library python:os]
  call:setup call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// `def`, `async def`, `__init__`, a proven `@property` and its setter, methods, and nested functions; a function that yields is a generator.
#[test]
fn functions_in_every_form() {
    lowers(
        "class Account:\n    def __init__(self, owner):\n        self.owner = owner\n\n    @property\n    def balance(self):\n        return total(self)\n\n    @balance.setter\n    def balance(self, value):\n        store(value)\n\n    def close(self):\n        \"\"\"Close it.\"\"\"\n\n    async def sync(self):\n        await push(self)\n\n\ndef helper():\n    def inner():\n        pass\n    yield inner()\n\n\nasync def fetch():\n    return await get()\n",
        r#"
pkg/main.py# module
  Account class head=`class Account` stmt
    __init__ constructor head=`def __init__(self, owner)` stmt
    call:property call role=decorator for=balance [→ builtin]
    balance getter head=`def balance(self)` stmt=`@property`+node
      return[0] return stmt value=`total(self)`
        call:total call exit_value [→ unknown (unknown_name)]
    call:balance.setter call role=decorator for=balance [→ unknown (unknown_receiver)]
    balance[1] setter head=`def balance(self, value)` stmt=`@balance.setter`+node
      call:store call stmt [→ unknown (unknown_name)]
    close method head=`def close(self)` stmt doc
    sync method head=`async def sync(self)` stmt async
      call:push call stmt=`await push(self)` awaited [→ unknown (unknown_name)]
  helper function head=`def helper()` stmt generator
    inner function head=`def inner()` stmt
    yield[0] yield stmt value=`inner()`
      call:inner call exit_value [→ project pkg/main.py#helper.inner]
  fetch function head=`async def fetch()` stmt async
    return[0] return stmt value=`await get()`
      call:get call awaited exit_value [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A lambda is named by the simple assignment whose value it is, `callback[n]` in a call's arguments (the call keyed by a literal first argument), and `lambda[n]` anywhere else, including called where it is written.
#[test]
fn lambdas_are_named_callbacks_or_lambdas() {
    lowers(
        "handler = lambda event: process(event)\nrows.sort(key=lambda row: row.age)\nregister(\"/path\", lambda request: respond(request))\nfactory = make(lambda: build())\ncallbacks = [lambda: first(), lambda: second()]\n(lambda: run())()\n",
        r#"
pkg/main.py# module
  handler lambda head=`lambda event` stmt=`handler = lambda event: process(event)`
    call:process call [→ unknown (unknown_name)]
  call:rows.sort call stmt [→ unknown (unknown_receiver)]
    callback[0] lambda head=`lambda row`
  call:register("/path") call stmt [→ unknown (unknown_name)]
    callback[0] lambda head=`lambda request`
      call:respond call [→ unknown (unknown_name)]
  call:make call binds=factory stmt=`factory = make(lambda: build())` [→ unknown (unknown_name)]
    callback[0] lambda head=`lambda`
      call:build call [→ unknown (unknown_name)]
  lambda[0] lambda head=`lambda` stmt=`callbacks = [lambda: first(), lambda: second()]`
    call:first call [→ unknown (unknown_name)]
  lambda[1] lambda head=`lambda`
    call:second call [→ unknown (unknown_name)]
  call:(…) call stmt [→ project pkg/main.py#/call:(…)/lambda[0]]
    lambda[0] lambda head=`lambda`
      call:run call [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A base written as a name is an `extends` link; a call in a class's header is a `base` call in the enclosing body.
#[test]
fn class_bases_are_extends_links_or_base_calls() {
    lowers(
        "class Declined(Exception):\n    pass\n\n\nclass Record(namedtuple(\"Record\", \"a b\"), metaclass=make_meta()):\n    pass\n",
        r#"
pkg/main.py# module
  Declined class head=`class Declined(Exception)` stmt [extends Exception → builtin]
  call:namedtuple call role=base for=Record [→ unknown (unknown_name)]
  call:make_meta call role=base for=Record [→ unknown (unknown_name)]
  Record class head=`class Record(namedtuple("Record", "a b"), metaclass=make_meta())` stmt
"#
        .trim_start(),
    );
}

/// Decorators (plain names included), defaults, and annotations with calls are calls in the enclosing body, before the definition, naming it in `for`.
#[test]
fn decorators_and_defaults_run_in_the_enclosing_body() {
    lowers(
        "@dataclass\n@app.task(queue=\"q\")\nclass Job:\n    pass\n\n\n@retry(times=3)\ndef send(message, timeout=default_timeout(), *, tries: Annotated[int, Field(gt=0)] = 1) -> Result:\n    return message\n",
        r#"
pkg/main.py# module
  call:dataclass call role=decorator for=Job [→ unknown (unknown_name)]
  call:app.task call role=decorator for=Job [→ unknown (unknown_receiver)]
  Job class head=`class Job` stmt=`@dataclass @app.task(queue="q")`+node
  call:retry call role=decorator for=send [→ unknown (unknown_name)]
  call:default_timeout call role=default for=send [→ unknown (unknown_name)]
  call:Field call role=default for=send [→ unknown (unknown_name)]
  send function head=`def send(message, timeout=default_timeout(), *, tries: Annotated[int, Field(gt=0)] = 1) -> Result` stmt=`@retry(times=3)`+node [types Annotated → unknown (unknown_name)] [types int → builtin] [types Result → unknown (unknown_name)]
    return[0] return stmt value=`message`
"#
        .trim_start(),
    );
}

/// `if`/`elif`/`else` is one decision with ordered parts; each test is its part's head and its calls are test calls.
#[test]
fn if_elif_else_is_one_decision() {
    lowers(
        "def choose(order):\n    if order is None:\n        raise ValueError(\"missing\")\n    elif is_paid(order):\n        return order\n    elif order.held:\n        hold(order)\n    else:\n        charge(order)\n",
        r#"
pkg/main.py# module
  choose function head=`def choose(order)` stmt
    if[0] if
      then then head=`order is None`
        raise[0] raise stmt value=`ValueError("missing")`
          call:ValueError call exit_value [→ builtin]
      elif[0] elif head=`is_paid(order)`
        call:is_paid call role=test [→ unknown (unknown_name)]
        return[0] return stmt value=`order`
      elif[1] elif head=`order.held`
        call:hold call stmt [→ unknown (unknown_name)]
      else else
        call:charge call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// `match` is a decision headed by its subject alone; each case's pattern and guard are its head; the guardless `case _:` is `default`, with no head.
#[test]
fn match_is_a_decision_with_cases_and_default() {
    lowers(
        "def dispatch(command):\n    match parse(command):\n        case Move(x=x) if valid(x):\n            move(x)\n        case [\"quit\"]:\n            return None\n        case _:\n            unknown(command)\n",
        r#"
pkg/main.py# module
  dispatch function head=`def dispatch(command)` stmt
    match[0] match head=`parse(command)`
      call:parse call [→ unknown (unknown_name)]
      case[0] case head=`Move(x=x) if valid(x)`
        call:valid call role=test [→ unknown (unknown_name)]
        call:move call stmt [→ unknown (unknown_name)]
      case[1] case head=`["quit"]`
        return[0] return stmt value=`None`
      default default
        call:unknown call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A conditional expression with a call is a `ternary` decision; one without is code.
#[test]
fn conditional_expressions_with_calls_are_ternaries() {
    lowers(
        "def pick(flag):\n    value = left() if flag else right()\n    plain = 1 if flag else 2\n    return fast() if check() else slow()\n",
        r#"
pkg/main.py# module
  pick function head=`def pick(flag)` stmt
    ternary[0] ternary binds=value stmt=`value = left() if flag else right()`
      then then head=`flag`
        call:left call [→ unknown (unknown_name)]
      else else
        call:right call [→ unknown (unknown_name)]
    return[0] return stmt value=`fast() if check() else slow()`
      ternary[0] ternary exit_value
        then then head=`check()`
          call:check call role=test [→ unknown (unknown_name)]
          call:fast call [→ unknown (unknown_name)]
        else else
          call:slow call [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// `for`, `async for`, and `while` are loops with header calls, a `body`, and an `else`; `break` and `continue` name their loop.
#[test]
fn loops_and_their_else_parts() {
    lowers(
        "async def work(items):\n    for item in load(items):\n        if skip(item):\n            continue\n        handle(item)\n    else:\n        done()\n    async for event in stream():\n        consume(event)\n    while pending():\n        if stop():\n            break\n        step()\n    else:\n        finish()\n",
        r#"
pkg/main.py# module
  work function head=`async def work(items)` stmt async
    for[0] for head=`for item in load(items)`
      call:load call [→ unknown (unknown_name)]
      body body
        if[0] if
          then then head=`skip(item)`
            call:skip call role=test [→ unknown (unknown_name)]
            continue[0] continue stmt target=pkg/main.py#work/for[0]
        call:handle call stmt [→ unknown (unknown_name)]
      else else
        call:done call stmt [→ unknown (unknown_name)]
    for[1] for head=`async for event in stream()`
      call:stream call [→ unknown (unknown_name)]
      body body
        call:consume call stmt [→ unknown (unknown_name)]
    while[0] while head=`while pending()`
      call:pending call [→ unknown (unknown_name)]
      body body
        if[0] if
          then then head=`stop()`
            call:stop call role=test [→ unknown (unknown_name)]
            break[0] break stmt target=pkg/main.py#work/while[0]
        call:step call stmt [→ unknown (unknown_name)]
      else else
        call:finish call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A comprehension or generator with a call is a `comprehension` loop: `for` clauses are the header, `if` clauses the body's test, the element the body.
#[test]
fn comprehensions_with_calls_are_loops() {
    lowers(
        "def build(rows):\n    names = [label(row) for row in fetch(rows) if keep(row)]\n    pairs = {key(row): value(row) for row in rows}\n    total = sum(price(item) for item in rows)\n    plain = [row for row in rows if row]\n    return {convert(row) for row in rows}\n",
        r#"
pkg/main.py# module
  build function head=`def build(rows)` stmt
    comprehension[0] comprehension head=`for row in fetch(rows)` binds=names stmt=`names = [label(row) for row in fetch(rows) if keep(row)]`
      call:fetch call [→ unknown (unknown_name)]
      body body head=`keep(row)`
        call:keep call role=test [→ unknown (unknown_name)]
        call:label call [→ unknown (unknown_name)]
    comprehension[1] comprehension head=`for row in rows` binds=pairs stmt=`pairs = {key(row): value(row) for row in rows}`
      body body
        call:key call [→ unknown (unknown_name)]
        call:value call [→ unknown (unknown_name)]
    call:sum call binds=total stmt=`total = sum(price(item) for item in rows)` [→ builtin]
      comprehension[0] comprehension head=`for item in rows`
        body body
          call:price call [→ unknown (unknown_name)]
    return[0] return stmt value=`{convert(row) for row in rows}`
      comprehension[0] comprehension head=`for row in rows` exit_value
        body body
          call:convert call [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// The right side of `and` and `or` runs only sometimes, so every call written there is `conditional`, including the calls in the parts of a ternary or comprehension; a function's body runs when it is called, so its calls are not.
#[test]
fn short_circuits_make_calls_in_expression_blocks_conditional() {
    lowers(
        "def pick(x, rows):\n    a = x or (left() if test() else right())\n    b = x and [convert(r) for r in fetch(rows) if keep(r)]\n    c = x or (lambda: later())\n",
        r#"
pkg/main.py# module
  pick function head=`def pick(x, rows)` stmt
    ternary[0] ternary stmt=`a = x or (left() if test() else right())`
      then then head=`test()`
        call:test call role=test conditional [→ unknown (unknown_name)]
        call:left call conditional [→ unknown (unknown_name)]
      else else
        call:right call conditional [→ unknown (unknown_name)]
    comprehension[0] comprehension head=`for r in fetch(rows)` stmt=`b = x and [convert(r) for r in fetch(rows) if keep(r)]`
      call:fetch call conditional [→ unknown (unknown_name)]
      body body head=`keep(r)`
        call:keep call role=test conditional [→ unknown (unknown_name)]
        call:convert call conditional [→ unknown (unknown_name)]
    lambda[0] lambda head=`lambda` stmt=`c = x or (lambda: later())`
      call:later call [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A walrus binds its target and keeps what the expression around it says of the value: `return (y := f())` returns exactly `f`'s value.
#[test]
fn a_walrus_keeps_its_exit_value() {
    lowers(
        "def keep():\n    return (y := compute())\n",
        r#"
pkg/main.py# module
  keep function head=`def keep()` stmt
    return[0] return stmt value=`y := compute()`
      call:compute call binds=y exit_value [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// A comprehension's body part runs from its element to its last `if` test, the shortest range holding both: it never reaches a `for` clause written after the last test, and with no tests it is the element alone.
#[test]
fn a_comprehension_body_is_its_element_and_tests() {
    let path = "pkg/main.py";
    let source =
        "a = [f(x) for x in xs() if p(x) for y in ys(x)]\nb = {k(x): v(x) for x in xs() for y in ys(x)}\n";
    let compiled = compile(&[(path, source)]);
    let file = compiled.file(path);
    let text = |range: unfold_core::ByteRange| &source[range.as_usize()];
    let bodies: Vec<(&str, Option<&str>)> = file
        .nodes
        .iter()
        .filter(|node| node.form == unfold_core::vocab::Form::Body)
        .map(|node| (text(node.range), node.head.map(text)))
        .collect();
    assert_eq!(
        bodies,
        [("f(x) for x in xs() if p(x)", Some("p(x)")), ("k(x): v(x)", None)]
    );
}

/// `try` has a `body`, an `except[n]` per handler (the clause its head, with `types` links; `group` for `except*`), `else`, and `finally`.
#[test]
fn try_has_body_handlers_else_and_finally() {
    lowers(
        "def guarded():\n    try:\n        risky()\n    except (KeyError, errors.Missing) as error:\n        report(error)\n    except Exception:\n        raise\n    else:\n        succeed()\n    finally:\n        cleanup()\n    try:\n        group()\n    except* OSError:\n        handle_group()\n",
        r#"
pkg/main.py# module
  guarded function head=`def guarded()` stmt
    try[0] try
      body body
        call:risky call stmt [→ unknown (unknown_name)]
      except[0] except head=`except (KeyError, errors.Missing) as error` [types KeyError → builtin] [types errors.Missing → unknown (unknown_receiver)]
        call:report call stmt [→ unknown (unknown_name)]
      except[1] except head=`except Exception` [types Exception → builtin]
        raise[0] raise stmt
      else else
        call:succeed call stmt [→ unknown (unknown_name)]
      finally finally
        call:cleanup call stmt [→ unknown (unknown_name)]
    try[1] try
      body body
        call:group call stmt [→ unknown (unknown_name)]
      except[0] except head=`except* OSError` group [types OSError → builtin]
        call:handle_group call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// `with` and `async with` (flag `async`) have header calls and a `body`.
#[test]
fn with_and_async_with() {
    lowers(
        "async def scoped(path):\n    with open(path) as handle, lock():\n        handle.read()\n    async with session() as client:\n        await client.get()\n",
        r#"
pkg/main.py# module
  scoped function head=`async def scoped(path)` stmt async
    with[0] with head=`with open(path) as handle, lock()`
      call:open call [→ builtin]
      call:lock call [→ unknown (unknown_name)]
      body body
        call:handle.read call stmt [→ unknown (unknown_receiver)]
    with[1] with head=`async with session() as client` async
      call:session call [→ unknown (unknown_name)]
      body body
        call:client.get call stmt=`await client.get()` awaited [→ unknown (unknown_receiver)]
"#
        .trim_start(),
    );
}

/// `return`, `raise` (its value runs through its cause), `break`, `continue`, `yield`, and `yield from` are exits.
#[test]
fn exits_record_their_statement_value_and_target() {
    lowers(
        "def leave(items):\n    for item in items:\n        if item:\n            break\n        continue\n    if items:\n        return compute(items)\n    raise Problem(\"no\") from cause()\n\n\ndef produce():\n    yield first()\n    received = yield\n    yield from rest()\n",
        r#"
pkg/main.py# module
  leave function head=`def leave(items)` stmt
    for[0] for head=`for item in items`
      body body
        if[0] if
          then then head=`item`
            break[0] break stmt target=pkg/main.py#leave/for[0]
        continue[0] continue stmt target=pkg/main.py#leave/for[0]
    if[0] if
      then then head=`items`
        return[0] return stmt value=`compute(items)`
          call:compute call exit_value [→ unknown (unknown_name)]
    raise[0] raise stmt value=`Problem("no") from cause()`
      call:Problem call exit_value [→ unknown (unknown_name)]
      call:cause call [→ unknown (unknown_name)]
  produce function head=`def produce()` stmt generator
    yield[0] yield stmt value=`first()`
      call:first call exit_value [→ unknown (unknown_name)]
    yield[1] yield stmt=`received = yield`
    yield[2] yield stmt value=`rest()`
      call:rest call exit_value [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// Every call is a node, builtins and unresolved ones included, and calls in `assert` and `del`: with its normalized callee, `binds`, `conditional`, and the statement it begins.
#[test]
fn every_call_is_a_call_node() {
    lowers(
        "def calls(order):\n    print(order)\n    missing_name()\n    receipt = charge(order)\n    order.receipt = charge(order)\n    email_id, inserted = insert(order)\n    a = b = both()\n    total = base() + extra()\n    total += more()\n    fallback = cached() or fetched() and checked()\n    assert valid(order), explain(order)\n    del cache[key(order)]\n    res.status(500).json(body(order))\n    handlers[kind](order)\n    \", \".join(names(order))\n    (order.get(\"x\") or \"\").strip()\n    if (match := find(order)):\n        use(match)\n",
        r#"
pkg/main.py# module
  calls function head=`def calls(order)` stmt
    call:print call stmt [→ builtin]
    call:missing_name call stmt [→ unknown (unknown_name)]
    call:charge call binds=receipt stmt=`receipt = charge(order)` [→ unknown (unknown_name)]
    call:charge[1] call binds=order.receipt stmt=`order.receipt = charge(order)` [→ unknown (unknown_name)]
    call:insert call binds=email_id, inserted stmt=`email_id, inserted = insert(order)` [→ unknown (unknown_name)]
    call:both call binds=a = b stmt=`a = b = both()` [→ unknown (unknown_name)]
    call:base call stmt=`total = base() + extra()` [→ unknown (unknown_name)]
    call:extra call [→ unknown (unknown_name)]
    call:more call stmt=`total += more()` [→ unknown (unknown_name)]
    call:cached call stmt=`fallback = cached() or fetched() and checked()` [→ unknown (unknown_name)]
    call:fetched call conditional [→ unknown (unknown_name)]
    call:checked call conditional [→ unknown (unknown_name)]
    call:valid call stmt=`assert valid(order), explain(order)` [→ unknown (unknown_name)]
    call:explain call [→ unknown (unknown_name)]
    call:key call stmt=`del cache[key(order)]` [→ unknown (unknown_name)]
    call:res.status().json call stmt [→ unknown (unknown_receiver)]
      call:res.status call [→ unknown (unknown_receiver)]
      call:body call [→ unknown (unknown_name)]
    call:handlers%5B%5D call name=handlers[] stmt [→ unknown (dynamic)]
    call:"".join call stmt [→ builtin]
      call:names call [→ unknown (unknown_name)]
    call:(…).strip call stmt [→ unknown (unknown_receiver)]
      call:order.get call [→ unknown (unknown_receiver)]
    if[0] if
      then then head=`match := find(order)`
        call:find call binds=match role=test [→ unknown (unknown_name)]
        call:use call stmt [→ unknown (unknown_name)]
"#
        .trim_start(),
    );
}

/// Imports are links from the definition they are written in: the module, a function, or a class (none under a proven `TYPE_CHECKING`); `importlib.import_module("x")` is a call with an `imports` link.
#[test]
fn imports_are_links_from_the_module_or_function() {
    lowers(
        "import importlib\nimport os.path as path, json\nfrom . import sibling\nfrom .models import Order, __version__\nfrom typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    import erased\n\nloader = importlib.import_module(\"plugins.loader\")\n\n\ndef late():\n    import inside\n\n\nclass Holder:\n    import attribute\n",
        r#"
pkg/main.py# module [imports importlib → library python:importlib] [imports os.path → library python:os] [imports json → library python:json] [imports . → unknown (not_indexed)] [imports .models → unknown (not_found)] [imports typing → library python:typing]
  if[0] if
    then then head=`TYPE_CHECKING`
  call:importlib.import_module call binds=loader stmt=`loader = importlib.import_module("plugins.loader")` [→ library python:importlib] [imports plugins.loader → library plugins]
  late function head=`def late()` stmt [imports inside → library inside]
  Holder class head=`class Holder` stmt [imports attribute → library attribute]
"#
        .trim_start(),
    );
}

/// Assignments, augmented assignments, `pass`, `global`, `del`, and `assert` that call nothing are code: no nodes.
#[test]
fn statements_that_call_nothing_are_code() {
    lowers(
        "total = 0\ntotal += 1\nitems = []\nitems.append\npass\nglobal_value = None\n\n\ndef nothing():\n    global global_value\n    global_value = 2\n    x = [1, 2]\n    y = 1 if x else 2\n    del x\n    assert y\n",
        r#"
pkg/main.py# module
  nothing function head=`def nothing()` stmt
"#
        .trim_start(),
    );
}

/// The shop sample's Python files: their nodes are exactly `unfold-core`'s sample graph, which
/// builds `place_order` as diagram-blocks' "Proposed: 21 blocks" by hand.
fn shop() -> crate::common::Compiled {
    compile(&[
        ("shop/service.py", sample::SERVICE_PY),
        ("shop/payments.py", sample::PAYMENTS_PY),
        ("shop/store.py", sample::STORE_PY),
    ])
}

#[test]
fn place_order_is_the_designs_21_blocks() {
    let compiled = shop();
    let file = compiled.file("shop/service.py");
    let blocks = file
        .nodes
        .iter()
        .filter(|node| node.key.as_str().starts_with("shop/service.py#place_order"))
        .count();
    assert_eq!(blocks, 21);
    // `charge_card` is decorated with `@retry(3)`, a wrapper linking can't see through.
    let expected = r#"
place_order function head=`def place_order(order_id)` stmt doc
  call:load_order call binds=order stmt=`order = load_order(order_id)` [→ project shop/store.py#load_order]
  if[0] if
    then then head=`order is None`
      raise[0] raise stmt value=`ValueError("unknown order")`
        call:ValueError call exit_value [→ builtin]
    elif[0] elif head=`order["paid"]`
      return[0] return stmt value=`order`
  call:sum call binds=total stmt=`total = sum(item["price"] for item in order["items"])` [→ builtin]
  try[0] try
    body body
      call:charge_card call binds=receipt stmt=`receipt = charge_card(order["card"], total)` [→ unknown (wrapper)]
    except[0] except head=`except PaymentDeclined as error` [types PaymentDeclined → project shop/payments.py#PaymentDeclined]
      call:logger.warning call stmt [→ library python:logging]
      return[0] return stmt value=`mark_failed(order)`
        call:mark_failed call exit_value [→ project shop/store.py#mark_failed]
  call:save_order call stmt [→ project shop/store.py#save_order]
  for[0] for head=`for item in order["items"]`
    body body
      call:reserve_stock call stmt [→ project shop/store.py#reserve_stock]
  return[0] return stmt value=`order`
"#;
    // `place_order` is the module's last definition: its tree runs to the end.
    let printed = compiled.tree("shop/service.py");
    let (_, place_order) = printed.split_once("\n  place_order ").expect("place_order");
    let place_order: String = format!("place_order {place_order}")
        .lines()
        .map(|line| format!("{}\n", line.strip_prefix("  ").unwrap_or(line)))
        .collect();
    assert_eq!(place_order, expected.trim_start());
}

#[test]
fn the_shop_sample_lowers_to_unfold_cores_sample_graph() {
    let compiled = shop();
    let reference = sample::shop().build();
    for path in ["shop/service.py", "shop/payments.py", "shop/store.py"] {
        let lowered = compiled.file(path);
        let expected = reference.file(path).expect("the sample has the file");
        assert_eq!(lowered.nodes, expected.nodes, "{path}");
        assert_eq!(lowered.types.signatures, expected.types.signatures, "{path}");
        assert_eq!(lowered.types.parameters, expected.types.parameters, "{path}");
        // Links agree, but where the sample assumes more than linking proves: `charge_card` is
        // decorated with `@retry(3)` (a wrapper).
        let differing: Vec<String> = lowered
            .links
            .iter()
            .zip(&expected.links)
            .filter(|(lowered, expected)| lowered != expected)
            .map(|(lowered, _)| format!("{} {:?}", lowered.text, lowered.to))
            .collect();
        assert_eq!(lowered.links.len(), expected.links.len(), "{path}");
        let allowed: &[&str] = match path {
            "shop/service.py" => &["charge_card Unknown(Wrapper)"],
            _ => &[],
        };
        assert_eq!(differing, allowed, "{path}");
    }
}

/// The worked examples of diagram-blocks-examples.html, compiled with modules that define what
/// they call: each tree has the example's blocks, keys, fields, and outcomes.
#[test]
fn the_design_examples_lower_to_their_trees() {
    let compiled = compile_folder(&testdata().join("fixtures/python-lowering")).valid();
    let examples = [
        (
            "inbox/sync.py",
            r#"
inbox/sync.py# module [imports inbox.gmail → project inbox/gmail.py#] [imports inbox.locks → project inbox/locks.py#] [imports inbox.notify → project inbox/notify.py#] [imports inbox.retry → project inbox/retry.py#] [imports inbox.store → project inbox/store.py#]
  sync_inbox function head=`def sync_inbox(account)` stmt doc
    with[0] with head=`with mailbox_lock(account)`
      call:mailbox_lock call [→ project inbox/locks.py#mailbox_lock]
      body body
        try[0] try
          body body
            call:fetch_messages call binds=messages stmt=`messages = fetch_messages(account)` [→ project inbox/gmail.py#fetch_messages]
          except[0] except head=`except RateLimited as error` [types RateLimited → project inbox/gmail.py#RateLimited]
            call:schedule_retry call stmt [→ project inbox/retry.py#schedule_retry]
            return[0] return stmt value=`0`
          else else
            call:store_messages call stmt [→ project inbox/store.py#store_messages]
          finally finally
            call:record_attempt call stmt [→ project inbox/store.py#record_attempt]
    call:notify_synced call stmt [→ project inbox/notify.py#notify_synced]
    return[0] return stmt value=`len(messages)`
      call:len call exit_value [→ builtin]
"#,
        ),
        (
            "booking/slots.py",
            r#"
booking/slots.py# module [imports booking.calendar → project booking/calendar.py#] [imports booking.errors → project booking/errors.py#] [imports booking.notify → project booking/notify.py#]
  reserve_first_free function head=`def reserve_first_free(calendar, day)` stmt
    for[0] for head=`for slot in list_slots(calendar, day)`
      call:list_slots call [→ project booking/calendar.py#list_slots]
      body body
        if[0] if
          then then head=`slot.blocked`
            continue[0] continue stmt target=booking/slots.py#reserve_first_free/for[0]
        if[1] if
          then then head=`is_free(slot)`
            call:is_free call role=test [→ project booking/calendar.py#is_free]
            call:reserve call stmt [→ project booking/calendar.py#reserve]
            break[0] break stmt target=booking/slots.py#reserve_first_free/for[0]
      else else
        raise[0] raise stmt value=`NoFreeSlot(day)`
          call:NoFreeSlot new exit_value [→ project booking/errors.py#NoFreeSlot]
    call:notify_reserved call stmt [→ project booking/notify.py#notify_reserved]
"#,
        ),
        (
            "mail/route.py",
            r#"
mail/route.py# module [imports mail.classify → project mail/classify.py#] [imports mail.delivery → project mail/delivery.py#] [imports mail.labels → project mail/labels.py#] [imports mail.profiles → project mail/profiles.py#] [imports mail.routing → project mail/routing.py#]
  route_message function head=`def route_message(message, override)` stmt
    call:cached_profile call stmt=`profile = cached_profile(message.sender) or fetch_profile(message.sender)` [→ project mail/profiles.py#cached_profile]
    call:fetch_profile call conditional [→ project mail/profiles.py#fetch_profile]
    ternary[0] ternary binds=decision stmt=`decision = Routed(override) if override else classify(message, profile)`
      then then head=`override`
        call:Routed new [→ project mail/routing.py#Routed.__init__]
      else else
        call:classify call [→ project mail/classify.py#classify]
    comprehension[0] comprehension head=`for part in message.parts` binds=labels stmt=`labels = [label_for(part) for part in message.parts if is_relevant(part)]`
      body body head=`is_relevant(part)`
        call:is_relevant call role=test [→ project mail/classify.py#is_relevant]
        call:label_for call [→ project mail/labels.py#label_for]
    return[0] return stmt value=`deliver(decision, labels)`
      call:deliver call exit_value [→ project mail/delivery.py#deliver]
"#,
        ),
        (
            "shop/checkout.py",
            r#"
shop/checkout.py# module [imports json → library python:json] [imports logging → library python:logging] [imports shop.audit → project shop/audit.py#] [imports shop.format → project shop/format.py#] [imports shop.payments → project shop/payments.py#] [imports shop.receipts → project shop/receipts.py#] [imports shop.store → project shop/store.py#]
  call:logging.getLogger call binds=logger stmt=`logger = logging.getLogger(__name__)` [→ library python:logging]
  checkout function head=`def checkout(order)` stmt
    call:save_order call stmt [→ project shop/store.py#save_order]
      call:build_receipt call [→ project shop/receipts.py#build_receipt]
        call:charge_card call [→ project shop/payments.py#charge_card]
    for[0] for head=`for line in order.lines`
      body body
    call:logger.info call stmt [→ library python:logging]
      call:order_label call [→ project shop/format.py#order_label]
    call:audit.record call stmt [→ unknown (unknown_receiver)]
      call:json.dumps call [→ library python:json]
        call:summary call [→ project shop/format.py#summary]
"#,
        ),
        (
            "shop/cli.py",
            r#"
shop/cli.py# module [imports sys → library python:sys] [imports shop.service → project shop/service.py#]
  main function head=`def main()` stmt
    call:place_order call stmt [→ project shop/service.py#place_order]
  if[0] if
    then then head=`__name__ == "__main__"`
      call:main call stmt [→ project shop/cli.py#main]
"#,
        ),
        (
            "shop/api.py",
            r#"
shop/api.py# module [imports fastapi → library fastapi] [imports shop.auth → project shop/auth.py#] [imports shop.service → project shop/service.py#]
  call:FastAPI call binds=app stmt=`app = FastAPI()` [→ library fastapi]
  call:APIRouter call binds=router stmt=`router = APIRouter(prefix="/orders")` [→ library fastapi]
  call:router.post call role=decorator for=create_order [→ unknown (unknown_receiver)]
  call:Depends call role=default for=create_order [→ library fastapi] [passes current_user → project shop/auth.py#current_user]
  create_order function head=`def create_order(order_id: str, user=Depends(current_user))` stmt=`@router.post("/{order_id}")`+node [types str → builtin]
    return[0] return stmt value=`place_order(order_id)`
      call:place_order call exit_value [→ project shop/service.py#place_order]
  call:app.include_router call stmt [→ unknown (unknown_receiver)]
"#,
        ),
    ];
    for (path, expected) in examples {
        assert_eq!(compiled.tree(path), expected.trim_start(), "{path}");
    }
}

/// A function shaped like pathway-agents' `_capture` (diagram-blocks, "A real function"): two
/// `try` statements, a conditional expression with calls, and nested decisions.
#[test]
fn a_capture_shaped_function_lowers_to_its_tree() {
    let compiled = compile_folder(&testdata().join("fixtures/python-lowering")).valid();
    let expected = r#"
pipeline/capture.py# module doc [imports logging → library python:logging] [imports pipeline.errors → project pipeline/errors.py#] [imports pipeline.fetch → project pipeline/fetch.py#] [imports pipeline.routing → project pipeline/routing.py#] [imports pipeline.status → project pipeline/status.py#] [imports pipeline.store → project pipeline/store.py#]
  call:logging.getLogger call binds=logger stmt=`logger = logging.getLogger(__name__)` [→ library python:logging]
  bind_context function head=`def bind_context(decision)` stmt
  routing_text function head=`def routing_text(decision)` stmt
    return[0] return stmt value=`str(decision)`
      call:str call exit_value [→ builtin]
  capture function head=`def capture(run, message_id, override)` stmt doc
    try[0] try
      body body
        call:execute call binds=raw stmt=`raw = execute(run.service.users().messages().get(id=message_id))` [→ project pipeline/fetch.py#execute]
          call:run.service.users().messages().get call [→ unknown (unknown_receiver)]
            call:run.service.users().messages call [→ unknown (unknown_receiver)]
              call:run.service.users call [→ unknown (unknown_receiver)]
        call:read_message call binds=message stmt=`message = read_message(raw)` [→ project pipeline/fetch.py#read_message]
      except[0] except head=`except Exception as exc` [types Exception → builtin]
        call:run.failures.append call stmt [→ unknown (unknown_receiver)]
          call:str call [→ builtin]
        call:logger.exception call stmt [→ library python:logging]
        return[0] return stmt value=`None`
    call:logger.info call stmt [→ library python:logging]
      call:message.headers.get call [→ unknown (unknown_receiver)]
    ternary[0] ternary binds=decision stmt=`decision = Routed(override) if override else route(message)`
      then then head=`override`
        call:Routed new [→ project pipeline/routing.py#Routed.__init__]
      else else
        call:route call [→ project pipeline/routing.py#route]
    if[0] if
      then then head=`isinstance(decision, Routed)`
        call:isinstance call role=test [→ builtin] [passes Routed → project pipeline/routing.py#Routed]
        call:bind_context call stmt [→ project pipeline/capture.py#bind_context]
    try[1] try
      body body
        call:store_files call binds=names stmt=`names = store_files(run, message)` [→ project pipeline/store.py#store_files]
        call:outcome_columns call binds=outcome stmt=`outcome = outcome_columns(decision)` [→ project pipeline/store.py#outcome_columns]
      except[0] except head=`except CaptureError` [types CaptureError → project pipeline/errors.py#CaptureError]
        call:capture_failed call stmt [→ project pipeline/status.py#capture_failed]
          call:routing_text call [→ project pipeline/capture.py#routing_text]
        raise[0] raise stmt
    call:row_for call binds=row stmt=`row = row_for(message, outcome, names)` [→ project pipeline/store.py#row_for]
    if[1] if
      then then head=`run.exact and row["status"] in ("stored", "skipped")`
        call:run.errors.append call stmt [→ unknown (unknown_receiver)]
    if[2] if
      then then head=`not run.write`
        call:run.record call stmt [→ unknown (unknown_receiver)]
        return[0] return stmt value=`row`
    if[3] if
      then then head=`(stored := run.lookup(message_id)) is None`
        call:run.lookup call binds=stored role=test [→ unknown (unknown_receiver)]
        call:insert_email call binds=row_id, inserted stmt=`row_id, inserted = insert_email(row)` [→ project pipeline/store.py#insert_email]
        if[0] if
          then then head=`not inserted and row_id is not None`
            call:logger.info call stmt [→ library python:logging]
            return[0] return stmt value=`row`
        if[1] if
          then then head=`row_id is not None`
            call:logger.info call stmt [→ library python:logging]
          else else
            call:logger.warning call stmt [→ library python:logging]
      else else
        if[0] if
          then then head=`not update_expecting(stored, row)`
            call:update_expecting call role=test [→ project pipeline/store.py#update_expecting]
            call:logger.info call stmt [→ library python:logging]
            return[0] return stmt value=`row`
    call:run.record call stmt [→ unknown (unknown_receiver)]
    return[0] return stmt value=`row`
"#;
    assert_eq!(compiled.tree("pipeline/capture.py"), expected.trim_start());
}

/// An import written directly in a class body links from the class, and the census lists it.
#[test]
fn class_body_imports_link_from_the_class() {
    lowers(
        "class Codec:\n    import json\n    from base64 import b64encode\n\n    def encode(self, value):\n        return json.dumps(value)\n",
        r#"
pkg/main.py# module
  Codec class head=`class Codec` stmt [imports json → library python:json] [imports base64 → library python:base64]
    encode method head=`def encode(self, value)` stmt
      return[0] return stmt value=`json.dumps(value)`
        call:json.dumps call exit_value [→ unknown (unknown_receiver)]
"#
        .trim_start(),
    );
}

/// `@property` makes a getter only when it is proven to be the builtin.
#[test]
fn property_needs_proven_identity() {
    let compiled = compile(&[(
        "main.py",
        "def property(fn): return fn\nclass Decoy:\n    @property\n    def value(self): return 1\nimport builtins\nclass Real:\n    @builtins.property\n    def value(self): return 1\n",
    )]);
    assert_eq!(
        compiled.node("main.py#Decoy.value").form,
        unfold_core::vocab::Form::Method
    );
    assert_eq!(
        compiled.node("main.py#Real.value").form,
        unfold_core::vocab::Form::Getter
    );
}

/// Names and callees keep their written spelling, which the binder reads as NFKC.
#[test]
fn names_keep_their_written_spelling() {
    let compiled = compile(&[(
        "main.py",
        "ｃｏｕｎｔ = κόστος(1)\ndef 𐐀receipt(value): return κόστος(value)\n",
    )]);
    let keys: Vec<&str> = compiled
        .file("main.py")
        .nodes
        .iter()
        .map(|node| node.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "main.py#",
            "main.py#/call:κόστος",
            "main.py#𐐀receipt",
            "main.py#𐐀receipt/return[0]",
            "main.py#𐐀receipt/return[0]/call:κόστος",
        ]
    );
    assert_eq!(
        compiled.node("main.py#/call:κόστος").binds.as_deref(),
        Some("ｃｏｕｎｔ")
    );
}

/// Ranges are UTF-8 bytes over CRLF line ends and non-ASCII text, and a decorated definition's
/// range starts at its own keyword, after the decorators that run before it.
#[test]
fn the_range_corpus_keeps_unicode_and_crlf_ranges() {
    let text = std::fs::read_to_string(testdata().join("fixtures/python-ranges/fixtures/unicode.py"))
        .expect("the corpus");
    let compiled = compile(&[("main.py", &text)]);
    let decorated = compiled.node("main.py#decorated");
    assert!(text[decorated.range.as_usize()].starts_with("def decorated():"));
    let decorator = compiled.node("main.py#/call:decorator");
    assert_eq!(decorator.role, Some(unfold_core::vocab::CallRole::Decorator));
    assert_eq!(&text[decorator.range.as_usize()], "decorator(𐐀receipt)");
    let cost = compiled.node("main.py#κόστος");
    assert_eq!(
        &text[cost.range.as_usize()],
        "def κόστος(ποσό):\r\n    return ποσό"
    );
}

/// A `from` import's module and each named child import are written at their specifiers, the
/// child's text normalized like the module's (no whitespace or line continuations).
#[test]
fn child_import_specifiers_are_normalized() {
    let compiled = compile(&[
        ("m.py", "from pkg . sub import child\n"),
        ("pkg/__init__.py", ""),
        ("pkg/sub/__init__.py", ""),
        ("pkg/sub/child.py", ""),
    ]);
    let texts: Vec<&str> = compiled
        .file("m.py")
        .links
        .iter()
        .map(|link| &*link.text)
        .collect();
    assert_eq!(texts, ["pkg.sub", "pkg.sub.child"]);
}

/// A guard erases its imports only when it is proven to be `typing.TYPE_CHECKING`; a same-file
/// write to the identity invalidates the proof.
#[test]
fn type_checking_needs_proven_identity() {
    let imports = |source: &str| -> Vec<String> {
        compile(&[("main.py", source)])
            .file("main.py")
            .links
            .iter()
            .map(|link| link.text.to_string())
            .collect()
    };
    assert_eq!(
        imports(
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    import erased\nelse:\n    import kept\n"
        ),
        ["typing", "kept"]
    );
    assert_eq!(
        imports("import typing\ntyping.TYPE_CHECKING = True\nif typing.TYPE_CHECKING:\n    import kept\n"),
        ["typing", "kept"]
    );
    assert_eq!(
        imports("TYPE_CHECKING = False\nif TYPE_CHECKING:\n    import kept\n"),
        ["kept"]
    );
}

/// `importlib.import_module` with a literal name is a call with an `imports` link written at
/// the literal; anything unproven is an ordinary call.
#[test]
fn literal_import_module_calls_import_their_module() {
    let compiled = compile(&[(
        "main.py",
        "import importlib\nfrom importlib import import_module\nimport_module(\"a.b\")\nimportlib.import_module(name=\"c\")\nimportlib.import_module(package='p', name='x')\nimportlib.import_module('.x', 'p')\nimportlib.import_module(**options)\nimportlib.import_module(f'x')\n",
    )]);
    let file = compiled.file("main.py");
    let text = compiled.text("main.py");
    let dynamic: Vec<(&str, &str)> = file
        .links
        .iter()
        .filter(|link| link.from != 0)
        .filter(|link| link.kind == unfold_core::vocab::LinkKind::Imports)
        .map(|link| {
            (
                &file.nodes[link.from as usize].key.as_str()[9..],
                &text[link.range.as_usize()],
            )
        })
        .collect();
    assert_eq!(
        dynamic,
        [
            ("call:import_module", "\"a.b\""),
            ("call:importlib.import_module", "\"c\"")
        ]
    );
    let calls = file
        .nodes
        .iter()
        .filter(|node| node.kind() == unfold_core::vocab::NodeKind::Call)
        .count();
    assert_eq!(calls, 6, "every call is a node, an import or not");
}

/// The value table: a callee and its receivers, and the arguments that may pass a definition,
/// with written labels and the ranges their links are written at.
#[test]
fn values_keep_written_labels_and_link_ranges() {
    use unfold_engine::facts::ValueShape;
    let text = "service.repo.save(order, *rest, key=lambda o: o.id, **extra, flag=options.flag)\n";
    let snapshot =
        unfold_core::Snapshot::from_text(unfold_core::RepoPath::new("main.py").expect("a path"), text)
            .expect("text");
    let facts = unfold_lang_python::extract(&snapshot, unfold_engine::settings::PythonVersion::V3_14)
        .expect("extracts");
    let values = facts.graph.values();
    let call = &facts.graph.calls()[0];
    let callee = &values[call.callee as usize];
    assert_eq!(&*callee.label, "service.repo.save");
    assert_eq!(callee.shape, ValueShape::Member);
    assert_eq!(callee.member.as_deref(), Some("save"));
    assert_eq!(&text[callee.range.as_usize()], "service.repo.save");
    let receiver = &values[callee.receiver.expect("a receiver") as usize];
    assert_eq!(&*receiver.label, "service.repo");
    let arguments: Vec<(&str, Option<&str>)> = call
        .arguments
        .iter()
        .map(|argument| {
            (
                &text[argument.range.as_usize()],
                argument.value.map(|value| &*values[value as usize].label),
            )
        })
        .collect();
    assert_eq!(
        arguments,
        [
            ("order", Some("order")),
            ("*rest", None),
            ("key=lambda o: o.id", None),
            ("**extra", None),
            ("flag=options.flag", Some("options.flag")),
        ]
    );
}

/// A truncated block keeps its parts, empty, and everything below the depth limit is left
/// out of the graph and the census alike.
#[test]
fn nesting_past_the_depth_limit_is_truncated() {
    let depth = 300;
    let source = format!("x = {}1{}\n", "f(".repeat(depth), ")".repeat(depth));
    let compiled = compile(&[("main.py", &source)]);
    let file = compiled.file("main.py");
    let truncated: Vec<&unfold_core::Node> = file
        .nodes
        .iter()
        .filter(|node| node.flags.contains(unfold_core::vocab::Flag::Truncated))
        .collect();
    assert_eq!(truncated.len(), 1);
    assert_eq!(file.nodes.len(), 257, "the module and 256 nested calls");
    assert!(
        file.nodes[256]
            .flags
            .contains(unfold_core::vocab::Flag::Truncated)
    );
    let mut ifs = String::new();
    for level in 0..200 {
        ifs.push_str(&"    ".repeat(level));
        ifs.push_str("if f():\n");
    }
    ifs.push_str(&"    ".repeat(200));
    ifs.push_str("g()\n");
    let compiled = compile(&[("main.py", &ifs)]);
    let file = compiled.file("main.py");
    let last = file.nodes.last().expect("nodes");
    assert!(last.flags.contains(unfold_core::vocab::Flag::Truncated));
    assert_eq!(
        last.form,
        unfold_core::vocab::Form::Then,
        "a truncated decision keeps its part"
    );
    assert!(
        file.links
            .iter()
            .all(|link| !matches!(link.to, LinkTarget::Project(_)))
    );
}
