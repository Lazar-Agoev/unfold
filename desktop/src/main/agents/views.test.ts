import { describe, expect, it } from "vitest";
import type { ViewReport } from "../../shared/agent-api";
import { ViewTracker, parseViewReport } from "./views";

const ORDERS = { project: "p-orders0000", id: ".unfold/boards/orders.json" };
const EMAILS = { project: "p-orders0000", id: ".unfold/boards/emails.json" };

const REPORT: ViewReport = {
  board: ORDERS,
  page: "page-1",
  selection: { kind: "card", id: "create-order" },
  lens: "diagram",
  visible: ["create-order", "place"],
  held: [ORDERS, EMAILS],
};

describe("parseViewReport", () => {
  it("accepts every selection, and Home (no board)", () => {
    expect(parseViewReport(REPORT)).toEqual(REPORT);
    const selections = [
      { kind: "none" },
      { kind: "step", card: "create-order", key: "src/a.ts#function:a/if[0]" },
      { kind: "step", card: "create-order", key: "src/b.ts#function:b/if[0]", owner: "src/b.ts#function:b" },
      { kind: "proposal", id: "email-a-confirmation" },
      { kind: "group", id: "checks" },
      { kind: "link", id: "a-to-b" },
      { kind: "frame", id: "server" },
      { kind: "resource", id: "orders-table" },
      { kind: "note", id: "async-note" },
      { kind: "line", from: "a", to: "b", source: "src/a.ts#function:a/call[0]", seq: 0, line: 12 },
      { kind: "line", from: "a", to: "b", source: "src/a.ts#function:a/call[0]", seq: 1, line: null },
      {
        kind: "calls",
        from: "a",
        to: "b",
        calls: [
          { source: "src/a.ts#function:a/call[0]", seq: 0, line: 12 },
          { source: "src/a.ts#function:a/call[1]", seq: 0, line: 14 },
        ],
      },
    ];
    for (const selection of selections) {
      expect(parseViewReport({ ...REPORT, selection })?.selection).toEqual(selection);
    }
    expect(parseViewReport({ selection: { kind: "none" }, visible: [], held: [] })).toEqual({ selection: { kind: "none" }, visible: [], held: [] });
    // A window from before the Diagram/Code switch reports Flow, which is Diagram now.
    expect(parseViewReport({ ...REPORT, lens: "flow" })?.lens).toBe("diagram");
    expect(parseViewReport({ ...REPORT, lens: "code" })?.lens).toBe("code");
  });

  it("refuses malformed reports", () => {
    for (const report of [
      null,
      [],
      { ...REPORT, board: { project: "p" } },
      { ...REPORT, page: 3 },
      { ...REPORT, lens: "table" },
      { ...REPORT, selection: { kind: "card" } },
      { ...REPORT, selection: { kind: "frame" } },
      { ...REPORT, selection: { kind: "table", id: "x" } },
      { ...REPORT, selection: { kind: "note" } },
      { ...REPORT, selection: { kind: "proposal" } },
      { ...REPORT, selection: { kind: "step", card: "a", key: "k", owner: 3 } },
      { ...REPORT, selection: { kind: "line", from: "a", to: "b", source: "s", seq: 0.5, line: null } },
      { ...REPORT, selection: { kind: "calls", from: "a", to: "b", calls: [{ source: "s", seq: 0, line: "12" }] } },
      { ...REPORT, selection: { kind: "calls", from: "a", to: "b", calls: "s" } },
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
