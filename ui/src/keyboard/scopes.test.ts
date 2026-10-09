// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { registerScope, runScoped, scopedHandler } from "./scopes";

describe("command scopes", () => {
  it("routes a command to the innermost scope around the focused element", () => {
    const outer = document.createElement("div");
    const inner = document.createElement("div");
    const button = document.createElement("button");
    inner.append(button);
    outer.append(inner);
    document.body.append(outer);

    const outerUndo = vi.fn();
    const innerUndo = vi.fn();
    const removeOuter = registerScope(outer, () => ({ undo: outerUndo }));
    const removeInner = registerScope(inner, () => ({ undo: innerUndo }));

    button.focus();
    expect(runScoped("undo")).toBe(true);
    expect(innerUndo).toHaveBeenCalledTimes(1);
    expect(scopedHandler("redo")).toBeNull();

    removeInner();
    runScoped("undo");
    expect(outerUndo).toHaveBeenCalledTimes(1);
    removeOuter();
    expect(runScoped("undo")).toBe(false);
    outer.remove();
  });

  it("uses the last element pressed when focus is on the body", () => {
    const board = document.createElement("div");
    const canvas = document.createElement("div");
    board.append(canvas);
    document.body.append(board);
    const undo = vi.fn();
    const remove = registerScope(board, () => ({ undo }));
    (document.activeElement as HTMLElement | null)?.blur();

    canvas.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    expect(runScoped("undo")).toBe(true);
    expect(undo).toHaveBeenCalledTimes(1);

    document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    expect(runScoped("undo")).toBe(false);
    remove();
    board.remove();
  });
});
