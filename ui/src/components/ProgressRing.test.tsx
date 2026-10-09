// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressRing } from "./ProgressRing";

describe("ProgressRing", () => {
  it("draws 07's ring at 14 and 12, with the stroke each size is drawn with", () => {
    const ring = (size: 14 | 12, strokeWidth?: number) =>
      render(<ProgressRing size={size} {...(strokeWidth === undefined ? {} : { strokeWidth })} />).container.querySelector("svg");
    expect(ring(14)?.getAttribute("width")).toBe("14");
    expect(ring(14)?.getAttribute("stroke-width")).toBe("1.75");
    expect(ring(12)?.getAttribute("stroke-width")).toBe("2");
    expect(ring(12, 1.5)?.getAttribute("stroke-width")).toBe("1.5");
    expect(ring(14)?.classList.contains("progress-ring")).toBe(true);
    expect(ring(14)?.getAttribute("aria-hidden")).toBe("true");
  });
});
