import { describe, expect, it } from "vitest";
import type { ViewReport } from "../../shared/agent-api";
import { ViewTracker, parseViewReport } from "./views";

const ORDERS = { project: "p-orders0000", id: ".unfold/boards/orders.json" };
const DRAFT = { project: "p-orders0000", id: ".unfold/local/drafts/b.json" };

const REPORT: ViewReport = {
  board: ORDERS,
  page: "page-1",
  selection: { kind: "card", id: "create-order" },
  lens: "flow",
  visible: ["create-order", "place"],
  tabs: [ORDERS, DRAFT],
};

describe("parseViewReport", () => {
  it("accepts every selection, and Home (no board)", () => {
    expect(parseViewReport(REPORT)).toEqual(REPORT);
    const selections = [
      { kind: "none" },
      { kind: "step", card: "create-order", key: "src/a.ts#function:a/if[0]" },
      { kind: "group", id: "checks" },
      { kind: "link", id: "a-to-b" },
      { kind: "frame", id: "server" },
      { kind: "line", from: "a", to: "b", project: "p-orders0000", source: "src/a.ts#function:a/call[0]", seq: 0, line: 12 },
      { kind: "line", from: "a", to: "b", project: "p-orders0000", source: "src/a.ts#function:a/call[0]", seq: 1, line: null },
      {
        kind: "calls",
        from: "a",
        to: "b",
        project: "p-orders0000",
        calls: [
          { source: "src/a.ts#function:a/call[0]", seq: 0, line: 12 },
          { source: "src/a.ts#function:a/call[1]", seq: 0, line: 14 },
        ],
      },
    ];
    for (const selection of selections) {
      expect(parseViewReport({ ...REPORT, selection })?.selection).toEqual(selection);
    }
    expect(parseViewReport({ selection: { kind: "none" }, visible: [], tabs: [] })).toEqual({ selection: { kind: "none" }, visible: [], tabs: [] });
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
      { ...REPORT, selection: { kind: "resource", id: "x" } },
      { ...REPORT, selection: { kind: "line", from: "a", to: "b", project: "p", source: "s", seq: 0.5, line: null } },
      { ...REPORT, selection: { kind: "calls", from: "a", to: "b", project: "p", calls: [{ source: "s", seq: 0, line: "12" }] } },
      { ...REPORT, selection: { kind: "calls", from: "a", to: "b", project: "p", calls: "s" } },
      { ...REPORT, visible: ["a", 1] },
      { ...REPORT, tabs: [ORDERS, { id: "b" }] },
      { ...REPORT, tabs: undefined },
    ]) {
      expect(parseViewReport(report)).toBeNull();
    }
  });
});

describe("ViewTracker", () => {
  it("keeps each window's last report and which window was used last", () => {
    const views = new ViewTracker();
    expect(views.lastUsed()).toBeUndefined();
    views.report(2, { ...REPORT, tabs: [] });
    expect(views.lastUsed()).toBe(2);
    views.report(1, REPORT);
    views.focused(1);
    views.focused(2);
    expect(views.lastUsed()).toBe(2);
    views.focused(1);
    expect(views.lastUsed()).toBe(1);
    expect(views.get(1)).toEqual(REPORT);
    views.forget(1);
    expect(views.get(1)).toBeUndefined();
    expect(views.lastUsed()).toBe(2);
  });

  it("finds the window that holds a board from the reported tabs, and follows boards opened for an agent", () => {
    const views = new ViewTracker();
    views.report(1, REPORT);
    views.report(2, { ...REPORT, tabs: [] });
    expect(views.holder(DRAFT)).toBe(1);
    expect(views.holder({ project: "p-orders0000", id: "other.json" })).toBeUndefined();

    views.holds(2, DRAFT);
    expect(views.holder(DRAFT)).toBe(2);
    expect(views.get(1)?.tabs).toEqual([ORDERS]);

    views.released(2, DRAFT);
    expect(views.holder(DRAFT)).toBeUndefined();

    views.holds(3, ORDERS);
    expect(views.get(3)).toEqual({ selection: { kind: "none" }, visible: [], tabs: [ORDERS] });
  });
});
