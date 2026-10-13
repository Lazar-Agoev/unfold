// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, TextButton } from "./Button";

describe("Button", () => {
  it.each(["primary", "secondary", "ghost", "danger"] as const)("draws the %s variant, never submitting by accident", (variant) => {
    render(<Button variant={variant}>Publish</Button>);
    const button = screen.getByRole("button", { name: "Publish" });
    expect(button.classList.contains(`button-${variant}`)).toBe(true);
    expect(button.classList.contains("button-md")).toBe(true);
    expect(button.getAttribute("type")).toBe("button");
  });

  it("lays out a leading icon and a trailing icon", () => {
    render(
      <>
        <Button variant="primary" icon="plus">
          New board
        </Button>
        <Button variant="secondary" trailingIcon="chevron-down">
          Share
        </Button>
      </>,
    );
    const newBoard = screen.getByRole("button", { name: "New board" });
    expect(newBoard.classList.contains("button-with-icon")).toBe(true);
    expect(newBoard.querySelector("[data-icon='plus']")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Share" }).querySelector(".button-trailing[data-icon='chevron-down']")).not.toBeNull();
  });

  it("shows the ring while busy and stays focusable", () => {
    render(
      <Button variant="primary" size="sm" busy icon="publish">
        Publish
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Publish" });
    expect(button.getAttribute("aria-busy")).toBe("true");
    expect(button.classList.contains("button-sm")).toBe(true);
    expect(button.querySelector(".progress-ring")).not.toBeNull();
    expect(button.querySelector("[data-icon='publish']")).toBeNull();
  });
});

describe("TextButton", () => {
  it("is an accent action by default, or a quiet muted link", () => {
    const onClick = vi.fn();
    render(
      <>
        <TextButton onClick={onClick}>Label them</TextButton>
        <TextButton weight={400} tone="muted">
          Copy diagnostics
        </TextButton>
      </>,
    );
    const label = screen.getByRole("button", { name: "Label them" });
    expect(label.className).toBe("text-button text-button-accent");
    fireEvent.click(label);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Copy diagnostics" }).className).toBe("text-button text-button-muted text-button-regular");
  });
});
