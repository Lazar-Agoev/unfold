// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ResizablePane } from "./Resizable";

const bounds = { min: 180, max: 400, initial: 260 };

function Harness({ edge, onResizeEnd }: { edge: "left" | "right"; onResizeEnd?: (size: number) => void }) {
  const [size, setSize] = useState(260);
  return (
    <ResizablePane id="pane" edge={edge} size={size} bounds={bounds} onResize={setSize} label="Resize sidebar" {...(onResizeEnd ? { onResizeEnd } : {})}>
      <div>content</div>
    </ResizablePane>
  );
}

const handle = () => screen.getByRole("separator", { name: "Resize sidebar" });

describe("ResizablePane", () => {
  it("resizes from the keyboard within bounds, and Enter or double-click resets", () => {
    const onResizeEnd = vi.fn();
    render(<Harness edge="right" onResizeEnd={onResizeEnd} />);
    expect(handle().getAttribute("aria-valuenow")).toBe("260");
    expect(handle().getAttribute("aria-controls")).toBe("pane");
    fireEvent.keyDown(handle(), { key: "ArrowRight" });
    expect(handle().getAttribute("aria-valuenow")).toBe("270");
    fireEvent.keyDown(handle(), { key: "ArrowLeft", shiftKey: true });
    expect(handle().getAttribute("aria-valuenow")).toBe("220");
    fireEvent.keyDown(handle(), { key: "Home" });
    expect(handle().getAttribute("aria-valuenow")).toBe("180");
    fireEvent.keyDown(handle(), { key: "ArrowLeft" });
    expect(handle().getAttribute("aria-valuenow")).toBe("180");
    fireEvent.keyDown(handle(), { key: "End" });
    expect(handle().getAttribute("aria-valuenow")).toBe("400");
    fireEvent.keyDown(handle(), { key: "Enter" });
    expect(handle().getAttribute("aria-valuenow")).toBe("260");
    fireEvent.keyDown(handle(), { key: "End" });
    fireEvent.doubleClick(handle());
    expect(handle().getAttribute("aria-valuenow")).toBe("260");
    expect(onResizeEnd).toHaveBeenLastCalledWith(260);
    expect(document.getElementById("pane")?.style.width).toBe("260px");
  });

  it("grows a pane whose handle is on its left edge when the splitter moves left", () => {
    render(<Harness edge="left" />);
    fireEvent.keyDown(handle(), { key: "ArrowLeft" });
    expect(handle().getAttribute("aria-valuenow")).toBe("270");
  });

  it("follows a pointer drag", () => {
    render(<Harness edge="right" />);
    const element = handle();
    element.setPointerCapture = () => undefined;
    fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 100 });
    expect(document.body.dataset["resizing"]).toBe("column");
    fireEvent.pointerMove(element, { pointerId: 1, clientX: 160 });
    expect(element.getAttribute("aria-valuenow")).toBe("320");
    fireEvent.pointerUp(element, { pointerId: 1, clientX: 160 });
    expect(document.body.dataset["resizing"]).toBeUndefined();
  });
});
