// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ResizeHandle } from "./Resizable";

const bounds = { min: 220, max: 360, initial: 264 };

function Harness({ edge, onResizeEnd }: { edge: "left" | "right"; onResizeEnd?: (size: number) => void }) {
  const [size, setSize] = useState(264);
  return (
    <div id="sidebar" style={{ width: size }}>
      <ResizeHandle
        edge={edge}
        size={size}
        bounds={bounds}
        onResize={setSize}
        label="Resize sidebar"
        controls="sidebar"
        {...(onResizeEnd ? { onResizeEnd } : {})}
      />
    </div>
  );
}

const handle = () => screen.getByRole("separator", { name: "Resize sidebar" });

describe("ResizeHandle", () => {
  it("resizes from the keyboard within bounds, and Enter or double-click resets", () => {
    const onResizeEnd = vi.fn();
    render(<Harness edge="right" onResizeEnd={onResizeEnd} />);
    expect(handle().getAttribute("aria-valuenow")).toBe("264");
    expect(handle().getAttribute("aria-controls")).toBe("sidebar");
    fireEvent.keyDown(handle(), { key: "ArrowRight" });
    expect(handle().getAttribute("aria-valuenow")).toBe("274");
    fireEvent.keyDown(handle(), { key: "ArrowLeft", shiftKey: true });
    expect(handle().getAttribute("aria-valuenow")).toBe("224");
    fireEvent.keyDown(handle(), { key: "Home" });
    expect(handle().getAttribute("aria-valuenow")).toBe("220");
    fireEvent.keyDown(handle(), { key: "ArrowLeft" });
    expect(handle().getAttribute("aria-valuenow")).toBe("220");
    fireEvent.keyDown(handle(), { key: "End" });
    expect(handle().getAttribute("aria-valuenow")).toBe("360");
    fireEvent.keyDown(handle(), { key: "Enter" });
    expect(handle().getAttribute("aria-valuenow")).toBe("264");
    fireEvent.keyDown(handle(), { key: "End" });
    fireEvent.doubleClick(handle());
    expect(handle().getAttribute("aria-valuenow")).toBe("264");
    expect(onResizeEnd).toHaveBeenLastCalledWith(264);
    expect(document.getElementById("sidebar")?.style.width).toBe("264px");
  });

  it("grows a pane whose handle is on its left edge when the splitter moves left", () => {
    render(<Harness edge="left" />);
    fireEvent.keyDown(handle(), { key: "ArrowLeft" });
    expect(handle().getAttribute("aria-valuenow")).toBe("274");
  });

  it("follows a pointer drag", () => {
    render(<Harness edge="right" />);
    const element = handle();
    element.setPointerCapture = () => undefined;
    fireEvent.pointerDown(element, { button: 0, pointerId: 1, clientX: 100 });
    expect(document.body.dataset["resizing"]).toBe("column");
    fireEvent.pointerMove(element, { pointerId: 1, clientX: 160 });
    expect(element.getAttribute("aria-valuenow")).toBe("324");
    fireEvent.pointerUp(element, { pointerId: 1, clientX: 160 });
    expect(document.body.dataset["resizing"]).toBeUndefined();
  });
});
