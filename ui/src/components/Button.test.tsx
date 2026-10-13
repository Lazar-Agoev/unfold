// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, TextButton } from "./Button";

describe("Button", () => {
  it.each(["primary", "secondary", "ghost", "danger"] as const)("draws the %s variant, never submitting by accident", (variant) => {
    render(<Button variant={variant}>Save</Button>);
    const button = screen.getByRole("button", { name: "Save" });
    expect(button.classList.contains(`button-${variant}`)).toBe(true);
    expect(button.classList.contains("button-md")).toBe(true);
    expect(button.getAttribute("type")).toBe("button");
  });

  it("lays out a leading icon", () => {
    render(
      <Button variant="primary" icon="plus">
        New board
      </Button>,
    );
    const newBoard = screen.getByRole("button", { name: "New board" });
    expect(newBoard.classList.contains("button-with-icon")).toBe(true);
    expect(newBoard.querySelector("[data-icon='plus']")).not.toBeNull();
  });

  it("shows the ring while busy and stays focusable", () => {
    render(
      <Button variant="primary" size="sm" busy icon="plus">
        Save
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Save" });
    expect(button.getAttribute("aria-busy")).toBe("true");
    expect(button.classList.contains("button-sm")).toBe(true);
    expect(button.querySelector(".progress-ring")).not.toBeNull();
    expect(button.querySelector("[data-icon='plus']")).toBeNull();
  });
});

describe("TextButton", () => {
  it("is an accent action by default, or a quiet muted link", () => {
    const onClick = vi.fn();
    render(
      <>
        <TextButton onClick={onClick}>Show callers</TextButton>
        <TextButton weight={400} tone="muted">
          Copy link
        </TextButton>
      </>,
    );
    const label = screen.getByRole("button", { name: "Show callers" });
    expect(label.className).toBe("text-button text-button-accent");
    fireEvent.click(label);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Copy link" }).className).toBe("text-button text-button-muted text-button-regular");
  });
});
