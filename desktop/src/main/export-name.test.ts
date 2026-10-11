import { describe, expect, it } from "vitest";
import { exportFileName } from "./export-name";

describe("exportFileName", () => {
  it("names the file after the title", () => {
    expect(exportFileName("Order placement")).toBe("Order placement.mmd");
    expect(exportFileName("POST /orders")).toBe("POST -orders.mmd");
    expect(exportFileName('What: "retries"?')).toBe("What- -retries.mmd");
  });

  it("uses a board ID's file name", () => {
    expect(exportFileName(".unfold/boards/order-placement.json")).toBe("order-placement.mmd");
    expect(exportFileName(".unfold/local/drafts/untitled-2.json")).toBe("untitled-2.mmd");
  });

  it("takes another extension", () => {
    expect(exportFileName("Orders - Placing an order", "png")).toBe("Orders - Placing an order.png");
    expect(exportFileName(".unfold/boards/order-placement.json", "png")).toBe("order-placement.png");
    expect(exportFileName(undefined, "png")).toBe("board.png");
  });

  it("falls back to board.mmd", () => {
    expect(exportFileName(undefined)).toBe("board.mmd");
    expect(exportFileName("   ")).toBe("board.mmd");
    expect(exportFileName("...")).toBe("board.mmd");
  });
});
