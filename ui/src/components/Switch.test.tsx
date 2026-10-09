// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("reports its state and asks for the other one", () => {
    const onChange = vi.fn();
    const { rerender } = render(<Switch checked={false} label="Background updates" onChange={onChange} />);
    const control = screen.getByRole("switch", { name: "Background updates" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(control);
    expect(onChange).toHaveBeenLastCalledWith(true);
    rerender(<Switch checked label="Background updates" onChange={onChange} />);
    fireEvent.click(control);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it("does nothing while disabled", () => {
    const onChange = vi.fn();
    render(<Switch checked label="Express routes" disabled onChange={onChange} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(onChange).not.toHaveBeenCalled();
  });
});
