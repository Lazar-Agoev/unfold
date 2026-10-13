import { describe, expect, it, vi } from "vitest";
import type { AgentEditRequest, AgentPage, AgentReadRequest, AgentShellEvent } from "../../shared/agent-api";
import type { BoardSummary, ReadBoardResult } from "./engine-types";
import { projectOrView, readPage, resolveBoard } from "./target";
import { readPage as readPageTool } from "./tools/pages";
import { describe as describeTool } from "./tools/shape";
import { harness, PROJECT, projectRow } from "./tools/testing";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const OTHER = "p-python0000";
const ID = ".unfold/boards/orders.json";
const PAGE: AgentPage = { id: "page-2", title: "Async path", defaultView: "flow", cards: [], groups: [], labels: [], guards: [], links: [] };

function summary(project: string): BoardSummary {
  return { project, id: ID, name: "orders", title: "Orders", folder: null, pages: 2, cards: 0, updatedAtMs: 0, label: null, error: null };
}

function file(): ReadBoardResult {
  return {
    project: PROJECT,
    id: ID,
    name: "orders",
    document: { format: "unfold-board", version: 1, title: "Orders", pages: [{ ...PAGE, id: "page-1", title: "Placing an order" } as never, PAGE as never] },
    revision: "r1",
  };
}

function app(holders: readonly string[]) {
  return harness({
    projects: [projectRow(PROJECT, "ts-orders"), projectRow(OTHER, "py-orders")],
    engine: {
      list_boards: (args) => ({ project: args["project"], boards: holders.includes(String(args["project"])) ? [summary(String(args["project"]))] : [], emptyFolders: [] }),
      read_board: file,
    },
  });
}

describe("what a tool works on", () => {
  it("takes a link's board, page, and item, with plain ids overriding its page and item", async () => {
    const tools = app([]);
    expect(await resolveBoard(tools.context(), { board: `unfold://py-orders/${ID}?page=page-1&item=a` })).toMatchObject({ project: { id: OTHER }, board: { project: OTHER, id: ID }, page: "page-1", item: "a" });
    expect(await resolveBoard(tools.context(), { board: `unfold://py-orders/${ID}?page=page-1&item=a`, page: "page-2" })).toMatchObject({ page: "page-2", item: "a" });
  });

  it("finds a board id among the projects' boards, and asks which when several have it", async () => {
    expect(await resolveBoard(app([OTHER]).context(), { board: ID })).toMatchObject({ project: { id: OTHER }, board: { project: OTHER, id: ID } });
    await expect(resolveBoard(app([PROJECT, OTHER]).context(), { board: ID })).rejects.toThrow(
      `Can't tell which board you mean: ts-orders and py-orders each have a board with id “${ID}”. Pass project as well, or a link to the board.`,
    );
    await expect(resolveBoard(app([]).context(), { board: ID })).rejects.toThrow(`Can't find the board: no project has a board with id “${ID}”.`);
  });

  it("defaults to the board and page in view, and the project of the board in view", async () => {
    const tools = app([]);
    tools.report({ board: { project: OTHER, id: ID }, page: "page-2", selection: { kind: "none" }, visible: [], held: [] });
    expect(await resolveBoard(tools.context(), {})).toMatchObject({ board: { project: OTHER, id: ID }, page: "page-2" });
    expect(await resolveBoard(tools.context(), { board: ID })).toMatchObject({ project: { id: OTHER }, page: "page-2" });
    expect((await projectOrView(tools.context(), undefined)).id).toBe(OTHER);
    expect((await projectOrView(tools.context(), "ts-orders")).id).toBe(PROJECT);
  });

  it("reads a page from its file when no window holds the board", async () => {
    const tools = app([]);
    const read = await readPage(tools.context(), { project: PROJECT, id: ID }, "page-2");
    expect(read).toMatchObject({ title: "Orders", unsaved: false, page: { id: "page-2" } });
    expect((await readPage(tools.context(), { project: PROJECT, id: ID }, undefined)).page.id).toBe("page-1");
    await expect(readPage(tools.context(), { project: PROJECT, id: ID }, "page-9")).rejects.toThrow(
      "Can't find the page: the board “Orders” has no page with id “page-9”. Its pages: Placing an order (id page-1), Async path (id page-2).",
    );
  });

  it("reads a board's first page from the window that holds it, unsaved edits included", async () => {
    const edited: AgentPage = { ...PAGE, id: "page-1", title: "Placing an order, renamed" };
    const tools = harness({
      engine: { read_board: (): ReadBoardResult => file() },
      window: (request) => (request.type === "agent_read" && request.page === "page-1" ? { ok: true, page: edited, board: { title: "Orders", revision: "r1", unsaved: true }, sizes: {} } : undefined),
    });
    tools.report({ board: { project: PROJECT, id: ID }, page: "page-2", selection: { kind: "none" }, visible: [], held: [{ project: PROJECT, id: ID }] });
    const read = await readPage(tools.context(), { project: PROJECT, id: ID }, undefined);
    expect(read).toMatchObject({ page: { id: "page-1", title: "Placing an order, renamed" }, unsaved: true, revision: tools.context().revision(edited) });
  });
});

describe("a page link the person copied and pasted", () => {
  const PASTED = { project: OTHER, id: ".unfold/boards/emails.json" };
  const pasted: AgentPage = { ...PAGE, id: "page-7", title: "Sending emails" };

  /** A window that holds both boards: the one in view and the one the link names. */
  function app() {
    const tools = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow(OTHER, "py-orders")],
      engine: { read_board: file },
      window: (request: AgentShellEvent) => {
        if (request.type === "agent_read") {
          const page = request.board.id === PASTED.id ? pasted : PAGE;
          return { ok: true, page, board: { title: request.board.id === PASTED.id ? "Emails" : "Orders", revision: "r1", unsaved: false }, sizes: {} };
        }
        if (request.type === "agent_edit") {
          return { ok: true, page: pasted, board: { title: "Emails", revision: "r2" }, items: [], warnings: [] };
        }
        return undefined;
      },
    });
    tools.report({ board: { project: PROJECT, id: ID }, page: "page-2", selection: { kind: "none" }, visible: [], held: [{ project: PROJECT, id: ID }, PASTED] });
    return tools;
  }

  const link = `unfold://py-orders/${PASTED.id}?page=page-7`;

  it("names the page a read works on, over the page in view", async () => {
    const tools = app();
    const { text } = await tools.call(readPageTool, { page: link });
    expect(text.split("\n")[0]).toContain("py-orders › Emails › Sending emails");
    const reads = tools.requests.filter((request): request is AgentShellEvent & AgentReadRequest => request.type === "agent_read");
    expect(reads.map((request) => [request.board, request.page])).toEqual([[PASTED, "page-7"]]);
  });

  it("names the page an edit works on, over the page in view, passed as page or as board", async () => {
    const tools = app();
    for (const args of [{ page: link }, { board: link }]) {
      expect((await tools.call(describeTool, { ...args, items: [{ id: "send-email", text: "Sends one email." }] })).isError).toBe(false);
    }
    const edits = tools.requests.filter((request): request is AgentShellEvent & AgentEditRequest => request.type === "agent_edit");
    expect(edits.map((request) => [request.board, request.page])).toEqual([
      [PASTED, "page-7"],
      [PASTED, "page-7"],
    ]);
  });
});
