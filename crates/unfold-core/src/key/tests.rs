use super::*;
use Form as F;

/// One file's keys, pushed in preorder; each method returns the new node's local ID.
struct File(KeyBuilder);

impl File {
    fn new(path: &str) -> Self {
        Self(KeyBuilder::new(&RepoPath::new(path).unwrap()))
    }

    fn push(&mut self, parent: u32, child: Child<'_>) -> u32 {
        self.0.push(parent, child).unwrap();
        u32::try_from(self.0.keys().len() - 1).unwrap()
    }

    fn def(&mut self, parent: u32, name: &str) -> u32 {
        self.push(parent, Child::Definition(name))
    }

    fn form(&mut self, parent: u32, form: Form) -> u32 {
        self.push(parent, Child::Form(form))
    }

    fn call(&mut self, parent: u32, callee: &Callee) -> u32 {
        self.push(
            parent,
            Child::Call {
                callee,
                first_literal: None,
                takes_callback: false,
            },
        )
    }

    /// A call whose first argument is the string `literal` and which takes a callback.
    fn route(&mut self, parent: u32, callee: &Callee, literal: &str) -> u32 {
        self.push(
            parent,
            Child::Call {
                callee,
                first_literal: Some(literal),
                takes_callback: true,
            },
        )
    }

    fn inline(&mut self, parent: u32, inline: Inline) -> u32 {
        self.push(parent, Child::Inline(inline))
    }

    fn key(&self, id: u32) -> &str {
        self.0.keys()[id as usize].as_str()
    }
}

fn name(text: &str) -> Callee {
    Callee::name(text)
}

/// The tree of diagram-blocks' "Proposed: 21 blocks", keyed exactly as the design writes it.
#[test]
fn place_order_keys_match_the_design() {
    let mut f = File::new("shop/service.py");
    let place_order = f.def(0, "place_order");
    f.call(place_order, &name("load_order"));
    let decision = f.form(place_order, F::If);
    let then = f.form(decision, F::Then);
    let raise = f.form(then, F::Raise);
    f.call(raise, &name("ValueError"));
    let elif = f.form(decision, F::Elif);
    f.form(elif, F::Return);
    f.call(place_order, &name("sum"));
    let attempt = f.form(place_order, F::Try);
    let body = f.form(attempt, F::Body);
    f.call(body, &name("charge_card"));
    let except = f.form(attempt, F::Except);
    f.call(except, &name("logger").member("warning"));
    let early = f.form(except, F::Return);
    f.call(early, &name("mark_failed"));
    f.call(place_order, &name("save_order"));
    let lp = f.form(place_order, F::For);
    let lp_body = f.form(lp, F::Body);
    f.call(lp_body, &name("reserve_stock"));
    f.form(place_order, F::Return);

    let prefix = "shop/service.py#";
    let keys: Vec<&str> =
        f.0.keys()
            .iter()
            .map(|key| &key.as_str()[prefix.len()..])
            .collect();
    assert_eq!(
        keys,
        [
            "",
            "place_order",
            "place_order/call:load_order",
            "place_order/if[0]",
            "place_order/if[0]/then",
            "place_order/if[0]/then/raise[0]",
            "place_order/if[0]/then/raise[0]/call:ValueError",
            "place_order/if[0]/elif[0]",
            "place_order/if[0]/elif[0]/return[0]",
            "place_order/call:sum",
            "place_order/try[0]",
            "place_order/try[0]/body",
            "place_order/try[0]/body/call:charge_card",
            "place_order/try[0]/except[0]",
            "place_order/try[0]/except[0]/call:logger.warning",
            "place_order/try[0]/except[0]/return[0]",
            "place_order/try[0]/except[0]/return[0]/call:mark_failed",
            "place_order/call:save_order",
            "place_order/for[0]",
            "place_order/for[0]/body",
            "place_order/for[0]/body/call:reserve_stock",
            "place_order/return[0]",
        ]
    );
}

#[test]
fn a_modules_own_steps_follow_hash_slash() {
    // Example 07: `if __name__ == "__main__": main()`.
    let mut f = File::new("shop/cli.py");
    let main = f.def(0, "main");
    f.call(main, &name("place_order"));
    let decision = f.form(0, F::If);
    let then = f.form(decision, F::Then);
    let call = f.call(then, &name("main"));
    assert_eq!(f.key(0), "shop/cli.py#");
    assert_eq!(f.key(main), "shop/cli.py#main");
    assert_eq!(f.key(call), "shop/cli.py#/if[0]/then/call:main");
}

#[test]
fn a_function_named_match_is_not_a_match_statement() {
    let mut f = File::new("a.py");
    let function = f.def(0, "match");
    let statement = f.form(0, F::Match);
    let case = f.form(statement, F::Case);
    let default = f.form(statement, F::Default);
    let inner = f.form(function, F::Match);
    assert_eq!(f.key(function), "a.py#match");
    assert_eq!(f.key(statement), "a.py#/match[0]");
    assert_eq!(f.key(case), "a.py#/match[0]/case[0]");
    assert_eq!(f.key(default), "a.py#/match[0]/default");
    assert_eq!(f.key(inner), "a.py#match/match[0]");
    let Ok(ParsedKey::Block(parsed)) = Key::from_text("a.py#match/match[0]").parse() else {
        panic!("a block key");
    };
    assert_eq!(
        parsed.segments(),
        [
            Segment::Name {
                name: "match".into(),
                repeat: None
            },
            Segment::Form {
                form: F::Match,
                ordinal: Some(0)
            },
        ]
    );
}

#[test]
fn named_definitions_are_qualified_by_their_enclosing_definition() {
    let mut f = File::new("src/repo.ts");
    let class = f.def(0, "Repo");
    let save = f.def(class, "save");
    let handler = f.def(0, "handler");
    let property = f.def(0, "handlers.onCreate");
    let default = f.def(0, "default");
    let outer = f.def(0, "place_order");
    let guard = f.form(outer, F::If);
    let then = f.form(guard, F::Then);
    // A definition inside a part is still keyed by its name, under its enclosing definition.
    let helper = f.def(then, "helper");
    let nested = f.def(helper, "inner");
    let step = f.call(nested, &name("go"));
    assert_eq!(f.key(class), "src/repo.ts#Repo");
    assert_eq!(f.key(save), "src/repo.ts#Repo.save");
    assert_eq!(f.key(handler), "src/repo.ts#handler");
    assert_eq!(f.key(property), "src/repo.ts#handlers.onCreate");
    assert_eq!(f.key(default), "src/repo.ts#default");
    assert_eq!(f.key(helper), "src/repo.ts#place_order.helper");
    assert_eq!(f.key(nested), "src/repo.ts#place_order.helper.inner");
    assert_eq!(f.key(step), "src/repo.ts#place_order.helper.inner/call:go");
}

#[test]
fn a_repeated_name_gets_an_ordinal_wherever_it_sits() {
    // `if TYPE_CHECKING: def f(): … else: def f(): …`, then a third `f` at the top level.
    let mut f = File::new("a.py");
    let guard = f.form(0, F::If);
    let then = f.form(guard, F::Then);
    let first = f.def(then, "f");
    let otherwise = f.form(guard, F::Else);
    let second = f.def(otherwise, "f");
    let third = f.def(0, "f");
    let inside = f.call(second, &name("g"));
    let class = f.def(0, "C");
    let method = f.def(class, "f");
    assert_eq!(f.key(first), "a.py#f");
    assert_eq!(f.key(second), "a.py#f[1]");
    assert_eq!(f.key(third), "a.py#f[2]");
    assert_eq!(f.key(inside), "a.py#f[1]/call:g");
    assert_eq!(
        f.key(method),
        "a.py#C.f",
        "a name repeats only under the same definition"
    );
}

#[test]
fn control_blocks_and_exits_count_siblings_of_the_same_form() {
    let mut f = File::new("a.py");
    let function = f.def(0, "f");
    let first_if = f.form(function, F::If);
    let ternary = f.form(function, F::Ternary);
    let second_if = f.form(function, F::If);
    let ret = f.form(function, F::Return);
    let raise = f.form(function, F::Raise);
    let second_ret = f.form(function, F::Return);
    let then = f.form(second_if, F::Then);
    let nested = f.form(then, F::If);
    let elif0 = f.form(second_if, F::Elif);
    let elif1 = f.form(second_if, F::Elif);
    let otherwise = f.form(second_if, F::Else);
    let keys: Vec<&str> = [
        first_if, ternary, second_if, ret, raise, second_ret, nested, elif0, elif1, otherwise,
    ]
    .iter()
    .map(|&id| f.key(id))
    .collect();
    assert_eq!(
        keys,
        [
            "a.py#f/if[0]",
            "a.py#f/ternary[0]",
            "a.py#f/if[1]",
            "a.py#f/return[0]",
            "a.py#f/raise[0]",
            "a.py#f/return[1]",
            "a.py#f/if[1]/then/if[0]",
            "a.py#f/if[1]/elif[0]",
            "a.py#f/if[1]/elif[1]",
            "a.py#f/if[1]/else",
        ]
    );
}

#[test]
fn every_part_is_keyed_by_its_form() {
    // Example 03: a with block around a try with every part.
    let mut f = File::new("inbox/sync.py");
    let sync = f.def(0, "sync_inbox");
    let with = f.form(sync, F::With);
    let lock = f.call(with, &name("mailbox_lock"));
    let with_body = f.form(with, F::Body);
    let attempt = f.form(with_body, F::Try);
    let body = f.form(attempt, F::Body);
    let except = f.form(attempt, F::Except);
    let otherwise = f.form(attempt, F::Else);
    let finally = f.form(attempt, F::Finally);
    let record = f.call(finally, &name("record_attempt"));
    let ret = f.form(sync, F::Return);
    let len = f.call(ret, &name("len"));
    let prefix = "inbox/sync.py#sync_inbox/";
    let keys: Vec<&str> = [lock, body, except, otherwise, finally, record, len]
        .iter()
        .map(|&id| &f.key(id)[prefix.len()..])
        .collect();
    assert_eq!(
        keys,
        [
            "with[0]/call:mailbox_lock",
            "with[0]/body/try[0]/body",
            "with[0]/body/try[0]/except[0]",
            "with[0]/body/try[0]/else",
            "with[0]/body/try[0]/finally",
            "with[0]/body/try[0]/finally/call:record_attempt",
            "return[0]/call:len",
        ]
    );

    // TypeScript parts and forms: `switch` with `case[n]` and `default`, `catch`, `using`.
    let mut ts = File::new("src/a.ts");
    let function = ts.def(0, "f");
    let switch = ts.form(function, F::Switch);
    let case = ts.form(switch, F::Case);
    let second = ts.form(switch, F::Case);
    let default = ts.form(switch, F::Default);
    let throw = ts.form(second, F::Throw);
    let error = ts.push(
        throw,
        Child::Call {
            callee: &name("Error"),
            first_literal: Some("cancelled"),
            takes_callback: false,
        },
    );
    let attempt = ts.form(function, F::Try);
    let catch = ts.form(attempt, F::Catch);
    let using = ts.form(function, F::Using);
    let loop_do = ts.form(function, F::Do);
    let brk = ts.form(case, F::Break);
    assert_eq!(ts.key(case), "src/a.ts#f/switch[0]/case[0]");
    assert_eq!(ts.key(default), "src/a.ts#f/switch[0]/default");
    assert_eq!(ts.key(error), "src/a.ts#f/switch[0]/case[1]/throw[0]/call:Error");
    assert_eq!(ts.key(catch), "src/a.ts#f/try[0]/catch");
    assert_eq!(ts.key(using), "src/a.ts#f/using[0]");
    assert_eq!(ts.key(loop_do), "src/a.ts#f/do[0]");
    assert_eq!(ts.key(brk), "src/a.ts#f/switch[0]/case[0]/break[0]");
}

#[test]
fn expression_blocks_and_loop_parts() {
    // Examples 04 and 05: a loop's `else`, a ternary's parts, a comprehension's body.
    let mut f = File::new("mail/route.py");
    let route = f.def(0, "route_message");
    let ternary = f.form(route, F::Ternary);
    let then = f.form(ternary, F::Then);
    let routed = f.call(then, &name("Routed"));
    let comprehension = f.form(route, F::Comprehension);
    let body = f.form(comprehension, F::Body);
    let test = f.call(body, &name("is_relevant"));
    let lp = f.form(route, F::For);
    let lp_else = f.form(lp, F::Else);
    let raise = f.form(lp_else, F::Raise);
    let lp_body = f.form(lp, F::Body);
    let cont = f.form(lp_body, F::Continue);
    let yld = f.form(route, F::Yield);
    assert_eq!(
        f.key(routed),
        "mail/route.py#route_message/ternary[0]/then/call:Routed"
    );
    assert_eq!(
        f.key(test),
        "mail/route.py#route_message/comprehension[0]/body/call:is_relevant"
    );
    assert_eq!(f.key(raise), "mail/route.py#route_message/for[0]/else/raise[0]");
    assert_eq!(f.key(cont), "mail/route.py#route_message/for[0]/body/continue[0]");
    assert_eq!(f.key(yld), "mail/route.py#route_message/yield[0]");
}

#[test]
fn calls_repeat_only_in_the_same_parent() {
    let mut f = File::new("collector.py");
    let capture = f.def(0, "_capture");
    let info = name("logger").member("info");
    let first = f.call(capture, &info);
    let second = f.call(capture, &info);
    let other = f.call(capture, &name("logger").member("warning"));
    let third = f.call(capture, &info);
    let guard = f.form(capture, F::If);
    let then = f.form(guard, F::Then);
    let nested = f.call(then, &info);
    let inner = f.call(first, &name("message").member("headers").member("get"));
    let inner_again = f.call(first, &name("message").member("headers").member("get"));
    assert_eq!(f.key(first), "collector.py#_capture/call:logger.info");
    assert_eq!(f.key(second), "collector.py#_capture/call:logger.info[1]");
    assert_eq!(f.key(other), "collector.py#_capture/call:logger.warning");
    assert_eq!(f.key(third), "collector.py#_capture/call:logger.info[2]");
    assert_eq!(f.key(nested), "collector.py#_capture/if[0]/then/call:logger.info");
    assert_eq!(
        f.key(inner),
        "collector.py#_capture/call:logger.info/call:message.headers.get"
    );
    assert_eq!(
        f.key(inner_again),
        "collector.py#_capture/call:logger.info/call:message.headers.get[1]"
    );
}

#[test]
fn calls_contain_the_calls_in_their_receiver_and_arguments() {
    // Example 06: `save_order(build_receipt(order, charge_card(order.card)))`.
    let mut f = File::new("shop/checkout.py");
    let checkout = f.def(0, "checkout");
    let save = f.call(checkout, &name("save_order"));
    let build = f.call(save, &name("build_receipt"));
    let charge = f.call(build, &name("charge_card"));
    assert_eq!(
        f.key(charge),
        "shop/checkout.py#checkout/call:save_order/call:build_receipt/call:charge_card"
    );

    // Example 09: `res.status(500).json({ error: String(error) })`.
    let mut ts = File::new("src/routes/orders.ts");
    let json = ts.call(0, &name("res").member("status").call().member("json"));
    let status = ts.call(json, &name("res").member("status"));
    let string = ts.call(json, &name("String"));
    assert_eq!(ts.key(json), "src/routes/orders.ts#/call:res.status().json");
    assert_eq!(
        ts.key(status),
        "src/routes/orders.ts#/call:res.status().json/call:res.status"
    );
    assert_eq!(
        ts.key(string),
        "src/routes/orders.ts#/call:res.status().json/call:String"
    );
}

#[test]
fn callees_are_normalized() {
    let cases = [
        // `res.status(500).json(…)`
        (
            name("res").member("status").call().member("json"),
            "res.status().json",
            "call:res.status().json",
        ),
        // `handlers[kind](event)`
        (name("handlers").index(), "handlers[]", "call:handlers%5B%5D"),
        // `", ".join(names)`
        (Callee::string().member("join"), "\"\".join", "call:\"\".join"),
        // `(row.get("x") or "").strip()`
        (
            Callee::expression().member("strip"),
            "(…).strip",
            "call:(…).strip",
        ),
        // `run.service.users().messages().get(…)`
        (
            name("run")
                .member("service")
                .member("users")
                .call()
                .member("messages")
                .call()
                .member("get"),
            "run.service.users().messages().get",
            "call:run.service.users().messages().get",
        ),
        // `this.#count.inc()` and `super().save()`
        (
            name("this").member("#count").member("inc"),
            "this.#count.inc",
            "call:this.%23count.inc",
        ),
        (
            name("super").call().member("save"),
            "super().save",
            "call:super().save",
        ),
    ];
    for (callee, text, step) in cases {
        assert_eq!(callee.text(), text);
        let mut f = File::new("a.py");
        let id = f.call(0, &callee);
        assert_eq!(f.key(id), format!("a.py#/{step}"));
    }
}

#[test]
fn a_long_callee_is_cut_at_80_characters() {
    let long = "a".repeat(70);
    let callee = name(&long).member("method_name").call().member("again");
    assert_eq!(callee.text().chars().count(), 80);
    assert_eq!(callee.text(), format!("{long}.method_na"));
    let short = name(&"é".repeat(80));
    assert_eq!(short.text(), "é".repeat(80));
    let cut = short.member("x");
    assert_eq!(cut.text(), "é".repeat(80), "the cut counts characters, not bytes");
    // The cut is taken before escaping, so an escape is never split.
    let brackets = name(&"b".repeat(79)).index();
    assert_eq!(brackets.text(), format!("{}[", "b".repeat(79)));
    let mut f = File::new("a.py");
    let id = f.call(0, &brackets);
    assert_eq!(f.key(id), format!("a.py#/call:{}%5B", "b".repeat(79)));
    assert!(Key::from_text(f.key(id)).parse().is_ok());
}

#[test]
fn a_call_that_takes_a_callback_carries_its_literal_first_argument() {
    // Example 09, with a second route added above the first.
    let post = name("ordersRouter").member("post");
    let mut f = File::new("src/routes/orders.ts");
    f.call(0, &name("Router"));
    let added = f.route(0, &post, "/orders");
    let added_handler = f.inline(added, Inline::Callback);
    let route = f.route(0, &post, "/orders/:id");
    let handler = f.inline(route, Inline::Callback);
    let attempt = f.form(handler, F::Try);
    assert_eq!(
        f.key(added_handler),
        "src/routes/orders.ts#/call:ordersRouter.post(\"/orders\")/callback[0]"
    );
    assert_eq!(
        f.key(handler),
        "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]"
    );
    assert_eq!(
        f.key(attempt),
        "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]/try[0]"
    );

    // Other registration calls, and a repeated literal.
    let mut g = File::new("jobs.ts");
    let cron = g.route(0, &name("cron").member("schedule"), "0 2 * * *");
    let describe = g.route(0, &name("describe"), "place_order");
    let again = g.route(0, &name("describe"), "place_order");
    assert_eq!(g.key(cron), "jobs.ts#/call:cron.schedule(\"0 2 * * *\")");
    assert_eq!(g.key(describe), "jobs.ts#/call:describe(\"place_order\")");
    assert_eq!(g.key(again), "jobs.ts#/call:describe(\"place_order\")[1]");
}

#[test]
fn without_a_callback_or_a_literal_the_ordinal_rule_applies() {
    let post = name("app").member("post");
    let mut f = File::new("a.ts");
    // A literal first argument without a callback is not part of the key.
    let plain = f.push(
        0,
        Child::Call {
            callee: &post,
            first_literal: Some("/a"),
            takes_callback: false,
        },
    );
    // A callback without a literal first argument (`app.post(path, handler)` with a variable).
    let variable = f.push(
        0,
        Child::Call {
            callee: &post,
            first_literal: None,
            takes_callback: true,
        },
    );
    // A literal route doesn't count as a repeat of the unliteral ones, and the reverse.
    let literal = f.route(0, &post, "/b");
    let third = f.call(0, &post);
    assert_eq!(f.key(plain), "a.ts#/call:app.post");
    assert_eq!(f.key(variable), "a.ts#/call:app.post[1]");
    assert_eq!(f.key(literal), "a.ts#/call:app.post(\"/b\")");
    assert_eq!(f.key(third), "a.ts#/call:app.post[2]");
}

#[test]
fn literals_escape_quotes_and_backslashes_only() {
    let mut f = File::new("a.ts");
    let id = f.route(0, &name("on"), r#"say "hi" \ to /a#b [c] 100%"#);
    let key = f.key(id).to_owned();
    assert_eq!(key, r#"a.ts#/call:on("say \"hi\" \\ to /a#b [c] 100%")"#);
    let Ok(ParsedKey::Block(parsed)) = Key::from_text(&key).parse() else {
        panic!("a block key");
    };
    assert_eq!(
        parsed.segments(),
        [Segment::Call {
            callee: "on".into(),
            literal: Some(r#"say "hi" \ to /a#b [c] 100%"#.into()),
            repeat: None,
        }]
    );
    assert_eq!(parsed.to_key().as_str(), key);
}

#[test]
fn callbacks_are_in_a_calls_arguments_and_lambdas_are_anywhere_else() {
    let mut f = File::new("src/app.ts");
    let component = f.def(0, "Checkout");
    // `useMutation({ onSuccess: () => …, onError: () => … })`: object properties included.
    let mutation = f.call(component, &name("useMutation"));
    let success = f.inline(mutation, Inline::Callback);
    let error = f.inline(mutation, Inline::Callback);
    // `(() => …)()`: called where it is written, a lambda inside its call.
    let iife = f.call(component, &Callee::expression().call());
    let lambda = f.inline(iife, Inline::Lambda);
    // An unnamed function anywhere else.
    let ret = f.form(component, F::Return);
    let returned = f.inline(ret, Inline::Lambda);
    // A named definition inside a callback extends the callback's key.
    let helper = f.def(success, "helper");
    let repeat = f.def(success, "helper");
    let inner = f.call(helper, &name("go"));
    let nested = f.inline(inner, Inline::Callback);
    let nested_def = f.def(nested, "Local");
    assert_eq!(f.key(success), "src/app.ts#Checkout/call:useMutation/callback[0]");
    assert_eq!(f.key(error), "src/app.ts#Checkout/call:useMutation/callback[1]");
    assert_eq!(f.key(iife), "src/app.ts#Checkout/call:(…)()");
    assert_eq!(f.key(lambda), "src/app.ts#Checkout/call:(…)()/lambda[0]");
    assert_eq!(f.key(returned), "src/app.ts#Checkout/return[0]/lambda[0]");
    assert_eq!(
        f.key(helper),
        "src/app.ts#Checkout/call:useMutation/callback[0].helper"
    );
    assert_eq!(
        f.key(repeat),
        "src/app.ts#Checkout/call:useMutation/callback[0].helper[1]"
    );
    assert_eq!(
        f.key(nested_def),
        "src/app.ts#Checkout/call:useMutation/callback[0].helper/call:go/callback[0].Local"
    );
}

#[test]
fn names_and_callees_escape_reserved_characters() {
    let mut f = File::new("a.ts");
    let odd = f.def(0, "a%b/c#d[e]f:g é");
    let call = f.call(0, &name("x/y").member("z#").index());
    assert_eq!(f.key(odd), "a.ts#a%25b%2Fc%23d%5Be%5Df:g é");
    assert_eq!(f.key(call), "a.ts#/call:x%2Fy.z%23%5B%5D");
    let Ok(ParsedKey::Block(parsed)) = Key::from_text(f.key(call)).parse() else {
        panic!("a block key");
    };
    assert_eq!(
        parsed.segments(),
        [Segment::Call {
            callee: "x/y.z#[]".into(),
            literal: None,
            repeat: None
        }]
    );
}

#[test]
fn bad_pushes_are_rejected_and_push_nothing() {
    let mut keys = KeyBuilder::new(&RepoPath::new("a.py").unwrap());
    assert_eq!(
        keys.push(1, Child::Form(F::If)),
        Err(KeyError::UnknownParent { parent: 1 })
    );
    for form in [
        F::Module,
        F::Class,
        F::Function,
        F::Lambda,
        F::Interface,
        F::Call,
        F::New,
    ] {
        assert_eq!(keys.push(0, Child::Form(form)), Err(KeyError::NotAStep(form)));
    }
    assert_eq!(keys.push(0, Child::Definition("")), Err(KeyError::EmptyName));
    let empty = Callee::name("");
    let call = Child::Call {
        callee: &empty,
        first_literal: None,
        takes_callback: false,
    };
    assert_eq!(keys.push(0, call), Err(KeyError::EmptyName));
    keys.push(0, Child::Form(F::Try)).unwrap();
    keys.push(1, Child::Form(F::Finally)).unwrap();
    assert_eq!(
        keys.push(1, Child::Form(F::Finally)),
        Err(KeyError::RepeatedPart(F::Finally))
    );
    assert_eq!(keys.keys().len(), 3);
    // The rejected `finally` did not take an ordinal: the next `except` is still `except[0]`.
    assert_eq!(
        keys.push(1, Child::Form(F::Except)).unwrap().as_str(),
        "a.py#/try[0]/except[0]"
    );
}

#[test]
fn every_key_form_round_trips_through_parsing() {
    let keys = [
        "shop/service.py#",
        "shop/service.py#place_order",
        "src/repo.ts#Repo.save",
        "a.py#f[1]",
        "a.py#C.f[2].g",
        "shop/cli.py#/if[0]/then/call:main",
        "shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed",
        "a.py#f/if[1]/elif[3]/return[0]",
        "a.py#f/match[0]/case[2]/raise[0]",
        "a.py#f/match[0]/default/break[0]",
        "a.py#f/for[0]/else/continue[1]",
        "a.py#f/while[0]/body/yield[0]",
        "a.ts#f/do[0]/body/throw[0]",
        "a.ts#f/switch[0]/case[0]/return[0]",
        "a.ts#f/try[0]/catch/call:log",
        "a.py#f/try[0]/finally",
        "a.py#f/with[0]/body",
        "a.ts#f/using[0]/body",
        "a.py#f/ternary[0]/then/call:Routed",
        "a.py#f/comprehension[0]/body/call:is_relevant",
        "a.py#f/call:logger.info[1]",
        "a.ts#/call:res.status().json",
        "a.ts#/call:handlers%5B%5D",
        "a.py#/call:\"\".join",
        "a.py#/call:(…).strip",
        "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]",
        "a.ts#/call:on(\"a \\\"b\\\" \\\\\")[2]/callback[1].helper/lambda[0]",
        "a.ts#/call:(…)()/lambda[0]",
        "a.ts#/call:x/callback[0].Local[1].method/return[0]",
        "a.ts#a%25b%2Fc%23d%5Be%5Df",
        "dir#x/file.py#f",
        "entrance:shop-api",
        "entrance:shop-api/place-an-order",
        "entrance:1-collect-incoming-email/sync",
    ];
    for text in keys {
        let parsed = Key::from_text(text)
            .parse()
            .unwrap_or_else(|error| panic!("{error}"));
        match &parsed {
            ParsedKey::Block(block) => assert_eq!(block.to_key().as_str(), text),
            ParsedKey::App { app } => assert_eq!(format!("entrance:{app}"), text),
            ParsedKey::Entrance { app, entrance } => assert_eq!(format!("entrance:{app}/{entrance}"), text),
        }
    }
}

#[test]
fn parsing_reads_the_structure() {
    let key = Key::from_text(
        "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0].h[1]/try[0]",
    );
    let Ok(ParsedKey::Block(parsed)) = key.parse() else {
        panic!("a block key");
    };
    assert_eq!(parsed.path().as_str(), "src/routes/orders.ts");
    assert_eq!(
        parsed.segments(),
        [
            Segment::Call {
                callee: "ordersRouter.post".into(),
                literal: Some("/orders/:id".into()),
                repeat: None
            },
            Segment::Inline {
                inline: Inline::Callback,
                ordinal: 0
            },
            Segment::Name {
                name: "h".into(),
                repeat: Some(1)
            },
            Segment::Form {
                form: F::Try,
                ordinal: Some(0)
            },
        ]
    );
    let Ok(ParsedKey::Block(module)) = Key::from_text("dir#x/file.py#").parse() else {
        panic!("a block key");
    };
    assert_eq!(module.path().as_str(), "dir#x/file.py");
    assert!(module.segments().is_empty());
    assert_eq!(
        Key::from_text("entrance:shop-api/place-an-order").parse(),
        Ok(ParsedKey::Entrance {
            app: "shop-api".into(),
            entrance: "place-an-order".into()
        })
    );
    assert_eq!(
        Key::from_text("entrance:shop-api").parse(),
        Ok(ParsedKey::App {
            app: "shop-api".into()
        })
    );
}

#[test]
fn text_outside_the_grammar_is_not_a_key() {
    let bad = [
        "",
        "shop/service.py",
        "#place_order",
        "../a.py#f",
        "a.py#/",
        "a.py#.f",
        "a.py#f.",
        "a.py#f//if[0]",
        "a.py#f/if",
        "a.py#f/if[01]",
        "a.py#f/if[-1]",
        "a.py#f/if[x]",
        "a.py#f/if[0",
        "a.py#f/then[0]",
        "a.py#f/elif",
        "a.py#f/function[0]",
        "a.py#f/call[0]",
        "a.py#f/lambda",
        "a.py#f/statement[0]",
        "a.py#f/call:",
        "a.py#f/call:x[0]",
        "a.py#f[0]",
        "a.py#f/call:x(\"open",
        "a.py#f/call:x(\"a\"",
        "a.py#f/call:x(\"a\\n\")",
        "a.py#f/call:a]b",
        "a.py#f%2",
        "a.py#f%41",
        "a.py#f/if[0].g",
        "a.py#function:place_order/variable[0]",
        "entrance:",
        "entrance:Shop-API",
        "entrance:-shop",
        "entrance:shop--api",
        "entrance:shop/",
        "entrance:shop/a/b",
        "entrance:configured:api-start",
        r#"entrance:["express.route","http","POST","/orders","requireUser"]"#,
    ];
    for text in bad {
        assert!(Key::from_text(text).parse().is_err(), "{text} parsed");
    }
    let error = Key::from_text("a.py#f/if").parse().unwrap_err();
    assert_eq!(
        error.to_string(),
        "`a.py#f/if` is not a key: a control block, exit, or repeating part has no ordinal"
    );
}

#[test]
fn entrance_keys_come_from_names() {
    assert_eq!(Key::app("Shop API").unwrap().as_str(), "entrance:shop-api");
    assert_eq!(
        Key::entrance("Shop API", "Place an order").unwrap().as_str(),
        "entrance:shop-api/place-an-order"
    );
    assert_eq!(
        slug("1. Collect incoming email").unwrap(),
        "1-collect-incoming-email"
    );
    assert_eq!(slug("  --Background   jobs!-- ").unwrap(), "background-jobs");
    assert_eq!(slug("Orders API (v2)").unwrap(), "orders-api-v2");
    assert_eq!(slug("Café Größe").unwrap(), "café-größe");
    assert_eq!(slug("ÀB").unwrap(), "àb");
    for invalid in ["", "   ", "--", "!?", "→ ←"] {
        assert_eq!(slug(invalid), Err(SlugError(invalid.into())));
        assert!(Key::app(invalid).is_err());
        assert!(Key::entrance("Shop", invalid).is_err());
        assert!(Key::entrance(invalid, "Place").is_err());
    }
    for name in ["Shop API", "1. Collect incoming email", "Café Größe"] {
        let key = Key::app(name).unwrap();
        assert!(matches!(key.parse(), Ok(ParsedKey::App { .. })), "{key}");
    }
}
