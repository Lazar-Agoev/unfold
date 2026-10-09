// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { createKeyBindings, findConflicts, parseKeyBindings, serializeKeyBindings } from "./bindings";
import { dispatchShortcut, type DispatchableEvent } from "./dispatcher";
import { ActionIds } from "./keymap";
import { createActionRegistry } from "./registry";
import { chordIdentity, normalizeShortcut, shortcutFromEvent, shortcutLabel, type KeyLike } from "./shortcut";

function keydown(init: Partial<KeyLike> & { key: string }): DispatchableEvent {
  return {
    code: "",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    target: document.body,
    preventDefault: () => undefined,
    ...init,
  };
}

const mac = { platform: "darwin", modalOpen: false } as const;

describe("normalizeShortcut", () => {
  it("accepts any case and modifier order, and writes the keymap's spelling", () => {
    expect(normalizeShortcut("mod+shift+p")).toBe("Mod+Shift+P");
    expect(normalizeShortcut(" Shift + Mod + p ")).toBe("Mod+Shift+P");
    expect(normalizeShortcut("alt+arrowup")).toBe("Alt+ArrowUp");
    expect(normalizeShortcut("mod+plus")).toBe("Mod+Plus");
    expect(normalizeShortcut("]")).toBe("]");
    expect(normalizeShortcut("f12")).toBe("F12");
  });

  it("rejects what isn't one key with modifiers", () => {
    for (const text of ["", "Mod+", "Mod", "Hyper+K", "Mod+Mod+K", "Mod+Banana", "Shift+F99"]) {
      expect(normalizeShortcut(text)).toBeNull();
    }
  });
});

describe("shortcutFromEvent", () => {
  it("spells ⌘ as Mod on macOS and Ctrl as Mod elsewhere", () => {
    expect(shortcutFromEvent({ ...keydown({ key: "k", code: "KeyK", metaKey: true, shiftKey: true }) }, "darwin")).toBe(
      "Mod+Shift+K",
    );
    expect(shortcutFromEvent(keydown({ key: "k", code: "KeyK", ctrlKey: true }), "darwin")).toBe("Ctrl+K");
    expect(shortcutFromEvent(keydown({ key: "k", code: "KeyK", ctrlKey: true }), "win32")).toBe("Mod+K");
    expect(shortcutFromEvent(keydown({ key: "k", code: "KeyK", metaKey: true }), "win32")).toBeNull();
  });

  it("reads the physical key when ⌥ changes the character, and waits while only modifiers are down", () => {
    expect(shortcutFromEvent(keydown({ key: "∫", code: "KeyB", altKey: true, metaKey: true }), "darwin")).toBe("Mod+Alt+B");
    expect(shortcutFromEvent(keydown({ key: "!", code: "Digit1", shiftKey: true }), "darwin")).toBe("Shift+1");
    expect(shortcutFromEvent(keydown({ key: "Shift", code: "ShiftLeft", shiftKey: true }), "darwin")).toBeNull();
    expect(shortcutFromEvent(keydown({ key: "Meta", code: "MetaLeft", metaKey: true }), "darwin")).toBeNull();
  });
});

describe("parseKeyBindings", () => {
  it("keeps valid entries, normalized, and an empty string as no shortcut", () => {
    const overrides = parseKeyBindings({ [ActionIds.toggleSidebar]: "mod+k", [ActionIds.focusSearch]: "" });
    expect(serializeKeyBindings(overrides)).toEqual({ [ActionIds.focusSearch]: "", [ActionIds.toggleSidebar]: "Mod+K" });
  });

  it("ignores unknown ids, unparseable shortcuts, and junk without throwing", () => {
    expect(
      parseKeyBindings({ "nope.action": "Mod+K", [ActionIds.compile]: "Mod+Banana", [ActionIds.undo]: 7, [ActionIds.redo]: "Mod+" }).size,
    ).toBe(0);
    for (const raw of [null, undefined, "Mod+K", 42, ["Mod+K"]]) {
      expect(parseKeyBindings(raw).size).toBe(0);
    }
  });
});

describe("findConflicts", () => {
  it("finds none in the defaults", () => {
    expect(findConflicts(new Map(), "darwin").size).toBe(0);
    expect(findConflicts(new Map(), "win32").size).toBe(0);
  });

  it("flags both actions that share a combination, by platform", () => {
    const conflicts = findConflicts(new Map([[ActionIds.compile, "Mod+B"]]), "darwin");
    expect(conflicts.get(ActionIds.compile)).toEqual([ActionIds.toggleSidebar]);
    expect(conflicts.get(ActionIds.toggleSidebar)).toEqual([ActionIds.compile]);
    // Ctrl+B is ⌃B on macOS (no clash) but the same chord as Mod+B elsewhere.
    expect(findConflicts(new Map([[ActionIds.compile, "Ctrl+B"]]), "darwin").size).toBe(0);
    expect(findConflicts(new Map([[ActionIds.compile, "Ctrl+B"]]), "linux").size).toBe(2);
    expect(chordIdentity("win32", "Ctrl+B")).toBe(chordIdentity("win32", "Mod+B"));
  });

  it("clears a clash when the other action is unbound", () => {
    const overrides = new Map([
      [ActionIds.compile, "Mod+B"],
      [ActionIds.toggleSidebar, ""],
    ] as const);
    expect(findConflicts(overrides, "darwin").size).toBe(0);
  });
});

describe("registry with key bindings", () => {
  it("dispatches by the override instead of the default, and re-resolves when bindings change", () => {
    const bindings = createKeyBindings();
    const registry = createActionRegistry({ bindings });
    const run = vi.fn();
    registry.register({ id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run });
    const listener = vi.fn();
    registry.subscribe(listener);

    bindings.set({ [ActionIds.toggleSidebar]: "mod+k" });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(registry.get(ActionIds.toggleSidebar)?.shortcuts).toEqual(["Mod+K"]);
    expect(dispatchShortcut(keydown({ key: "b", metaKey: true }), registry, mac)).toBeNull();
    expect(dispatchShortcut(keydown({ key: "k", metaKey: true }), registry, mac)).toBe(ActionIds.toggleSidebar);

    bindings.set({ [ActionIds.toggleSidebar]: "Mod+K" });
    expect(listener).toHaveBeenCalledTimes(1);

    bindings.set({});
    expect(dispatchShortcut(keydown({ key: "b", metaKey: true }), registry, mac)).toBe(ActionIds.toggleSidebar);
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("lets a binding win over a view's own shortcuts, and unbinds with an empty string", () => {
    const bindings = createKeyBindings();
    const registry = createActionRegistry({ bindings });
    registry.register({ id: ActionIds.toggleInspector, title: "Toggle Inspector", category: "View", shortcuts: ["Mod+I"], run: vi.fn() });
    bindings.set({ [ActionIds.toggleInspector]: "" });
    expect(registry.get(ActionIds.toggleInspector)?.shortcuts).toEqual([]);
    expect(dispatchShortcut(keydown({ key: "i", metaKey: true }), registry, mac)).toBeNull();
    expect(dispatchShortcut(keydown({ key: "]" }), registry, mac)).toBeNull();
  });

  it("answers effective shortcuts for unregistered ids (tooltips before a view mounts)", () => {
    const bindings = createKeyBindings();
    const registry = createActionRegistry({ bindings });
    expect(registry.shortcutsFor(ActionIds.fitBoard)).toEqual(["Shift+1"]);
    bindings.set({ [ActionIds.fitBoard]: "Shift+F" });
    expect(registry.shortcutsFor(ActionIds.fitBoard)).toEqual(["Shift+F"]);
    expect(shortcutLabel("darwin", registry.shortcutsFor(ActionIds.fitBoard)[0] ?? "")).toBe("⇧F");
  });
});
