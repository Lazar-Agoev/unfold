// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { dispatchShortcut, type DispatchableEvent } from "./dispatcher";
import { ActionIds } from "./keymap";
import { runMenuCommand } from "./menu";
import { createActionRegistry, type ActionSpec } from "./registry";

interface FakeEvent extends DispatchableEvent {
  prevented: boolean;
}

function keydown(init: Partial<Omit<DispatchableEvent, "preventDefault">> & { key: string }): FakeEvent {
  const event: FakeEvent = {
    code: "",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    target: document.body,
    prevented: false,
    preventDefault() {
      event.prevented = true;
    },
    ...init,
  };
  return event;
}

function registryWith(...specs: ActionSpec[]) {
  const registry = createActionRegistry();
  registry.register(specs);
  return registry;
}

const mac = { platform: "darwin", modalOpen: false } as const;
const windows = { platform: "win32", modalOpen: false } as const;

describe("dispatchShortcut", () => {
  it("runs the action for ⌘ on macOS and Ctrl on Windows, and prevents the default", () => {
    const run = vi.fn();
    const registry = registryWith({ id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run });
    const onMac = keydown({ key: "b", metaKey: true });
    expect(dispatchShortcut(onMac, registry, mac)).toBe(ActionIds.toggleSidebar);
    expect(onMac.prevented).toBe(true);
    expect(dispatchShortcut(keydown({ key: "b", ctrlKey: true }), registry, mac)).toBeNull();
    expect(dispatchShortcut(keydown({ key: "b", ctrlKey: true }), registry, windows)).toBe(ActionIds.toggleSidebar);
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("keeps plain-key shortcuts quiet while typing, but lets ⌘ shortcuts through", () => {
    const inspector = vi.fn();
    const sidebar = vi.fn();
    const registry = registryWith(
      { id: ActionIds.toggleInspector, title: "Toggle Inspector", category: "View", run: inspector },
      { id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run: sidebar },
    );
    const input = document.createElement("input");
    const textarea = document.createElement("textarea");
    const checkbox = Object.assign(document.createElement("input"), { type: "checkbox" });

    const typed = keydown({ key: "]", target: input });
    expect(dispatchShortcut(typed, registry, mac)).toBeNull();
    expect(typed.prevented).toBe(false);
    expect(dispatchShortcut(keydown({ key: "]", target: textarea }), registry, mac)).toBeNull();
    expect(dispatchShortcut(keydown({ key: "]", target: checkbox }), registry, mac)).toBe(ActionIds.toggleInspector);
    expect(dispatchShortcut(keydown({ key: "]" }), registry, mac)).toBe(ActionIds.toggleInspector);
    expect(dispatchShortcut(keydown({ key: "b", metaKey: true, target: input }), registry, mac)).toBe(ActionIds.toggleSidebar);
  });

  it("leaves undo to text fields when the action opts out of inputs", () => {
    const undo = vi.fn();
    const registry = registryWith({ id: ActionIds.undo, title: "Undo", category: "Edit", allowInInput: false, run: undo });
    const inField = keydown({ key: "z", metaKey: true, target: document.createElement("input") });
    expect(dispatchShortcut(inField, registry, mac)).toBeNull();
    expect(inField.prevented).toBe(false);
    expect(dispatchShortcut(keydown({ key: "z", metaKey: true }), registry, mac)).toBe(ActionIds.undo);
    expect(dispatchShortcut(keydown({ key: "Z", metaKey: true, shiftKey: true }), registry, mac)).toBeNull();
  });

  it("does nothing while a modal is open, during composition, or after a component handled the key", () => {
    const run = vi.fn();
    const registry = registryWith({ id: ActionIds.togglePanel, title: "Toggle Panel", category: "View", run });
    expect(dispatchShortcut(keydown({ key: "j", metaKey: true }), registry, { platform: "darwin", modalOpen: true })).toBeNull();
    expect(dispatchShortcut(keydown({ key: "j", metaKey: true, isComposing: true }), registry, mac)).toBeNull();
    expect(dispatchShortcut(keydown({ key: "j", metaKey: true, defaultPrevented: true }), registry, mac)).toBeNull();
    expect(run).not.toHaveBeenCalled();
  });

  it("swallows auto-repeat unless the action repeats", () => {
    const toggle = vi.fn();
    const zoom = vi.fn();
    const registry = registryWith(
      { id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run: toggle },
      { id: ActionIds.zoomIn, title: "Zoom In", category: "Board", repeatable: true, run: zoom },
    );
    const held = keydown({ key: "b", metaKey: true, repeat: true });
    expect(dispatchShortcut(held, registry, mac)).toBeNull();
    expect(held.prevented).toBe(true);
    expect(toggle).not.toHaveBeenCalled();
    expect(dispatchShortcut(keydown({ key: "=", metaKey: true, repeat: true }), registry, mac)).toBe(ActionIds.zoomIn);
    expect(dispatchShortcut(keydown({ key: "+", metaKey: true, shiftKey: true }), registry, mac)).toBe(ActionIds.zoomIn);
    expect(zoom).toHaveBeenCalledTimes(2);
  });

  it("prefers the latest registration and falls through disabled actions", () => {
    const shell = vi.fn();
    const board = vi.fn();
    let boardEnabled = true;
    const registry = createActionRegistry();
    registry.register({ id: "shell.thing", title: "Shell", category: "X", shortcuts: ["Mod+K"], run: shell });
    registry.register({ id: "board.thing", title: "Board", category: "X", shortcuts: ["Mod+K"], enabled: () => boardEnabled, run: board });
    dispatchShortcut(keydown({ key: "k", metaKey: true }), registry, mac);
    expect(board).toHaveBeenCalledTimes(1);
    boardEnabled = false;
    dispatchShortcut(keydown({ key: "k", metaKey: true }), registry, mac);
    expect(shell).toHaveBeenCalledTimes(1);
  });

  it("lets keys of disabled actions through", () => {
    const registry = registryWith({ id: ActionIds.compile, title: "Compile", category: "Graph", enabled: () => false, run: vi.fn() });
    const event = keydown({ key: "B", metaKey: true, shiftKey: true });
    expect(dispatchShortcut(event, registry, mac)).toBeNull();
    expect(event.prevented).toBe(false);
  });
});

describe("runMenuCommand", () => {
  it("runs the action a menu command maps to, and ignores unknown commands", () => {
    const run = vi.fn();
    const registry = registryWith({ id: ActionIds.toggleInspector, title: "Toggle Inspector", category: "View", run });
    expect(runMenuCommand("toggle_inspector", registry)).toBe(true);
    expect(runMenuCommand("undo", registry)).toBe(false);
    expect(runMenuCommand(42, registry)).toBe(false);
    expect(runMenuCommand("toggle_panel", registry)).toBe(false);
    expect(run).toHaveBeenCalledTimes(1);
  });
});
