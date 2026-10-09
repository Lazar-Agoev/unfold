import { describe, expect, it, vi } from "vitest";
import { diffBoardOrder } from "./useBoardTabs";

vi.mock("../state/boards/api", () => ({}));

describe("diffBoardOrder", () => {
  it("tells a board whose ID changed in place from one that closed", () => {
    expect(diffBoardOrder(["a", "b", "c"], ["a", "B", "c"])).toEqual({ renamed: [["b", "B"]], gone: [] });
    expect(diffBoardOrder(["a", "b", "c"], ["a", "c"])).toEqual({ renamed: [], gone: ["b"] });
    expect(diffBoardOrder(["a"], ["a", "d"])).toEqual({ renamed: [], gone: [] });
  });
});
