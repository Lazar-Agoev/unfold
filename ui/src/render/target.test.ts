import { describe, expect, it } from "vitest";
import type { Box, PageGeometry } from "../board/checks";
import type { Frame } from "../ipc/generated";
import { page } from "../test/workspace";
import { imageBounds } from "./target";

const cards = [
  { id: "a", ref: { block: "k#function:a" }, at: [0, 0] as [number, number] },
  { id: "b", ref: { block: "k#function:b" }, at: [500, 300] as [number, number] },
];

function drawn(boxes: Record<string, Box>): PageGeometry {
  return { boxes: new Map(Object.entries(boxes)), lines: [], flows: new Map() };
}

const frame = (extra: Partial<Frame> = {}): Frame => ({ id: "api", title: "API", members: ["a", "b"], look: "box", author: "agent:claude", ...extra });

describe("imageBounds", () => {
  const boxes = { a: { x: 0, y: 0, width: 300, height: 100 }, b: { x: 500, y: 300, width: 200, height: 80 } };

  it("takes everything drawn on the page", () => {
    expect(imageBounds(page("p", cards), drawn(boxes), undefined)).toEqual({ ok: true, bounds: { x: 0, y: 0, width: 700, height: 380 } });
    expect(imageBounds(page("p"), drawn({}), undefined)).toEqual({ ok: false, code: "empty_page", message: "The page has nothing on it yet." });
  });

  it("takes one card, or a frame around its members when the canvas doesn't draw it", () => {
    expect(imageBounds(page("p", cards), drawn(boxes), "b")).toEqual({ ok: true, bounds: boxes.b });
    expect(imageBounds(page("p", cards, { frames: [frame()] }), drawn(boxes), "api")).toEqual({ ok: true, bounds: { x: -24, y: -24, width: 748, height: 428 } });
    const box = { x: -40, y: -60, width: 800, height: 500 };
    expect(imageBounds(page("p", cards, { frames: [frame()] }), drawn({ ...boxes, api: box }), "api")).toEqual({ ok: true, bounds: box });
  });

  it("refuses an item that isn't the page's, a step group, or one a collapsed frame hides", () => {
    const groups = [{ id: "checks", card: "a", title: "Checks", members: [], collapsed: false }];
    expect(imageBounds(page("p", cards, { groups }), drawn(boxes), "checks")).toEqual({
      ok: false,
      code: "item_not_found",
      message: "The page has no card, frame, resource card, or note `checks`.",
    });
    const frames = [frame({ members: ["inner"], collapsed: true }), { ...frame({ id: "inner", members: ["a"] }) }];
    expect(imageBounds(page("p", cards, { frames }), drawn(boxes), "a")).toMatchObject({ ok: false, code: "item_hidden" });
    expect(imageBounds(page("p", cards, { frames }), drawn(boxes), "inner")).toMatchObject({ ok: false, code: "item_hidden" });
  });
});
