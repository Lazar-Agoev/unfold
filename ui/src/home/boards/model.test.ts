import { describe, expect, it } from "vitest";
import type { BoardSummary } from "../../ipc";
import { boardSubline, filterBoards, sortBoards } from "./model";
import { thumbnailChunks } from "./useThumbnails";

const NOW = Date.UTC(2026, 9, 9, 12);

function board(id: string, title: string | null, place: BoardSummary["place"], updatedAtMs: number, overrides: Partial<BoardSummary> = {}): BoardSummary {
  return {
    project: "p-1",
    id,
    place,
    name: id,
    title,
    folder: null,
    pages: 1,
    cards: 3,
    projects: [],
    changes: 0,
    updatedAtMs,
    label: null,
    error: null,
    ...overrides,
  };
}

const BOARDS = [
  board("a", "Payment webhooks", "published", NOW - 86_400_000),
  board("b", "Order lifecycle", "published", NOW - 7_200_000),
  board("c", "Auth, end to end", "draft", NOW - 3 * 86_400_000),
  board("d", null, "draft", NOW - 7_200_000),
];

describe("Home's Boards", () => {
  it("filters by place", () => {
    expect(filterBoards(BOARDS, "all")).toHaveLength(4);
    expect(filterBoards(BOARDS, "published").map((item) => item.id)).toEqual(["a", "b"]);
    expect(filterBoards(BOARDS, "drafts").map((item) => item.id)).toEqual(["c", "d"]);
  });

  it("sorts by last edited (ties by title) or by title (natural order)", () => {
    expect(sortBoards(BOARDS, "edited").map((item) => item.id)).toEqual(["d", "b", "a", "c"]);
    expect(sortBoards(BOARDS, "title").map((item) => item.id)).toEqual(["c", "d", "b", "a"]);
    const pages = [board("x", "Page 10", "draft", 0), board("y", "page 2", "draft", 0)];
    expect(sortBoards(pages, "title").map((item) => item.id)).toEqual(["y", "x"]);
  });

  it("describes a board in its subline", () => {
    expect(boardSubline(board("b", "Order lifecycle", "published", NOW - 7_200_000, { pages: 3 }), NOW)).toBe("3 pages · Published · edited 2 hours ago");
    expect(boardSubline(board("c", "x", "published", NOW - 60_000, { changes: 2 }), NOW)).toBe("1 page · Published · 2 unpublished changes · edited a minute ago");
    // A cross-project board says who it's with instead of when it was edited.
    expect(boardSubline(board("e", "Cart → API trace", "draft", NOW, { pages: 2, projects: ["storefront-web"] }), NOW)).toBe("2 pages · Draft");
  });
});

describe("thumbnail chunks", () => {
  it("splits per project into chunks of at most 60, in list order", () => {
    const refs = Array.from({ length: 130 }, (_, index) => ({ project: index % 2 === 0 ? "p-a" : "p-b", id: `b${index}` }));
    const chunks = thumbnailChunks(refs);
    expect(chunks.map((chunk) => [chunk.project, chunk.boards.length])).toEqual([
      ["p-a", 60],
      ["p-b", 60],
      ["p-a", 5],
      ["p-b", 5],
    ]);
    expect(chunks[0]?.indexes.slice(0, 3)).toEqual([0, 2, 4]);
  });
});
