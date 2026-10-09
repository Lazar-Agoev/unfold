// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RadioGroup } from "./RadioGroup";

describe("RadioGroup", () => {
  it("checks the chosen option and reports a new choice", () => {
    const onChange = vi.fn();
    render(
      <RadioGroup
        label="Conflict"
        value="mine"
        onChange={onChange}
        options={[
          { value: "mine", label: "Keep mine" },
          { value: "theirs", label: "Take theirs and drop my changes" },
        ]}
      />,
    );
    expect(screen.getByRole("radiogroup", { name: "Conflict" })).toBeTruthy();
    expect(screen.getByRole<HTMLInputElement>("radio", { name: "Keep mine" }).checked).toBe(true);
    fireEvent.click(screen.getByRole("radio", { name: "Take theirs and drop my changes" }));
    expect(onChange).toHaveBeenCalledWith("theirs");
  });
});
