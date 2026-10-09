// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("shows a title, a body, and an action", () => {
    render(<EmptyState title="No boards yet" body="Trace an entrance to start one." action={<button type="button">New board</button>} />);
    expect(screen.getByText("No boards yet").className).toBe("empty-state-title");
    expect(screen.getByText("Trace an entrance to start one.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "New board" })).toBeTruthy();
  });
});
