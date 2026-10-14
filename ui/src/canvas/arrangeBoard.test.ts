import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../ipc/client";
import type { BoardDocument, Card } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const { arrangeBoard } = await import("./arrangeBoard");
const { fallbackSize } = await import("./layout");

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };

function open(cards: Card[] = [], document?: BoardDocument): void {
  store.adoptBoardState(boardState(P, ID, document ?? boardDocument([page("page-1", cards)])));
}

function cards(): readonly Card[] {
  return store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
}

beforeEach(() => {
  engine.call.mockReset();
  engine.call.mockImplementation(() => Promise.resolve(null));
  store.resetBoards();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
});

describe("Arrange", () => {
  it("lays a page out by call depth in one undoable step", () => {
    open([
      { id: "c", ref: { block: "src/c.ts#c" }, at: [0, 900] },
      { id: "b", ref: { block: "src/b.ts#b" }, at: [50, 500] },
      { id: "e", ref: { entrance: "entrance:shop/e" }, at: [900, 0] },
    ]);
    const before = cards().map((card) => card.at);
    const arrows = [
      { id: "1", source: "e", sourceHandle: "node", target: "b", project: "p", kind: "start" as const, dashed: false, order: 0, calls: [] },
      { id: "2", source: "b", sourceHandle: "call:src/b.ts#b/call:c", target: "c", project: "p", kind: "call" as const, dashed: false, order: 0, calls: ["src/b.ts#b/call:c"] },
    ];
    expect(arrangeBoard(PAGE, arrows, () => undefined)).toBe(3);
    const at = Object.fromEntries(cards().map((card) => [card.id, card.at]));
    expect(at["e"]?.[0]).toBeLessThan(at["b"]?.[0] ?? 0);
    expect(at["b"]?.[0]).toBeLessThan(at["c"]?.[0] ?? 0);
    // Arranging again changes nothing.
    expect(arrangeBoard(PAGE, arrows, () => undefined)).toBe(0);
    store.undo(REF);
    expect(cards().map((card) => card.at)).toEqual(before);
  });

  it("arranges a page of resource cards alone (a data model of tables)", () => {
    const table = (id: string, at: [number, number]) => ({ id, kind: "table" as const, name: id, anchors: [], at, author: "agent:claude" });
    open([], boardDocument([page("page-1", [], { resources: [table("orders", [0, 2000]), table("customers", [5000, 0])] })]));
    expect(arrangeBoard(PAGE, [], () => undefined)).toBeGreaterThan(0);
  });

  it("sizes cards it hasn't measured by their view", () => {
    open([
      { id: "b", ref: { block: "src/b.ts#b" }, at: [0, 0], view: "compact" },
      { id: "c", ref: { block: "src/c.ts#c" }, at: [900, 0], view: "compact" },
    ]);
    const arrows = [{ id: "1", source: "b", sourceHandle: "node", target: "c", project: "p", kind: "call" as const, dashed: false, order: 0, calls: ["src/b.ts#b/call:c"] }];
    arrangeBoard(PAGE, arrows, () => undefined);
    const [b, c] = cards();
    // A compact card's column is a compact card's width, not an expanded one's.
    expect((c?.at[0] ?? 0) - (b?.at[0] ?? 0)).toBeLessThan(fallbackSize("expanded").width);
  });
});
