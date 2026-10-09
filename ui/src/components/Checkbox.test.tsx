// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("draws checked and unchecked, and asks for the other state", () => {
    const onChange = vi.fn();
    const { rerender } = render(<Checkbox checked label="apps/api" onChange={onChange} />);
    const box = screen.getByRole("checkbox", { name: "apps/api" });
    expect((box as HTMLInputElement).checked).toBe(true);
    expect(box.parentElement?.dataset["state"]).toBe("checked");
    expect(box.parentElement?.querySelector(".checkbox-check")).not.toBeNull();
    fireEvent.click(box);
    expect(onChange).toHaveBeenLastCalledWith(false);
    rerender(<Checkbox checked={false} label="apps/api" onChange={onChange} />);
    fireEvent.click(box);
    expect(onChange).toHaveBeenLastCalledWith(true);
  });

  it("is mixed when some folders inside are excluded, and becomes checked", () => {
    const onChange = vi.fn();
    render(<Checkbox checked="mixed" label="src" onChange={onChange} />);
    const box = screen.getByRole<HTMLInputElement>("checkbox", { name: "src" });
    expect(box.indeterminate).toBe(true);
    expect(box.parentElement?.querySelector(".checkbox-bar")).not.toBeNull();
    fireEvent.click(box);
    expect(onChange).toHaveBeenLastCalledWith(true);
  });

  it("can't change while disabled (ignored folders)", () => {
    render(<Checkbox checked={false} disabled label="node_modules" onChange={vi.fn()} />);
    expect(screen.getByRole<HTMLInputElement>("checkbox", { name: "node_modules" }).disabled).toBe(true);
  });
});
