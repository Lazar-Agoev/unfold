import { beforeEach, describe, expect, it, vi } from "vitest";

const boards = vi.hoisted(() => ({
  addBlock: vi.fn(),
  addPage: vi.fn(() => "page-2"),
  getOpenBoard: vi.fn(() => ({ activePage: "page-1" })),
  newBoard: vi.fn(() => Promise.resolve({ project: "p-a", id: "draft" })),
  openEntrance: vi.fn(() => Promise.resolve()),
  traceEntrance: vi.fn(() => Promise.resolve({ project: "p-a", id: "draft" })),
}));
const overlays = vi.hoisted(() => ({ openPeek: vi.fn() }));
const components = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../state/boards", () => boards);
vi.mock("../state/overlays", () => overlays);
vi.mock("../components", () => components);

const { activateChoice } = await import("./activate");

const BOARD = { project: "p-a", id: ".unfold/boards/orders.json" };
const PAGE = { kind: "page", target: { board: BOARD, page: "page-1" } } as const;
const HOME = { kind: "home", project: "p-a" } as const;

beforeEach(() => {
  vi.clearAllMocks();
});

describe("activateChoice", () => {
  it("adds a function or an entrance to the current page", async () => {
    await activateChoice({ kind: "definition", project: "p-b", key: "k", name: "processRefund" }, PAGE, false);
    expect(boards.addBlock).toHaveBeenCalledWith(PAGE.target, "p-b", "k", undefined);
    await activateChoice({ kind: "entrance", project: "p-b", key: "e" }, PAGE, false);
    expect(boards.openEntrance).toHaveBeenCalledWith(PAGE.target, "p-b", "e");
    expect(boards.addPage).not.toHaveBeenCalled();
  });

  it("adds onto a new page after the current one with ⌘Enter", async () => {
    await activateChoice({ kind: "definition", project: "p-b", key: "k", name: "processRefund" }, PAGE, true);
    expect(boards.addPage).toHaveBeenCalledWith(BOARD);
    expect(boards.addBlock).toHaveBeenCalledWith({ board: BOARD, page: "page-2" }, "p-b", "k", undefined);
  });

  it("from Home starts a new draft in the result's own project", async () => {
    await activateChoice({ kind: "entrance", project: "p-b", key: "e" }, HOME, false);
    expect(boards.traceEntrance).toHaveBeenCalledWith("p-b", "e");
    await activateChoice({ kind: "definition", project: "p-b", key: "k", name: "processRefund" }, HOME, false);
    expect(boards.newBoard).toHaveBeenCalledWith("p-b", { title: "processRefund" });
    expect(boards.addBlock).toHaveBeenCalledWith({ board: { project: "p-a", id: "draft" }, page: "page-1" }, "p-b", "k", undefined);
  });

  it("adds a type declaration in its own view, on the page or in a new draft", async () => {
    const cart = { kind: "definition", project: "p-b", key: "src/cart.ts#interface:Cart", name: "Cart", view: "diagram" } as const;
    await activateChoice(cart, PAGE, false);
    expect(boards.addBlock).toHaveBeenLastCalledWith(PAGE.target, "p-b", cart.key, "diagram");
    await activateChoice(cart, HOME, false);
    expect(boards.addBlock).toHaveBeenLastCalledWith({ board: { project: "p-a", id: "draft" }, page: "page-1" }, "p-b", cart.key, "diagram");
  });

  it("opens a file in the peek sheet, and a text match at its line", async () => {
    await activateChoice({ kind: "file", project: "p-a", path: "src/a.ts" }, PAGE, false);
    expect(overlays.openPeek).toHaveBeenLastCalledWith({ project: "p-a", path: "src/a.ts" });
    const matches = [
      { line: 3, text: "a", column: 0, ranges: [[0, 1]] as [number, number][] },
      { line: 9, text: "a", column: 0, ranges: [[0, 1]] as [number, number][] },
    ];
    await activateChoice({ kind: "text", project: "p-a", path: "src/a.ts", matches, current: 1 }, HOME, true);
    expect(overlays.openPeek).toHaveBeenLastCalledWith({ project: "p-a", path: "src/a.ts", line: 9, matches, current: 1 });
    expect(boards.addBlock).not.toHaveBeenCalled();
  });

  it("turns a failure into a toast", async () => {
    boards.traceEntrance.mockRejectedValueOnce({ code: "not_compiled", message: "Compile it first." });
    await activateChoice({ kind: "entrance", project: "p-b", key: "e" }, HOME, false);
    expect(components.showToast).toHaveBeenCalledWith(expect.objectContaining({ body: "Compile it first.", tone: "danger" }));
  });
});
