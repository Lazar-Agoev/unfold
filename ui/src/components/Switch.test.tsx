// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("is a labeled switch that turns on and off", () => {
    const onChange = vi.fn();
    const { rerender } = render(<Switch label="Show all calls" on={false} onChange={onChange} />);
    const control = screen.getByRole("switch", { name: "Show all calls" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(control);
    expect(onChange).toHaveBeenLastCalledWith(true);
    rerender(<Switch label="Show all calls" on onChange={onChange} />);
    expect(control.getAttribute("aria-checked")).toBe("true");
    expect(control.querySelector(".switch-track.on")).not.toBeNull();
    fireEvent.click(control);
    expect(onChange).toHaveBeenLastCalledWith(false);
  });
});
