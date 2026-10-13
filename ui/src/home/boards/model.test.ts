import { describe, expect, it } from "vitest";
import { boardTitle, editedText } from "./model";

const NOW = new Date(2026, 9, 13, 12).getTime();

describe("Home's board rows", () => {
  it("say when a board was last edited, relative to now", () => {
    expect(editedText({ updatedAtMs: NOW - 2 * 60_000 }, NOW)).toBe("edited 2 minutes ago");
    expect(editedText({ updatedAtMs: NOW - 24 * 3_600_000 }, NOW)).toBe("edited yesterday");
    expect(editedText({ updatedAtMs: new Date(2026, 9, 1, 10).getTime() }, NOW)).toBe("edited Oct 1");
  });

  it("show a board's title, else its file name", () => {
    expect(boardTitle({ title: "Place an order", name: "place-an-order" })).toBe("Place an order");
    expect(boardTitle({ title: null, name: "place-an-order" })).toBe("place-an-order");
  });
});
