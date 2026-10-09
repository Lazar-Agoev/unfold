// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Sheet } from "./Sheet";

describe("Sheet", () => {
  it("floats over the content, and Escape inside it closes it", () => {
    const onClose = vi.fn();
    render(
      <Sheet open label="src/payments/stripe.ts" onClose={onClose}>
        <button type="button">Pin</button>
      </Sheet>,
    );
    const sheet = screen.getByRole("complementary", { name: "src/payments/stripe.ts" });
    expect(sheet.classList.contains("sheet-pinned")).toBe(false);
    fireEvent.keyDown(screen.getByRole("button", { name: "Pin" }), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("takes its place in the row when pinned, at its width", () => {
    render(
      <Sheet open pinned width={520} label="Peek" onClose={vi.fn()}>
        <p>312 lines</p>
      </Sheet>,
    );
    const sheet = screen.getByRole("complementary", { name: "Peek" });
    expect(sheet.classList.contains("sheet-pinned")).toBe(true);
    expect(sheet.style.width).toBe("520px");
  });

  it("renders nothing while closed", () => {
    render(
      <Sheet open={false} label="Peek" onClose={vi.fn()}>
        <p>312 lines</p>
      </Sheet>,
    );
    expect(screen.queryByRole("complementary")).toBeNull();
  });
});
