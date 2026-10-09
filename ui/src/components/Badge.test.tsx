// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Badge } from "./Badge";

describe("Badge", () => {
  it("shows its text with a 12px icon", () => {
    render(<Badge icon="check">File exists</Badge>);
    const badge = screen.getByText("File exists");
    expect(badge.className).toBe("badge");
    expect(badge.querySelector("svg")?.getAttribute("width")).toBe("12");
  });
});
