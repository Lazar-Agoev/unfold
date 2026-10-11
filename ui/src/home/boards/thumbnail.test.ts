import { describe, expect, it } from "vitest";
import type { Thumbnail, ThumbnailCard } from "../../ipc";
import { THUMB_HEIGHT, THUMB_WIDTH, cardSize, thumbnailDrawing } from "./thumbnail";

function card(id: string, at: [number, number], overrides: Partial<ThumbnailCard> = {}): ThumbnailCard {
  return { id, at, kind: "block", view: "code", folded: false, lines: 10, fields: null, foreign: false, ...overrides };
}

function thumbnail(cards: ThumbnailCard[], edges: [string, string][] = [], links: [string, string][] = []): Thumbnail {
  return { board: "b", page: { id: "page-1", title: "Page 1" }, pages: 1, cards, edges, links, truncated: false, error: null };
}

describe("board thumbnails", () => {
  it("sizes cards with the canvas's fallback rules", () => {
    const size = (overrides: Partial<ThumbnailCard>) => cardSize(card("x", [0, 0], overrides));
    expect(size({ kind: "entrance", lines: null })).toEqual({ width: 240, height: 160 });
    expect(size({ folded: true, lines: 40 })).toEqual({ width: 520, height: 44 });
    expect(size({ lines: 10 })).toEqual({ width: 520, height: 244 });
    expect(size({ lines: 99 }).height).toBe(44 + 20 * 24);
    expect(size({ lines: null })).toEqual({ width: 520, height: 320 });
  });

  it("sizes Diagram nodes and type cards as the canvas draws them", () => {
    const size = (overrides: Partial<ThumbnailCard>) => cardSize(card("x", [0, 0], overrides));
    // A Diagram node is compact, however long its function.
    expect(size({ view: "diagram", lines: 80 })).toEqual({ width: 280, height: 64 });
    expect(size({ kind: "entrance", view: "diagram", lines: null })).toEqual({ width: 280, height: 64 });
    // A type card lists its fields in Diagram view, and a type declaration in Flow too.
    const three = size({ kind: "type", view: "diagram", fields: 3 });
    expect(three.width).toBe(320);
    expect(size({ kind: "type", view: "flow", fields: 3 })).toEqual(three);
    expect(size({ kind: "type", view: "diagram", fields: 4 }).height - three.height).toBe(20);
    // Twelve fields or more fold behind "N more".
    expect(size({ kind: "type", view: "diagram", fields: 40 }).height).toBe(size({ kind: "type", view: "diagram", fields: 12 }).height);
    // A class keeps its Flow; any type card shows its source in Code.
    expect(size({ kind: "class", view: "flow", fields: 3, lines: 10 })).toEqual({ width: 520, height: 244 });
    expect(size({ kind: "type", view: "code", fields: 3, lines: 10 })).toEqual({ width: 520, height: 244 });
    expect(size({ kind: "type", view: "diagram", fields: 3, folded: true })).toEqual({ width: 320, height: 44 });
  });

  it("fits the page into the box with margins, on half pixels", () => {
    const drawing = thumbnailDrawing(
      thumbnail([card("entry", [0, 42], { kind: "entrance", lines: null }), card("a", [400, 0]), card("b", [1100, 200], { foreign: true })], [["entry", "a"]], [["a", "b"]]),
    );
    expect(drawing.cards).toHaveLength(3);
    for (const item of drawing.cards) {
      expect(item.x).toBeGreaterThanOrEqual(21.5);
      expect(item.y).toBeGreaterThanOrEqual(21.5);
      expect(item.x + item.width).toBeLessThanOrEqual(THUMB_WIDTH - 21.5);
      expect(item.y + item.height).toBeLessThanOrEqual(THUMB_HEIGHT - 21.5);
      expect(item.x % 1).toBe(0.5);
      expect(item.lines.length).toBeGreaterThanOrEqual(2);
      expect(item.lines.length).toBeLessThanOrEqual(5);
    }
    // The entrance card carries the accent line; the foreign card its monogram square.
    expect(drawing.cards[0]?.lines.filter((line) => line.accent)).toHaveLength(1);
    expect(drawing.cards[2]?.monogram).not.toBeNull();
    // Level cards join with a straight line, others with a small step.
    expect(drawing.edges).toMatch(/^M\d+ \d+H\d+$/);
    expect(drawing.links).toContain("C");
  });

  it("keeps a small page small and draws an empty page as nothing", () => {
    const one = thumbnailDrawing(thumbnail([card("a", [0, 0])]));
    expect(one.cards[0]?.width).toBeLessThanOrEqual(64);
    expect(thumbnailDrawing(thumbnail([]))).toEqual({ cards: [], edges: "", links: "" });
  });

  it("draws each thumbnail once", () => {
    const value = thumbnail([card("a", [0, 0])]);
    expect(thumbnailDrawing(value)).toBe(thumbnailDrawing(value));
  });
});
