import { describe, expect, it, vi } from "vitest";
import type { AgentPage } from "../../shared/agent-api";
import type { BoardSummary, ReadBoardResult } from "./engine-types";
import { projectOrView, readPage, resolveBoard } from "./target";
import { harness, PROJECT, projectRow } from "./tools/testing";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const OTHER = "p-python0000";
const ID = ".unfold/boards/orders.json";
const PAGE: AgentPage = { id: "page-2", title: "Async path", defaultView: "flow", cards: [], groups: [], labels: [], guards: [], links: [] };

function summary(project: string): BoardSummary {
  return { project, id: ID, place: "published", name: "orders", title: "Orders", folder: null, pages: 2, cards: 0, projects: [], changes: 0, updatedAtMs: 0, heldBy: null, label: null, error: null };
}

function app(holders: readonly string[]) {
  return harness({
    projects: [projectRow(PROJECT, "ts-orders"), projectRow(OTHER, "py-orders")],
    engine: {
      list_boards: (args) => ({ project: args["project"], boards: holders.includes(String(args["project"])) ? [summary(String(args["project"]))] : [], emptyFolders: [] }),
      read_board: (): ReadBoardResult => ({
        project: PROJECT,
        id: ID,
        place: "published",
        name: "orders",
        document: { format: "unfold-board", version: 1, title: "Orders", projects: [], pages: [{ ...PAGE, id: "page-1", title: "Placing an order" } as never, PAGE as never] },
        revision: "r1",
        published: null,
        heldBy: null,
      }),
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
    tools.report({ board: { project: OTHER, id: ID }, page: "page-2", selection: { kind: "none" }, visible: [], tabs: [] });
    expect(await resolveBoard(tools.context(), {})).toMatchObject({ board: { project: OTHER, id: ID }, page: "page-2" });
    expect(await resolveBoard(tools.context(), { board: ID })).toMatchObject({ project: { id: OTHER }, page: "page-2" });
    expect((await projectOrView(tools.context(), undefined)).id).toBe(OTHER);
    expect((await projectOrView(tools.context(), "ts-orders")).id).toBe(PROJECT);
  });

  it("reads a page from its file when no window holds the board", async () => {
    const tools = app([]);
    const read = await readPage(tools.context(), { project: PROJECT, id: ID }, "page-2");
    expect(read).toMatchObject({ title: "Orders", draft: false, unsaved: false, page: { id: "page-2" } });
    expect((await readPage(tools.context(), { project: PROJECT, id: ID }, undefined)).page.id).toBe("page-1");
    await expect(readPage(tools.context(), { project: PROJECT, id: ID }, "page-9")).rejects.toThrow(
      "Can't find the page: the board “Orders” has no page with id “page-9”. Its pages: Placing an order (id page-1), Async path (id page-2).",
    );
  });
});
