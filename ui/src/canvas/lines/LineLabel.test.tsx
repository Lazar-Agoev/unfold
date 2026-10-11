// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LineLabel } from "./LineLabel";
import { countForms, typeForms } from "./labelText";

const CURVE = { from: { x: 0, y: 0 }, c1: { x: 50, y: 0 }, c2: { x: 50, y: 0 }, to: { x: 100, y: 0 } };
const AT = { x: 50, y: 0, form: 0 };

describe("a line's label", () => {
  it("reads “3 calls” for an arrow that stands for several, selects them, and shows when they are selected", () => {
    const onSelect = vi.fn();
    const { rerender } = render(<LineLabel id="a|node|b" curve={CURVE} forms={countForms(3)} full="3 calls" placement={AT} onSelect={onSelect} />);
    const button = screen.getByRole("button", { name: "Line 3 calls" });
    fireEvent.click(button);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(button.classList.contains("selected")).toBe(false);
    rerender(<LineLabel id="a|node|b" curve={CURVE} forms={countForms(3)} full="3 calls" placement={AT} onSelect={onSelect} selected />);
    expect(screen.getByRole("button", { name: "Line 3 calls" }).classList.contains("selected")).toBe(true);
  });

  it("is quiet text on a type arrow: its field's written type, selecting nothing", () => {
    const { container } = render(<LineLabel id="cart|field:1|item" curve={CURVE} forms={typeForms({ text: "CartItem[]", spans: [] })} full="CartItem[]" placement={AT} />);
    expect(screen.queryByRole("button")).toBeNull();
    const label = container.querySelector(".line-label.quiet");
    expect(label?.textContent).toBe("CartItem[]");
    expect(label?.getAttribute("title")).toBe("CartItem[]");
  });

  it("draws nothing where the layout found no room", () => {
    const { container } = render(<LineLabel id="x" curve={CURVE} forms={countForms(2)} full="2 calls" placement={null} />);
    expect(container.querySelector(".line-label")).toBeNull();
  });
});
