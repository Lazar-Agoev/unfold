// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "./Dialog";
import { Toaster, dismissToast, showToast } from "./Toast";

beforeEach(() => {
  vi.useFakeTimers();
});

// Toasts live in a module store; close what a test left before the next one renders.
afterEach(() => {
  for (const button of screen.queryAllByRole("button", { name: "Dismiss" })) {
    fireEvent.click(button);
  }
  vi.useRealTimers();
});

const titles = () => [...document.querySelectorAll(".toast-title")].map((title) => title.textContent);

describe("Toast", () => {
  it("shows a title, a body, and an action that dismisses it", () => {
    const run = vi.fn();
    render(<Toaster />);
    act(() => {
      showToast({ title: "storefront-web compiled", body: "6 entrances found", action: { label: "Open Home", run } });
    });
    expect(screen.getByText("6 entrances found")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open Home" }));
    expect(run).toHaveBeenCalledTimes(1);
    expect(titles()).toEqual([]);
  });

  it("replaces a toast shown again with the same id, in place", () => {
    render(<Toaster />);
    act(() => {
      showToast({ id: "engine", title: "Reconnecting to the engine…", duration: null });
      showToast({ title: "Board published" });
      showToast({ id: "engine", title: "Reconnected", duration: null });
    });
    expect(titles()).toEqual(["Reconnected", "Board published"]);
    act(() => {
      dismissToast("engine");
    });
    expect(titles()).toEqual(["Board published"]);
  });

  it("hides info after 5 s and errors after 8 s, keeps at most four, and alerts on errors", () => {
    render(<Toaster />);
    act(() => {
      for (const title of ["one", "two", "three", "four", "five"]) {
        showToast({ title });
      }
    });
    expect(titles()).toEqual(["two", "three", "four", "five"]);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(titles()).toEqual([]);

    act(() => {
      showToast({ title: "Couldn't open the file", tone: "danger" });
    });
    expect(screen.getByRole("alert").textContent).toContain("Couldn't open the file");
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(titles()).toEqual(["Couldn't open the file"]);
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(titles()).toEqual([]);
  });

  it("closes from its × button", () => {
    render(<Toaster />);
    act(() => {
      showToast({ title: "Copied the link", duration: null });
    });
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(titles()).toEqual([]);
  });

  it("rises above an open dialog's footer when they would meet, and returns when it closes", () => {
    // jsdom's window is 1024 × 768 and every element 400 × 220 (test/dom): a footer along the bottom right meets the stack.
    let footerTop = 700;
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("dialog-footer") ? new DOMRect(600, footerTop, 400, 44) : new DOMRect();
    });
    const toaster = () => document.querySelector<HTMLElement>(".toaster");
    const dialog = (open: boolean) => <Dialog open={open} onClose={() => undefined} title="Settings" width={470} placement="window" footer={<button type="button">Save</button>} />;
    const { rerender } = render(
      <>
        <Toaster />
        {dialog(true)}
      </>,
    );
    act(() => {
      showToast({ title: "Copied link" });
    });
    expect(toaster()?.style.bottom).toBe(`${768 - 700 + 10}px`);

    footerTop = 100;
    rerender(
      <>
        <Toaster />
        {dialog(false)}
      </>,
    );
    expect(toaster()?.style.bottom).toBe("");
    rerender(
      <>
        <Toaster />
        {dialog(true)}
      </>,
    );
    // A footer that doesn't reach the stack leaves it in its corner.
    expect(toaster()?.style.bottom).toBe("");
    rect.mockRestore();
  });
});
