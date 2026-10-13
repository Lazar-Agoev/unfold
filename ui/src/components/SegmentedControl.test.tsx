// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

const LENS = [
  { value: "code", label: "Code" },
  { value: "flow", label: "Flow" },
] as const;

function Lens({ onChange }: { onChange: (value: "code" | "flow") => void }) {
  const [value, setValue] = useState<"code" | "flow">("code");
  return (
    <SegmentedControl
      options={LENS}
      value={value}
      label="Lens"
      onChange={(next) => {
        setValue(next);
        onChange(next);
      }}
    />
  );
}

describe("SegmentedControl", () => {
  it("is a radio group the arrow keys move through", () => {
    const onChange = vi.fn();
    render(<Lens onChange={onChange} />);
    const group = screen.getByRole("radiogroup", { name: "Lens" });
    expect(group.classList.contains("segmented-lens")).toBe(true);
    const code = screen.getByRole("radio", { name: "Code" });
    expect(code.getAttribute("aria-checked")).toBe("true");
    expect(code.tabIndex).toBe(0);
    fireEvent.keyDown(group, { key: "ArrowRight" });
    expect(onChange).toHaveBeenLastCalledWith("flow");
    expect(screen.getByRole("radio", { name: "Flow" }).getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("radio", { name: "Flow" }));
    fireEvent.keyDown(group, { key: "ArrowRight" });
    expect(onChange).toHaveBeenLastCalledWith("code");
  });
});
