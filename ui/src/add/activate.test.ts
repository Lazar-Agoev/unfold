import { beforeEach, describe, expect, it, vi } from "vitest";

const boards = vi.hoisted(() => ({
  addBlock: vi.fn(),
  openEntrance: vi.fn(() => Promise.resolve()),
}));
const overlays = vi.hoisted(() => ({ openPeek: vi.fn() }));
const components = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../state/boards", () => boards);
vi.mock("../state/overlays", () => overlays);
vi.mock("../components", () => components);

const { activateChoice } = await import("./activate");

const PAGE = { board: { project: "p-a", id: ".unfold/boards/orders.json" }, page: "page-1" };

beforeEach(() => {
  vi.clearAllMocks();
});

describe("activateChoice", () => {
  it("adds a definition or an entrance to the page", async () => {
    await activateChoice({ kind: "definition", key: "k" }, PAGE);
    expect(boards.addBlock).toHaveBeenCalledWith(PAGE, "k", undefined);
    await activateChoice({ kind: "entrance", key: "e" }, PAGE);
    expect(boards.openEntrance).toHaveBeenCalledWith(PAGE, "e");
  });

  it("adds a type declaration in its own view", async () => {
    await activateChoice({ kind: "definition", key: "src/cart.ts#interface:Cart", view: "diagram" }, PAGE);
    expect(boards.addBlock).toHaveBeenCalledWith(PAGE, "src/cart.ts#interface:Cart", "diagram");
  });

  it("opens a file of the board's project in the peek sheet", async () => {
    await activateChoice({ kind: "file", path: "src/a.ts" }, PAGE);
    expect(overlays.openPeek).toHaveBeenCalledWith({ project: "p-a", path: "src/a.ts" });
    expect(boards.addBlock).not.toHaveBeenCalled();
  });

  it("turns a failure into a toast", async () => {
    boards.openEntrance.mockRejectedValueOnce({ code: "not_compiled", message: "Compile it first." });
    await activateChoice({ kind: "entrance", key: "e" }, PAGE);
    expect(components.showToast).toHaveBeenCalledWith(expect.objectContaining({ body: "Compile it first.", tone: "danger" }));
  });
});
