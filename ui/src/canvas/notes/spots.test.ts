import { describe, expect, it } from "vitest";
import { STACK_GAP } from "../layout";
import { NOTE_GAP } from "../frames/geometry";
import { lineMiddle } from "../lines/paths";
import { lineNoteSpot } from "./spots";

const NOTE = { width: 160, height: 40 };

describe("lineMiddle", () => {
  it("is the middle of the routed line's longest straight run (the first, among equals)", () => {
    expect(lineMiddle([{ x: 400, y: 22 }, { x: 520, y: 22 }])).toEqual({ x: 460, y: 22 });
    const turns = [
      { x: 200, y: 22 },
      { x: 260, y: 22 },
      { x: 260, y: 322 },
      { x: 300, y: 322 },
    ];
    expect(lineMiddle(turns)).toEqual({ x: 260, y: 172 });
    expect(lineMiddle([{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }])).toEqual({ x: 50, y: 0 });
  });
});

describe("lineNoteSpot", () => {
  const middle = { x: 500, y: 100 };

  it("centers the note under the middle, else over it, where that is clear", () => {
    expect(lineNoteSpot(middle, NOTE, [])).toEqual([420, 100 + NOTE_GAP]);
    const under = { x: 400, y: 150, width: 200, height: 100 };
    expect(lineNoteSpot(middle, NOTE, [under])).toEqual([420, 100 - NOTE_GAP - NOTE.height]);
  });

  it("else moves the nearer of the two clear: up over the cards the line runs between, or down past them", () => {
    // Two tall cards with the line between their headers: over them is nearer than under.
    const cards = [
      { x: 0, y: 78, width: 440, height: 800 },
      { x: 560, y: 78, width: 300, height: 700 },
    ];
    expect(lineNoteSpot(middle, NOTE, cards)).toEqual([420, 78 - STACK_GAP - NOTE.height]);
    // With a tall card just over them, under them is nearer.
    const roof = { x: 0, y: -2000, width: 900, height: 2050 };
    const [x, y] = lineNoteSpot(middle, NOTE, [...cards, roof]);
    expect(x).toBe(420);
    expect(y).toBe(78 + 800 + STACK_GAP);
  });
});
