// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { MenuEntry } from "../../components";
import type { BoardDocument, Card, DrawnFlow as Flow, Key } from "../../ipc/generated";
import * as store from "../../state/boards/store";
import { useLayout } from "../../state/layout";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page } from "../../test/workspace";
import { blockMenuEntries, type BlockMenuActions } from "./blockMenu";

const GOLDENS = import.meta.glob<Flow>("../../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

function drawn(name: string): Flow {
  const flow = GOLDENS[`../../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const PLACE = "shop/service.py#place_order";
const LOAD_CALL = `${PLACE}/call:load_order`;
const THEN = `${PLACE}/if[0]/then`;
const DECISION = `${PLACE}/if[0]`;
const RETURN = `${PLACE}/return[0]`;
const WARNING = `${PLACE}/try[0]/except[0]/call:logger.warning`;
const CARD: Card = { id: "place", ref: { block: PLACE }, at: [0, 0], view: "expanded" };

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  engine = installFakeEngine({ handlers: { save_board: ({ board }) => boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"])) } });
});

afterEach(() => {
  engine.uninstall();
});

function open(overrides: Parameters<typeof page>[2] = {}, card: Card = CARD): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [card], overrides)])));
}

/** What a menu handed to its callbacks, by callback. */
interface Handed {
  readonly code: unknown[];
  readonly open: unknown[];
  readonly combine: unknown[];
  readonly propose: unknown[];
}

function actions(): BlockMenuActions & { readonly calls: Handed } {
  const calls: Handed = { code: [], open: [], combine: [], propose: [] };
  return {
    calls,
    onViewCode: (selection) => calls.code.push(selection),
    onToggleOpen: (path) => calls.open.push(path),
    onCombine: (request) => calls.combine.push(request),
    onPropose: (request) => calls.propose.push(request),
  };
}

/** The labels of a menu's items, without separators. */
function labels(entries: readonly MenuEntry[]): string[] {
  return entries.flatMap((entry) => (entry.kind === undefined || entry.kind === "item" ? [entry.label] : []));
}

function choose(entries: readonly MenuEntry[], label: string): void {
  const entry = entries.find((item) => (item.kind === undefined || item.kind === "item") && item.label === label);
  if (!entry || (entry.kind !== undefined && entry.kind !== "item") || !entry.onSelect) {
    throw new Error(`no item ${label}`);
  }
  entry.onSelect();
}

function menu(flow: Flow, key: Key, wired = actions(), card: Card = CARD) {
  return blockMenuEntries(PAGE, card, flow, { kind: "block", key }, wired);
}

describe("a block's menu", () => {
  it("offers what the block's actions allow: a project call opens in place, combines with its body's steps, and takes a proposal after it", () => {
    open();
    expect(labels(menu(drawn("01-place-order"), LOAD_CALL))).toEqual(["View code", "Open in place", "Name…", "Describe…", "Combine…", "Add proposed step after…", "Draw link…"]);
  });

  it("offers a proposal at the start of a part, and nothing its actions don't allow", () => {
    open();
    expect(labels(menu(drawn("01-place-order"), THEN))).toEqual(["View code", "Name…", "Describe…", "Add proposed step at the start…"]);
  });

  it("leaves out Combine… when the flow doesn't draw the body it combines in (a definition's own, in its module)", () => {
    open();
    expect(labels(menu(drawn("01-place-order"), PLACE))).toEqual(["View code", "Name…", "Describe…", "Add proposed step after…", "Add proposed step at the start…"]);
  });

  it("offers only View code, Name…, and Describe… when its actions allow nothing else", () => {
    open();
    const flow = drawn("01-place-order");
    const step = flow.steps.steps.find((item) => item.kind === "exit");
    if (step?.kind !== "exit") {
      throw new Error("no exit");
    }
    step.actions = { after: false, start: false, open: false };
    expect(labels(menu(flow, step.key))).toEqual(["View code", "Name…", "Describe…"]);
  });

  it("opens and closes a call in place along its open path", () => {
    open();
    const wired = actions();
    choose(menu(drawn("01-place-order"), LOAD_CALL, wired), "Open in place");
    expect(wired.calls.open).toEqual([[LOAD_CALL]]);
    const opened = { ...CARD, open: [[LOAD_CALL]] };
    expect(labels(menu(drawn("01-place-order"), LOAD_CALL, wired, opened))).toContain("Close in place");
  });

  it("hands Combine… the body's steps, drawn or not, and a whole control block combined by itself only itself", () => {
    open();
    const wired = actions();
    const flow = drawn("01-place-order");
    choose(menu(flow, DECISION, wired), "Combine…");
    expect(wired.calls.combine).toEqual([{ key: DECISION, steps: flow.combinable }]);
    const step = flow.steps.steps.find((item) => item.kind === "decision");
    if (step?.kind !== "decision") {
      throw new Error("no decision");
    }
    step.actions = { ...step.actions, combine: { with: "alone" } };
    choose(menu(flow, DECISION, wired), "Combine…");
    expect(wired.calls.combine[1]).toEqual({ key: DECISION, steps: null });
  });

  it("hands Add proposed step… where it goes and the block's words", () => {
    open();
    const wired = actions();
    choose(menu(drawn("01-place-order"), RETURN, wired), "Add proposed step after…");
    choose(menu(drawn("01-place-order"), THEN, wired), "Add proposed step at the start…");
    expect(wired.calls.propose).toEqual([
      { place: { after: RETURN }, label: "return order" },
      { place: { start: THEN }, label: "order is None" },
    ]);
  });

  it("names a block in Selected, a library call in a thin row too (the reader lifts it into its own box), and describes the card", () => {
    open();
    useLayout.setState({ selectedCollapsed: true });
    choose(menu(drawn("01-place-order"), WARNING), "Name…");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place", key: WARNING });
    expect(useLayout.getState().selectedCollapsed).toBe(false);
    choose(menu(drawn("01-place-order"), WARNING), "Describe…");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place", key: null });
  });

  it("starts a link from an unknown call, which becomes the link's step, so Kind ▸ can make it a calls connection", () => {
    const checkout: Card = { id: "checkout", ref: { block: "shop/checkout.py#checkout" }, at: [0, 0], view: "expanded" };
    open({}, checkout);
    const unknownCall = "shop/checkout.py#checkout/call:audit.record";
    choose(menu(drawn("06-checkout"), unknownCall, actions(), checkout), "Draw link…");
    expect(store.getPageView(PAGE).linking).toEqual({ from: "checkout", step: unknownCall, to: null });
  });

  it("opens a block's code", () => {
    open();
    const wired = actions();
    choose(menu(drawn("01-place-order"), LOAD_CALL, wired), "View code");
    expect(wired.calls.code).toEqual([{ kind: "block", key: LOAD_CALL }]);
  });
});

describe("a combined step's menu", () => {
  const COMBINED = { id: "stop", title: "Stop if missing", members: [DECISION], collapsed: true, author: "person:maya", signature: "" };

  it("views its code, opens or collapses it, names and describes it in Selected, and uncombines it, each one undo step", () => {
    open({ combined: [COMBINED] });
    const wired = actions();
    const entries = () => blockMenuEntries(PAGE, CARD, drawn("place-order-shaped"), { kind: "combined", item: "stop" }, wired);
    expect(labels(entries())).toEqual(["View code", "Open", "Name…", "Describe…", "Uncombine"]);
    choose(entries(), "View code");
    expect(wired.calls.code).toEqual([{ kind: "combined", item: "stop" }]);
    choose(entries(), "Open");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(false);
    expect(labels(entries())).toContain("Collapse");
    choose(entries(), "Name…");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "combined", combinedId: "stop" });
    choose(entries(), "Uncombine");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined).toEqual([]);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined.map((item) => item.id)).toEqual(["stop"]);
    store.undo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(true);
    store.redo(REF);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(false);
  });

  it("is empty for a combined step the page no longer has", () => {
    open();
    expect(blockMenuEntries(PAGE, CARD, drawn("place-order-shaped"), { kind: "combined", item: "gone" }, actions())).toEqual([]);
  });
});
