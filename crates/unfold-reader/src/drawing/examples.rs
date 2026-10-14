//! The examples of `docs/designs/diagram-blocks-examples.html` as graphs, compiled by hand the
//! way the page compiles them: 02–09 here, and 01 (`place_order`) from
//! [`unfold_core::testkit::sample`], which holds exactly its 21 blocks. Ranges are found by
//! line and text, so each block covers what the page's compiled panel shows.

use rustc_hash::FxHashSet;
use unfold_core::key::Callee;
use unfold_core::testkit::{GraphBuilder, NodeRef, builtin, library, project, sample, unknown};
use unfold_core::vocab::{CallRole, Flag, Form, Inline, Language, LinkKind, NodeKind, Reason};
use unfold_core::{Argument, ByteRange, Key, LinkTarget, Node};

/// One file being written into a graph, with ranges found by line.
pub(super) struct Code<'g> {
    pub g: &'g mut GraphBuilder,
    pub module: NodeRef,
    text: &'static str,
    /// The byte offset each one-based line starts at (`starts[line - 1]`).
    starts: Vec<u32>,
    /// The (body, line) pairs where a statement has begun.
    statements: FxHashSet<(u32, u32)>,
}

fn offset(value: usize) -> u32 {
    u32::try_from(value).expect("test offsets fit in u32")
}

impl<'g> Code<'g> {
    pub(super) fn new(g: &'g mut GraphBuilder, path: &str, language: Language, text: &'static str) -> Self {
        let module = g.file(path, language, text);
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(index, _)| offset(index + 1)))
            .collect();
        Self {
            g,
            module,
            text,
            starts,
            statements: FxHashSet::default(),
        }
    }

    /// Line `line` without its indentation and line break.
    pub(super) fn line(&self, line: u32) -> ByteRange {
        let start = self.starts[line as usize - 1] as usize;
        let end = self
            .starts
            .get(line as usize)
            .map_or(self.text.len(), |&end| end as usize - 1);
        let written = &self.text[start..end];
        let indent = written.len() - written.trim_start().len();
        ByteRange {
            start: offset(start + indent),
            end: offset(end),
        }
    }

    /// Lines `first` to `last`.
    pub(super) fn lines(&self, first: u32, last: u32) -> ByteRange {
        ByteRange {
            start: self.line(first).start,
            end: self.line(last).end,
        }
    }

    /// The first `needle` on line `line`.
    pub(super) fn at(&self, line: u32, needle: &str) -> ByteRange {
        let range = self.line(line);
        let found = self.text[range.as_usize()]
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` is on line {line}"));
        ByteRange {
            start: range.start + offset(found),
            end: range.start + offset(found + needle.len()),
        }
    }

    /// The last `needle` on line `line`.
    pub(super) fn last(&self, line: u32, needle: &str) -> ByteRange {
        let range = self.line(line);
        let found = self.text[range.as_usize()]
            .rfind(needle)
            .unwrap_or_else(|| panic!("`{needle}` is on line {line}"));
        ByteRange {
            start: range.start + offset(found),
            end: range.start + offset(found + needle.len()),
        }
    }

    /// From the start of `first` to the end of `last`.
    pub(super) fn span(first: ByteRange, last: ByteRange) -> ByteRange {
        ByteRange {
            start: first.start,
            end: last.end,
        }
    }

    pub(super) fn set(&mut self, node: NodeRef) -> &mut Node {
        self.g.node_mut(node)
    }

    fn fill(&mut self, node: NodeRef, head: Option<ByteRange>, body: Option<ByteRange>) {
        let written = self.set(node);
        written.head = head;
        written.body = body;
    }

    /// A named definition.
    pub(super) fn def(
        &mut self,
        parent: NodeRef,
        form: Form,
        name: &str,
        range: ByteRange,
        head: ByteRange,
        body: ByteRange,
    ) -> NodeRef {
        let node = self.g.def(parent, form, name, range);
        self.fill(node, Some(head), Some(body));
        node
    }

    /// A Python function on lines `first` to `last`, its head on its first line and its body
    /// after it.
    pub(super) fn function(
        &mut self,
        parent: NodeRef,
        name: &str,
        first: u32,
        last: u32,
        head: &str,
    ) -> NodeRef {
        let head = self.at(first, head);
        let body = self.lines(first + 1, last);
        self.def(parent, Form::Function, name, self.lines(first, last), head, body)
    }

    /// A function written in call `call`'s arguments, as its `index`-th argument.
    pub(super) fn callback(
        &mut self,
        call: NodeRef,
        index: u32,
        range: ByteRange,
        head: ByteRange,
        body: ByteRange,
    ) -> NodeRef {
        let node = self.g.inline(call, Inline::Callback, range);
        self.fill(node, Some(head), Some(body));
        self.g.types(call).arguments.push(Argument {
            call: call.local_id(),
            index,
            range,
            expand: None,
            keyword: None,
            pairing: None,
            bound: None,
        });
        node
    }

    /// A control block or part.
    pub(super) fn block(
        &mut self,
        parent: NodeRef,
        form: Form,
        range: ByteRange,
        head: Option<ByteRange>,
        body: Option<ByteRange>,
    ) -> NodeRef {
        let node = self.g.block(parent, form, range);
        self.fill(node, head, body);
        node
    }

    /// A Python part on lines `first` to `last` whose test or clause is the last `test` on its
    /// first line and whose suite starts on the next line.
    pub(super) fn part(
        &mut self,
        parent: NodeRef,
        form: Form,
        first: u32,
        last: u32,
        test: Option<&str>,
    ) -> NodeRef {
        let head = test.map(|test| self.last(first, test));
        self.block(
            parent,
            form,
            self.lines(first, last),
            head,
            Some(self.lines(first + 1, last)),
        )
    }

    /// A part whose range is its suite (a Python `try` body or loop body), on lines `first` to
    /// `last`.
    pub(super) fn suite(&mut self, parent: NodeRef, form: Form, first: u32, last: u32) -> NodeRef {
        let range = self.lines(first, last);
        self.block(parent, form, range, None, Some(range))
    }

    /// An exit that is the whole of line `line`, with its value written as `value`.
    pub(super) fn exit(&mut self, parent: NodeRef, form: Form, line: u32, value: Option<&str>) -> NodeRef {
        let range = self.line(line);
        let value = value.map(|value| self.at(line, value));
        let node = self.g.block(parent, form, range);
        let written = self.set(node);
        written.stmt = Some(range);
        written.value = value;
        node
    }

    /// A call written as `text` on line `line`, its callee the text before the first `(`. The
    /// first call written directly in a body on a line begins that line's statement.
    pub(super) fn call(
        &mut self,
        parent: NodeRef,
        callee: &str,
        line: u32,
        text: &str,
        to: LinkTarget,
    ) -> NodeRef {
        let range = self.at(line, text);
        let callee_length = text.find('(').unwrap_or(text.len());
        let head = ByteRange {
            start: range.start,
            end: range.start + offset(callee_length),
        };
        let form = match &to {
            LinkTarget::Project(_) if callee.starts_with(char::is_uppercase) => Form::New,
            _ => Form::Call,
        };
        let name = Callee::name(callee);
        let node = if form == Form::New {
            self.g.construction(parent, &name, range)
        } else {
            self.g.call(parent, &name, range)
        };
        let in_body = matches!(
            self.set(parent).kind(),
            NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Part
        );
        if in_body && self.statements.insert((parent.local_id(), line)) {
            let statement = self.line(line);
            self.set(node).stmt = Some(statement);
        }
        self.set(node).head = Some(head);
        let written = self.text[head.as_usize()].to_owned();
        self.g.link(node, LinkKind::Calls, head, &written, to);
        node
    }

    /// Sets what call (or expression block) `node`'s value is assigned to.
    pub(super) fn binds(&mut self, node: NodeRef, binds: &str) {
        self.set(node).binds = Some(binds.into());
    }

    pub(super) fn flag(&mut self, node: NodeRef, flag: Flag) {
        self.set(node).flags.insert(flag);
    }

    /// An `imports` link from the module, written as `text` on line `line`.
    pub(super) fn imports(&mut self, line: u32, text: &str, to: LinkTarget) {
        let range = self.at(line, text);
        let module = self.module;
        self.g.link(module, LinkKind::Imports, range, text, to);
    }
}

fn key(text: &str) -> Key {
    Key::from_text(text)
}

const PYTHON_BUILTINS: &str = "python:builtins";

/// 02: `placeOrder`, a switch that is an ordinary decision.
pub(super) fn place_order_ts() -> GraphBuilder {
    const CODE: &str = r#"import { chargeCard, loadOrder, saveOrder } from "./store";

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
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "src/service.ts", Language::TypeScript, CODE);
    let module = c.module;
    let range = Code::span(c.at(3, "async"), c.lines(14, 14));
    let head = c.at(3, "async function placeOrder(id: string)");
    let body = Code::span(c.at(3, "{"), c.lines(14, 14));
    let function = c.def(module, Form::Function, "placeOrder", range, head, body);
    c.flag(function, Flag::Async);
    let load = c.call(
        function,
        "loadOrder",
        4,
        "loadOrder(id)",
        project(&key("src/store.ts#loadOrder")),
    );
    c.binds(load, "order");
    c.flag(load, Flag::Awaited);
    let switch = c.block(
        function,
        Form::Switch,
        c.lines(5, 10),
        Some(c.at(5, "order.status")),
        None,
    );
    let paid = c.block(
        switch,
        Form::Case,
        c.lines(6, 7),
        Some(c.at(6, "\"paid\"")),
        Some(c.lines(7, 7)),
    );
    c.exit(paid, Form::Return, 7, Some("order"));
    let cancelled = c.block(
        switch,
        Form::Case,
        c.lines(8, 9),
        Some(c.at(8, "\"cancelled\"")),
        Some(c.lines(9, 9)),
    );
    let throw = c.exit(cancelled, Form::Throw, 9, Some("new Error(\"cancelled\")"));
    let error = c.call(throw, "Error", 9, "Error(\"cancelled\")", builtin("ecmascript"));
    c.set(error).form = Form::New;
    c.flag(error, Flag::ExitValue);
    let charge = c.call(
        function,
        "chargeCard",
        11,
        "chargeCard(order.card, order.total)",
        project(&key("src/store.ts#chargeCard")),
    );
    c.binds(charge, "order.receipt");
    c.flag(charge, Flag::Awaited);
    let save = c.call(
        function,
        "saveOrder",
        12,
        "saveOrder(order)",
        project(&key("src/store.ts#saveOrder")),
    );
    c.flag(save, Flag::Awaited);
    c.exit(function, Form::Return, 13, Some("order"));
    g
}

/// 03: `sync_inbox`, a with block around a try with every part.
pub(super) fn sync_inbox() -> GraphBuilder {
    const CODE: &str = r#"from inbox.gmail import RateLimited, fetch_messages
from inbox.locks import mailbox_lock
from inbox.notify import notify_synced
from inbox.retry import schedule_retry
from inbox.store import record_attempt, store_messages


def sync_inbox(account):
    """Pull new mail for one account and store it."""
    with mailbox_lock(account):
        try:
            messages = fetch_messages(account)
        except RateLimited as error:
            schedule_retry(account, error.retry_after)
            return 0
        else:
            store_messages(account, messages)
        finally:
            record_attempt(account)
    notify_synced(account)
    return len(messages)
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "inbox/sync.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "sync_inbox", 8, 21, "def sync_inbox(account)");
    c.set(function).doc = Some(c.line(9));
    let with = c.block(
        function,
        Form::With,
        c.lines(10, 19),
        Some(c.at(10, "mailbox_lock(account)")),
        None,
    );
    c.call(
        with,
        "mailbox_lock",
        10,
        "mailbox_lock(account)",
        project(&key("inbox/locks.py#mailbox_lock")),
    );
    let body = c.suite(with, Form::Body, 11, 19);
    let try_block = c.block(body, Form::Try, c.lines(11, 19), None, None);
    let try_body = c.suite(try_block, Form::Body, 12, 12);
    let fetch = c.call(
        try_body,
        "fetch_messages",
        12,
        "fetch_messages(account)",
        project(&key("inbox/gmail.py#fetch_messages")),
    );
    c.binds(fetch, "messages");
    let handler = c.part(
        try_block,
        Form::Except,
        13,
        15,
        Some("except RateLimited as error"),
    );
    let clause = c.at(13, "RateLimited");
    c.g.link(
        handler,
        LinkKind::Types,
        clause,
        "RateLimited",
        project(&key("inbox/gmail.py#RateLimited")),
    );
    c.call(
        handler,
        "schedule_retry",
        14,
        "schedule_retry(account, error.retry_after)",
        project(&key("inbox/retry.py#schedule_retry")),
    );
    c.exit(handler, Form::Return, 15, Some("0"));
    let otherwise = c.part(try_block, Form::Else, 16, 17, None);
    c.call(
        otherwise,
        "store_messages",
        17,
        "store_messages(account, messages)",
        project(&key("inbox/store.py#store_messages")),
    );
    let finally = c.part(try_block, Form::Finally, 18, 19, None);
    c.call(
        finally,
        "record_attempt",
        19,
        "record_attempt(account)",
        project(&key("inbox/store.py#record_attempt")),
    );
    c.call(
        function,
        "notify_synced",
        20,
        "notify_synced(account)",
        project(&key("inbox/notify.py#notify_synced")),
    );
    let exit = c.exit(function, Form::Return, 21, Some("len(messages)"));
    let length = c.call(exit, "len", 21, "len(messages)", builtin(PYTHON_BUILTINS));
    c.flag(length, Flag::ExitValue);
    g
}

/// 04: `reserve_first_free`, continue, break, and a loop's else.
pub(super) fn reserve_first_free() -> GraphBuilder {
    const CODE: &str = r#"from booking.calendar import is_free, list_slots, reserve
from booking.errors import NoFreeSlot
from booking.notify import notify_reserved


def reserve_first_free(calendar, day):
    for slot in list_slots(calendar, day):
        if slot.blocked:
            continue
        if is_free(slot):
            reserve(slot)
            break
    else:
        raise NoFreeSlot(day)
    notify_reserved(calendar, slot)
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "booking/slots.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(
        module,
        "reserve_first_free",
        6,
        15,
        "def reserve_first_free(calendar, day)",
    );
    let header = c.at(7, "for slot in list_slots(calendar, day)");
    let loop_block = c.block(function, Form::For, c.lines(7, 14), Some(header), None);
    c.call(
        loop_block,
        "list_slots",
        7,
        "list_slots(calendar, day)",
        project(&key("booking/calendar.py#list_slots")),
    );
    let body = c.suite(loop_block, Form::Body, 8, 12);
    let blocked = c.block(body, Form::If, c.lines(8, 9), None, None);
    let then = c.part(blocked, Form::Then, 8, 9, Some("slot.blocked"));
    let next = c.exit(then, Form::Continue, 9, None);
    c.set(next).target = Some(loop_block.local_id());
    let free = c.block(body, Form::If, c.lines(10, 12), None, None);
    let then = c.part(free, Form::Then, 10, 12, Some("is_free(slot)"));
    let test = c.call(
        then,
        "is_free",
        10,
        "is_free(slot)",
        project(&key("booking/calendar.py#is_free")),
    );
    c.set(test).role = Some(CallRole::Test);
    c.set(test).stmt = None;
    c.call(
        then,
        "reserve",
        11,
        "reserve(slot)",
        project(&key("booking/calendar.py#reserve")),
    );
    let leave = c.exit(then, Form::Break, 12, None);
    c.set(leave).target = Some(loop_block.local_id());
    let otherwise = c.part(loop_block, Form::Else, 13, 14, None);
    let raise = c.exit(otherwise, Form::Raise, 14, Some("NoFreeSlot(day)"));
    let error = c.call(
        raise,
        "NoFreeSlot",
        14,
        "NoFreeSlot(day)",
        project(&key("booking/errors.py#NoFreeSlot")),
    );
    c.flag(error, Flag::ExitValue);
    c.call(
        function,
        "notify_reserved",
        15,
        "notify_reserved(calendar, slot)",
        project(&key("booking/notify.py#notify_reserved")),
    );
    g
}

/// 05: `route_message`, expressions that decide or repeat.
pub(super) fn route_message() -> GraphBuilder {
    const CODE: &str = r#"from mail.classify import classify, is_relevant
from mail.delivery import deliver
from mail.labels import label_for
from mail.profiles import cached_profile, fetch_profile
from mail.routing import Routed


def route_message(message, override):
    profile = cached_profile(message.sender) or fetch_profile(message.sender)
    decision = Routed(override) if override else classify(message, profile)
    labels = [label_for(part) for part in message.parts if is_relevant(part)]
    return deliver(decision, labels)
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "mail/route.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(
        module,
        "route_message",
        8,
        12,
        "def route_message(message, override)",
    );
    c.call(
        function,
        "cached_profile",
        9,
        "cached_profile(message.sender)",
        project(&key("mail/profiles.py#cached_profile")),
    );
    let fetch = c.call(
        function,
        "fetch_profile",
        9,
        "fetch_profile(message.sender)",
        project(&key("mail/profiles.py#fetch_profile")),
    );
    c.flag(fetch, Flag::Conditional);
    let expression = Code::span(c.at(10, "Routed"), c.line(10));
    let ternary = c.block(function, Form::Ternary, expression, None, None);
    c.binds(ternary, "decision");
    c.set(ternary).stmt = Some(c.line(10));
    let then = c.block(
        ternary,
        Form::Then,
        expression,
        Some(c.last(10, "override")),
        Some(expression),
    );
    c.call(
        then,
        "Routed",
        10,
        "Routed(override)",
        project(&key("mail/routing.py#Routed")),
    );
    let otherwise = c.block(ternary, Form::Else, expression, None, Some(expression));
    c.call(
        otherwise,
        "classify",
        10,
        "classify(message, profile)",
        project(&key("mail/classify.py#classify")),
    );
    let comprehension = Code::span(c.at(11, "["), c.line(11));
    let header = c.at(11, "for part in message.parts");
    let loop_block = c.block(function, Form::Comprehension, comprehension, Some(header), None);
    c.binds(loop_block, "labels");
    c.set(loop_block).stmt = Some(c.line(11));
    let body = c.block(
        loop_block,
        Form::Body,
        comprehension,
        Some(c.at(11, "is_relevant(part)")),
        Some(comprehension),
    );
    let test = c.call(
        body,
        "is_relevant",
        11,
        "is_relevant(part)",
        project(&key("mail/classify.py#is_relevant")),
    );
    c.set(test).role = Some(CallRole::Test);
    c.set(test).stmt = None;
    let label = c.call(
        body,
        "label_for",
        11,
        "label_for(part)",
        project(&key("mail/labels.py#label_for")),
    );
    c.set(label).stmt = None;
    let exit = c.exit(function, Form::Return, 12, Some("deliver(decision, labels)"));
    let deliver = c.call(
        exit,
        "deliver",
        12,
        "deliver(decision, labels)",
        project(&key("mail/delivery.py#deliver")),
    );
    c.flag(deliver, Flag::ExitValue);
    g
}

/// 06: `checkout`, calls written inside other calls.
pub(super) fn checkout() -> GraphBuilder {
    const CODE: &str = r#"import json
import logging

from shop.audit import audit
from shop.format import order_label, summary
from shop.payments import charge_card
from shop.receipts import build_receipt
from shop.store import save_order

logger = logging.getLogger(__name__)


def checkout(order):
    save_order(build_receipt(order, charge_card(order.card)))
    order.status = "paid"
    for line in order.lines:
        line.locked = True
    logger.info("checked out %s", order_label(order))
    audit.record("checkout", json.dumps(summary(order)))
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "shop/checkout.py", Language::Python, CODE);
    let module = c.module;
    let function = c.function(module, "checkout", 13, 19, "def checkout(order)");
    let save = c.call(
        function,
        "save_order",
        14,
        "save_order(build_receipt(order, charge_card(order.card)))",
        project(&key("shop/store.py#save_order")),
    );
    let build = c.call(
        save,
        "build_receipt",
        14,
        "build_receipt(order, charge_card(order.card))",
        project(&key("shop/receipts.py#build_receipt")),
    );
    c.call(
        build,
        "charge_card",
        14,
        "charge_card(order.card)",
        project(&key("shop/payments.py#charge_card")),
    );
    let header = c.at(16, "for line in order.lines");
    let loop_block = c.block(function, Form::For, c.lines(16, 17), Some(header), None);
    c.suite(loop_block, Form::Body, 17, 17);
    let log = c.call(
        function,
        "logger.info",
        18,
        "logger.info(\"checked out %s\", order_label(order))",
        unknown(Reason::UnknownReceiver),
    );
    c.call(
        log,
        "order_label",
        18,
        "order_label(order)",
        project(&key("shop/format.py#order_label")),
    );
    let record = c.call(
        function,
        "audit.record",
        19,
        "audit.record(\"checkout\", json.dumps(summary(order)))",
        unknown(Reason::UnknownReceiver),
    );
    let dumps = c.call(
        record,
        "json.dumps",
        19,
        "json.dumps(summary(order))",
        library("python:json"),
    );
    c.call(
        dumps,
        "summary",
        19,
        "summary(order)",
        project(&key("shop/format.py#summary")),
    );
    g
}

/// 07: `shop/cli.py`, a module's own code, added to the shop sample so that `main` opens into
/// `place_order`.
pub(super) fn cli() -> GraphBuilder {
    const CODE: &str = r#"import sys

from shop.service import place_order


def main():
    place_order(sys.argv[1])


if __name__ == "__main__":
    main()
"#;
    let mut g = sample::shop();
    let mut c = Code::new(&mut g, "shop/cli.py", Language::Python, CODE);
    let module = c.module;
    c.imports(1, "sys", library("python:sys"));
    c.imports(3, "shop.service", project(&key("shop/service.py#")));
    let main = c.function(module, "main", 6, 7, "def main()");
    c.call(
        main,
        "place_order",
        7,
        "place_order(sys.argv[1])",
        project(&key("shop/service.py#place_order")),
    );
    let decision = c.block(module, Form::If, c.lines(10, 11), None, None);
    let then = c.part(decision, Form::Then, 10, 11, Some("__name__ == \"__main__\""));
    c.call(then, "main", 11, "main()", project(&key("shop/cli.py#main")));
    g
}

/// 08: `shop/api.py`, decorators and defaults that run in the enclosing body.
pub(super) fn api() -> GraphBuilder {
    const CODE: &str = r#"from fastapi import APIRouter, Depends, FastAPI

from shop.auth import current_user
from shop.service import place_order

app = FastAPI()
router = APIRouter(prefix="/orders")


@router.post("/{order_id}")
def create_order(order_id: str, user=Depends(current_user)):
    return place_order(order_id)


app.include_router(router)
"#;
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "shop/api.py", Language::Python, CODE);
    let module = c.module;
    c.imports(1, "fastapi", library("fastapi"));
    c.imports(3, "shop.auth", project(&key("shop/auth.py#")));
    c.imports(4, "shop.service", project(&key("shop/service.py#")));
    let app = c.call(module, "FastAPI", 6, "FastAPI()", library("fastapi"));
    c.binds(app, "app");
    let router = c.call(
        module,
        "APIRouter",
        7,
        "APIRouter(prefix=\"/orders\")",
        library("fastapi"),
    );
    c.binds(router, "router");
    let decorator = c.call(
        module,
        "router.post",
        10,
        "router.post(\"/{order_id}\")",
        unknown(Reason::UnknownReceiver),
    );
    c.set(decorator).role = Some(CallRole::Decorator);
    let depends = c.call(module, "Depends", 11, "Depends(current_user)", library("fastapi"));
    c.set(depends).role = Some(CallRole::Default);
    c.set(depends).stmt = None;
    let passed = c.at(11, "current_user");
    c.g.link(
        depends,
        LinkKind::Passes,
        passed,
        "current_user",
        project(&key("shop/auth.py#current_user")),
    );
    let function = c.function(
        module,
        "create_order",
        11,
        12,
        "def create_order(order_id: str, user=Depends(current_user))",
    );
    for call in [decorator, depends] {
        c.set(call).r#for = Some(function.local_id());
    }
    let exit = c.exit(function, Form::Return, 12, Some("place_order(order_id)"));
    let place = c.call(
        exit,
        "place_order",
        12,
        "place_order(order_id)",
        project(&key("shop/service.py#place_order")),
    );
    c.flag(place, Flag::ExitValue);
    c.call(
        module,
        "app.include_router",
        15,
        "app.include_router(router)",
        unknown(Reason::UnknownReceiver),
    );
    g
}

/// 09: `src/routes/orders.ts`, an inline handler keyed by its route.
pub(super) fn orders_route() -> GraphBuilder {
    const CODE: &str = r#"import { Router } from "express";
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
    let mut g = GraphBuilder::new();
    let mut c = Code::new(&mut g, "src/routes/orders.ts", Language::TypeScript, CODE);
    let module = c.module;
    c.imports(1, "express", library("express"));
    c.imports(2, "../service", project(&key("src/service.ts#")));
    let router = c.call(module, "Router", 4, "Router()", library("express"));
    c.binds(router, "ordersRouter");
    let range = Code::span(c.at(6, "ordersRouter.post"), c.at(13, "})"));
    let head = c.at(6, "ordersRouter.post");
    let name = Callee::name("ordersRouter.post");
    let post = c.g.registration(module, &name, Some("/orders/:id"), range);
    let statement = Code::span(range, c.line(13));
    c.set(post).head = Some(head);
    c.set(post).stmt = Some(statement);
    c.g.link(
        post,
        LinkKind::Calls,
        head,
        "ordersRouter.post",
        unknown(Reason::UnknownReceiver),
    );
    let literal = c.at(6, "\"/orders/:id\"");
    c.g.types(post).arguments.push(Argument {
        call: post.local_id(),
        index: 0,
        range: literal,
        expand: None,
        keyword: None,
        pairing: None,
        bound: None,
    });
    let callback_range = Code::span(c.at(6, "async"), c.at(13, "}"));
    let callback_head = c.at(6, "async (req, res) =>");
    let callback_body = Code::span(c.at(6, "{"), c.at(13, "}"));
    let callback = c.callback(post, 1, callback_range, callback_head, callback_body);
    c.flag(callback, Flag::Async);
    let try_block = c.block(
        callback,
        Form::Try,
        Code::span(c.at(7, "try"), c.at(12, "}")),
        None,
        None,
    );
    let body_range = Code::span(c.at(7, "{"), c.at(10, "}"));
    let body = c.block(try_block, Form::Body, body_range, None, Some(body_range));
    let place = c.call(
        body,
        "placeOrder",
        8,
        "placeOrder(req.params.id)",
        project(&key("src/service.ts#placeOrder")),
    );
    c.binds(place, "order");
    c.flag(place, Flag::Awaited);
    c.call(body, "res.json", 9, "res.json(order)", unknown(Reason::Parameter));
    let catch_range = Code::span(c.at(10, "catch"), c.at(12, "}"));
    let catch_body = Code::span(c.at(10, "{"), c.at(12, "}"));
    let handler = c.block(
        try_block,
        Form::Catch,
        catch_range,
        Some(c.at(10, "catch (error)")),
        Some(catch_body),
    );
    let json = c.call(
        handler,
        "res.status().json",
        11,
        "res.status(500).json({ error: String(error) })",
        unknown(Reason::UnknownReceiver),
    );
    c.set(json).head = Some(c.at(11, "res.status(500).json"));
    c.call(
        json,
        "res.status",
        11,
        "res.status(500)",
        unknown(Reason::Parameter),
    );
    c.call(json, "String", 11, "String(error)", builtin("ecmascript"));
    g
}

/// The key of example 09's handler.
pub(super) const ORDERS_HANDLER: &str =
    "src/routes/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]";
