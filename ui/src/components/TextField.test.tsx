// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SearchField, TextField } from "./TextField";

describe("TextField", () => {
  it("labels the input and reports an error on it", () => {
    render(<TextField label="File name" hint="Saved in .unfold/boards/" error="A board with this name exists." defaultValue="order-lifecycle" />);
    const field = screen.getByRole("textbox", { name: "File name" });
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(field.getAttribute("aria-describedby")?.split(" ")).toHaveLength(2);
    expect(screen.getByText("A board with this name exists.").className).toBe("text-field-error");
  });
});

describe("SearchField", () => {
  it("clears with Escape, then leaves", () => {
    const onChange = vi.fn();
    const { rerender } = render(<SearchField label="Filter entrances" value="orders" onChange={onChange} />);
    const field = screen.getByRole("searchbox", { name: "Filter entrances" });
    fireEvent.keyDown(field, { key: "Escape" });
    expect(onChange).toHaveBeenLastCalledWith("");
    fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
    expect(onChange).toHaveBeenCalledTimes(2);
    rerender(<SearchField label="Filter entrances" value="" onChange={onChange} />);
    expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
  });
});
