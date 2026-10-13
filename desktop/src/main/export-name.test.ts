import { describe, expect, it } from "vitest";
import { exportFileName } from "./export-name";

describe("exportFileName", () => {
  it("names the file after the title", () => {
    expect(exportFileName("Orders - Placing an order")).toBe("Orders - Placing an order.png");
    expect(exportFileName("POST /orders")).toBe("POST -orders.png");
    expect(exportFileName('What: "retries"?')).toBe("What- -retries.png");
  });

  it("uses a board ID's file name", () => {
    expect(exportFileName(".unfold/boards/order-placement.json")).toBe("order-placement.png");
    expect(exportFileName(".unfold/boards/untitled-2.json")).toBe("untitled-2.png");
  });

  it("falls back to board.png", () => {
    expect(exportFileName(undefined)).toBe("board.png");
    expect(exportFileName("   ")).toBe("board.png");
    expect(exportFileName("...")).toBe("board.png");
  });
});
