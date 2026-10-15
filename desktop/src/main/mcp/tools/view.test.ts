import { describe, expect, it, vi } from "vitest";
import type { AgentPage, AgentShellEvent, CodePanelReport, ViewReport } from "../../../shared/agent-api";
import { pageRevision } from "../../../shared/revision";
import { harness, PROJECT } from "./testing";
import { getCurrentView } from "./view";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#createOrder";
const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "compact",
  cards: [{ id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0], view: "expanded", open: [[`${CREATE_ORDER}/call:orderService.place`]] }],
  names: [],
  combined: [{ id: "checks", title: "Check the request", members: [`${CREATE_ORDER}/if[0]`], collapsed: true, author: "agent:claude", signature: "s" }],
  guards: [],
  links: [],
};
const REPORT: ViewReport = { board: BOARD, page: "page-1", selection: { kind: "none" }, visible: ["create-order", "checks"], held: [BOARD] };

/** A window that holds the board, showing `report`. */
function app(report: ViewReport | null) {
  const test = harness({
    window: (request: AgentShellEvent) => (request.type === "agent_read" ? { ok: true, page: PAGE, board: { title: "Order placement", revision: "r1", unsaved: true } } : undefined),
  });
  if (report) {
    test.report(report);
  }
  return test;
}

async function lines(report: ViewReport | null): Promise<string[]> {
  return (await app(report).call(getCurrentView)).text.split("\n");
}

/** The line about the code panel, for what it shows. */
async function codeLine(code: CodePanelReport | undefined): Promise<string | undefined> {
  return (await lines({ ...REPORT, ...(code ? { code } : {}) })).find((line) => line.startsWith("The code panel"));
}

describe("get_current_view", () => {
  it("says when the window hasn't reported, and when it shows Home", async () => {
    expect((await lines(null))[1]).toBe("Unfold's window hasn't reported what it shows: Unfold is starting, or its window is closed. Ask the person to open a board, or start from list_projects.");
    expect((await lines({ selection: { kind: "none" }, visible: [], held: [] })).slice(1)).toEqual(["The window shows Home, not a board.", "Next: ask the person which board to work on, or start from list_projects."]);
  });

  it("reads the page as the window holds it: the selection, the code panel, and the items in view", async () => {
    expect(await lines({ ...REPORT, selection: { kind: "step", card: "create-order", key: `${CREATE_ORDER}/if[0]` } })).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1.`,
      "The window has edits it hasn't saved yet.",
      `Selected: the step ${CREATE_ORDER}/if[0] of card create-order.`,
      "The code panel is closed.",
      "The page has 1 cards. In view: 2 items:",
      `- card create-order: ${CREATE_ORDER} (expanded, 1 call open in place)`,
      '- combined step checks: "Check the request" (collapsed)',
      "Next: read_page reads the whole page. From now on, pass these board and page ids to other tools.",
    ]);
  });

  it("reports the code panel's block and the lines it shows", async () => {
    expect(await codeLine({ key: `${CREATE_ORDER}/if[0]`, file: "src/routes/orders.ts", lines: [27, 31], widened: false, state: "current" })).toBe(
      `The code panel shows the block ${CREATE_ORDER}/if[0], showing src/routes/orders.ts:27–31. read_code with that key reads it.`,
    );
    expect(await codeLine({ span: [32, 33], file: "src/routes/orders.ts", lines: [25, 35], widened: true, state: "changed" })).toBe(
      "The code panel shows the lines src/routes/orders.ts:32–33, showing src/routes/orders.ts:25–35 (the person widened it past the selection's own lines); the file changed since the last compile, so read it before relying on it. read_file with those lines reads it.",
    );
    expect(await codeLine({ key: CREATE_ORDER, widened: false })).toBe(`The code panel shows the block ${CREATE_ORDER}. read_code with that key reads it.`);
    expect(await codeLine({ file: "db/schema.sql", lines: [1, 40], widened: false, state: "current" })).toBe(
      "The code panel shows the file db/schema.sql, showing db/schema.sql:1–40. read_file with those lines reads it.",
    );
  });

  it("names an arrow's calls and a step shown in a call opened in place", async () => {
    const arrow = await lines({ ...REPORT, selection: { kind: "arrow", from: "create-order", to: "place", calls: [`${CREATE_ORDER}/call:orderService.place`] } });
    expect(arrow).toContain(`Selected: the arrow from card create-order to card place, which draws the call ${CREATE_ORDER}/call:orderService.place. read_call with a call's key says what it sends.`);
    // create-order opens orderService.place in place; the window names the definition that holds the step from the card as drawn.
    const inset = await lines({ ...REPORT, selection: { kind: "step", card: "create-order", key: "src/services/order-service.ts#OrderService.place/call:validateOrder", owner: "src/services/order-service.ts#OrderService.place" } });
    expect(inset).toContain(
      "Selected: the step src/services/order-service.ts#OrderService.place/call:validateOrder of src/services/order-service.ts#OrderService.place, shown in a call card create-order opens in place; read_code without key reads that definition.",
    );
  });
});
