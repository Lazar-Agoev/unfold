import { describe, expect, it } from "vitest";
import type { ViewReport } from "../../shared/agent-api";
import { ViewTracker, parseViewReport } from "./views";

const ORDERS = { project: "p-orders0000", id: ".unfold/boards/orders.json" };
const EMAILS = { project: "p-orders0000", id: ".unfold/boards/emails.json" };

const REPORT: ViewReport = {
  board: ORDERS,
  page: "page-1",
  selection: { kind: "card", id: "create-order" },
  visible: ["create-order", "place"],
  held: [ORDERS, EMAILS],
};

describe("parseViewReport", () => {
  it("accepts every selection, and Home (no board)", () => {
    expect(parseViewReport(REPORT)).toEqual(REPORT);
    const selections = [
      { kind: "none" },
      { kind: "step", card: "create-order", key: "src/a.ts#a/if[0]" },
      { kind: "step", card: "create-order", key: "src/b.ts#b/if[0]", owner: "src/b.ts#b" },
      { kind: "proposal", id: "email-a-confirmation" },
      { kind: "combined", id: "checks" },
      { kind: "link", id: "a-to-b" },
      { kind: "frame", id: "server" },
      { kind: "resource", id: "orders-table" },
      { kind: "note", id: "async-note" },
      { kind: "arrow", from: "a", to: "b", calls: ["src/a.ts#a/call:b"] },
      { kind: "arrow", from: "a", to: "b", calls: ["src/a.ts#a/call:b", "src/a.ts#a/if[0]/then/call:b"] },
    ];
    for (const selection of selections) {
      expect(parseViewReport({ ...REPORT, selection })?.selection).toEqual(selection);
    }
    expect(parseViewReport({ selection: { kind: "none" }, visible: [], held: [] })).toEqual({ selection: { kind: "none" }, visible: [], held: [] });
  });

  it("accepts what the code panel shows: a block, lines of a file, or a file, before and after its lines load", () => {
    const panels = [
      { key: "src/a.ts#a/if[0]", file: "src/a.ts", lines: [3, 9], widened: false, state: "current" },
      { key: "src/a.ts#a", widened: false },
      { span: [12, 14], file: "src/a.ts", lines: [12, 14], widened: true, state: "changed" },
      { file: "db/schema.sql", widened: false, state: "notAnalyzed" },
    ];
    for (const code of panels) {
      expect(parseViewReport({ ...REPORT, code })?.code).toEqual(code);
    }
  });

  it("refuses malformed reports", () => {
    for (const report of [
      null,
      [],
      { ...REPORT, board: { project: "p" } },
      { ...REPORT, page: 3 },
      { ...REPORT, selection: { kind: "card" } },
      { ...REPORT, selection: { kind: "frame" } },
      { ...REPORT, selection: { kind: "table", id: "x" } },
      { ...REPORT, selection: { kind: "note" } },
      { ...REPORT, selection: { kind: "proposal" } },
      { ...REPORT, selection: { kind: "step", card: "a", key: "k", owner: 3 } },
      { ...REPORT, selection: { kind: "group", id: "checks" } },
      { ...REPORT, selection: { kind: "arrow", from: "a", to: "b", calls: [] } },
      { ...REPORT, selection: { kind: "arrow", from: "a", to: "b", calls: [3] } },
      { ...REPORT, selection: { kind: "arrow", from: "a", to: "b", calls: "s" } },
      { ...REPORT, visible: ["a", 1] },
      { ...REPORT, held: [ORDERS, { id: "b" }] },
      { ...REPORT, held: undefined },
      { ...REPORT, code: { key: "k" } },
      { ...REPORT, code: { key: 3, widened: false } },
      { ...REPORT, code: { lines: [0, 3], widened: false } },
      { ...REPORT, code: { span: [1], widened: false } },
      { ...REPORT, code: { widened: false, state: "stale" } },
    ]) {
      expect(parseViewReport(report)).toBeNull();
    }
  });
});

describe("ViewTracker", () => {
  it("keeps the window's last report until it closes", () => {
    const views = new ViewTracker();
    expect(views.current()).toBeUndefined();
    views.report({ ...REPORT, held: [] });
    views.report(REPORT);
    expect(views.current()).toEqual(REPORT);
    views.clear();
    expect(views.current()).toBeUndefined();
    expect(views.holds(ORDERS)).toBe(false);
  });

  it("knows the boards the window holds from its report, and follows boards loaded for an agent", () => {
    const views = new ViewTracker();
    views.report(REPORT);
    expect(views.holds(EMAILS)).toBe(true);
    expect(views.holds({ project: "p-orders0000", id: "other.json" })).toBe(false);

    views.release(EMAILS);
    expect(views.holds(EMAILS)).toBe(false);
    expect(views.current()?.held).toEqual([ORDERS]);

    views.hold(EMAILS);
    views.hold(EMAILS);
    expect(views.current()?.held).toEqual([ORDERS, EMAILS]);

    // Loaded before the window's first report.
    const fresh = new ViewTracker();
    fresh.hold(ORDERS);
    expect(fresh.current()).toEqual({ selection: { kind: "none" }, visible: [], held: [ORDERS] });
  });
});
