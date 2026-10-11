import { describe, expect, it } from "vitest";
import { HEADER_PORT, STACK_GAP } from "../layout";
import { NOTE_GAP } from "../frames/geometry";
import { lineMiddle, lineNoteSpot } from "./spots";

const NOTE = { width: 160, height: 40 };

describe("lineMiddle", () => {
  it("is halfway between the sides facing each other: side ends at the header, top and bottom ends mid-edge", () => {
    const left = { x: 0, y: 0, width: 400, height: 600 };
    expect(lineMiddle(left, { x: 520, y: 100, width: 200, height: 60 })).toEqual({ x: 460, y: 50 + HEADER_PORT });
    expect(lineMiddle({ x: 520, y: 100, width: 200, height: 60 }, left)).toEqual({ x: 460, y: 50 + HEADER_PORT });
    expect(lineMiddle({ x: 0, y: 0, width: 200, height: 100 }, { x: 100, y: 300, width: 200, height: 80 })).toEqual({ x: 150, y: 200 });
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
