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
    ]) {
      expect(parseViewReport(report)).toBeNull();
    }
  });
});

describe("ViewTracker", () => {
  it("keeps the window's last report until it closes", () => {
    const views = new ViewTracker();
    expect(views.lastUsed()).toBeUndefined();
    views.report(1, { ...REPORT, held: [] });
    views.report(1, REPORT);
    expect(views.lastUsed()).toBe(1);
    expect(views.get(1)).toEqual(REPORT);
    views.forget(1);
    expect(views.get(1)).toBeUndefined();
    expect(views.lastUsed()).toBeUndefined();

    // The window opened again, with a new id.
    views.report(2, REPORT);
    expect(views.lastUsed()).toBe(2);
  });

  it("finds the window that holds a board from the reported boards, and follows boards loaded for an agent", () => {
    const views = new ViewTracker();
    views.report(1, REPORT);
    views.report(2, { ...REPORT, held: [] });
    expect(views.holder(EMAILS)).toBe(1);
    expect(views.holder({ project: "p-orders0000", id: "other.json" })).toBeUndefined();

    views.holds(2, EMAILS);
    expect(views.holder(EMAILS)).toBe(2);
    expect(views.get(1)?.held).toEqual([ORDERS]);

    views.released(2, EMAILS);
    expect(views.holder(EMAILS)).toBeUndefined();

    views.holds(3, ORDERS);
    expect(views.get(3)).toEqual({ selection: { kind: "none" }, visible: [], held: [ORDERS] });
  });
});
