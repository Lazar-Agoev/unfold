import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchHit } from "../ipc/generated";

const boards = vi.hoisted(() => ({ addRef: vi.fn() }));
vi.mock("../state/boards", () => boards);

const { addResult } = await import("./activate");

const PAGE = { board: { project: "p-a", id: ".unfold/boards/orders.json" }, page: "page-1" };

const HIT: SearchHit = { project: "p-a", key: "k", name: "place_order", qualifiedName: "place_order", path: "shop/service.py", kind: "function", line: 9, app: null, score: 1 };

beforeEach(() => {
  vi.clearAllMocks();
});

describe("addResult", () => {
  it("puts the result's card on the page, named after it", () => {
    addResult({ id: "definition:k", hit: HIT, ref: { block: "k" } }, PAGE);
    expect(boards.addRef).toHaveBeenCalledWith(PAGE, { block: "k" }, "place_order");
    addResult({ id: "entrance:e", hit: { ...HIT, key: "e", name: "Place an order", kind: "entrance" }, ref: { entrance: "e" } }, PAGE);
    expect(boards.addRef).toHaveBeenLastCalledWith(PAGE, { entrance: "e" }, "Place an order");
  });
});
