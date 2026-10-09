import { describe, expect, it, vi } from "vitest";
import { ActionIds } from "./keymap";
import { createActionRegistry } from "./registry";

describe("action registry", () => {
  it("fills in the keymap's default shortcuts", () => {
    const registry = createActionRegistry();
    registry.register({ id: ActionIds.add, title: "Add…", category: "Go", run: () => undefined });
    expect(registry.get(ActionIds.add)?.shortcuts).toEqual(["A", "Mod+K", "/"]);
    registry.register({ id: "custom.thing", title: "Thing", category: "X", run: () => undefined });
    expect(registry.get("custom.thing")?.shortcuts).toEqual([]);
  });

  it("stacks registrations of one id, and unregistering restores the previous one", () => {
    const registry = createActionRegistry();
    const shell = vi.fn();
    const editor = vi.fn();
    registry.register({ id: ActionIds.closeTab, title: "Close Tab", category: "File", run: shell });
    const unregister = registry.register({ id: ActionIds.closeTab, title: "Close Board Tab", category: "File", run: editor });
    registry.run(ActionIds.closeTab);
    expect(editor).toHaveBeenCalledTimes(1);
    expect(registry.list()).toHaveLength(1);
    unregister();
    registry.run(ActionIds.closeTab);
    expect(shell).toHaveBeenCalledTimes(1);
  });

  it("lists actions by category and title, with a stable snapshot until something changes", () => {
    const registry = createActionRegistry();
    registry.register([
      { id: "b", title: "Zoom", category: "View", run: () => undefined },
      { id: "a", title: "Compile", category: "Graph", run: () => undefined },
      { id: "c", title: "Back", category: "View", run: () => undefined },
    ]);
    const first = registry.list();
    expect(first.map((action) => action.id)).toEqual(["a", "c", "b"]);
    expect(registry.list()).toBe(first);
    const listener = vi.fn();
    registry.subscribe(listener);
    registry.register({ id: "d", title: "D", category: "A", run: () => undefined });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.list()).not.toBe(first);
  });

  it("doesn't run disabled actions", () => {
    const registry = createActionRegistry();
    const run = vi.fn();
    let enabled = false;
    registry.register({ id: "x", title: "X", category: "X", enabled: () => enabled, run });
    expect(registry.run("x")).toBe(false);
    expect(registry.run("missing")).toBe(false);
    enabled = true;
    expect(registry.run("x")).toBe(true);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("reports thrown errors and rejected promises", async () => {
    const onError = vi.fn();
    const registry = createActionRegistry({ onError });
    registry.register([
      {
        id: "throws",
        title: "Throws",
        category: "X",
        run: () => {
          throw new Error("boom");
        },
      },
      { id: "rejects", title: "Rejects", category: "X", run: () => Promise.reject(new Error("later")) },
    ]);
    registry.run("throws");
    registry.run("rejects");
    await vi.waitFor(() => {
      expect(onError).toHaveBeenCalledTimes(2);
    });
    expect(onError.mock.calls.map(([error]) => (error as Error).message).sort()).toEqual(["boom", "later"]);
  });
});
