import { describe, expect, it } from "vitest";
import { matchesChord, parseShortcut, shortcutAriaLabel, shortcutLabel, type KeyLike } from "./shortcut";

const key = (init: Partial<KeyLike> & { key: string }): KeyLike => ({
  code: "",
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...init,
});

describe("parseShortcut", () => {
  it("reads modifiers and the key", () => {
    expect(parseShortcut("Alt+Mod+B")).toEqual({ mod: true, ctrl: false, alt: true, shift: false, key: "b" });
    expect(parseShortcut("Mod+Plus").key).toBe("+");
    expect(parseShortcut("]")).toMatchObject({ mod: false, key: "]" });
    expect(parseShortcut("Mod+ArrowUp").key).toBe("ArrowUp");
  });

  it("rejects malformed shortcuts", () => {
    expect(() => parseShortcut("Mod+")).toThrow();
    expect(() => parseShortcut("Hyper+K")).toThrow();
    expect(() => parseShortcut("Shift")).toThrow();
  });
});

describe("matchesChord", () => {
  it("uses ⌘ on macOS and Ctrl elsewhere for Mod", () => {
    const chord = parseShortcut("Mod+B");
    expect(matchesChord(key({ key: "b", metaKey: true }), chord, "darwin")).toBe(true);
    expect(matchesChord(key({ key: "b", ctrlKey: true }), chord, "darwin")).toBe(false);
    expect(matchesChord(key({ key: "b", ctrlKey: true }), chord, "win32")).toBe(true);
    expect(matchesChord(key({ key: "b", ctrlKey: true }), chord, "linux")).toBe(true);
    expect(matchesChord(key({ key: "b", metaKey: true }), chord, "linux")).toBe(false);
  });

  it("requires modifiers to match exactly", () => {
    const chord = parseShortcut("Mod+B");
    expect(matchesChord(key({ key: "b", metaKey: true, altKey: true }), chord, "darwin")).toBe(false);
    expect(matchesChord(key({ key: "B", metaKey: true, shiftKey: true }), chord, "darwin")).toBe(false);
    expect(matchesChord(key({ key: "B", metaKey: true, shiftKey: true }), parseShortcut("Mod+Shift+B"), "darwin")).toBe(true);
  });

  it("falls back to the physical key when a modifier changes the character", () => {
    // ⌥⌘B types "∫" on a US Mac layout; Shift+1 types "!".
    expect(matchesChord(key({ key: "∫", code: "KeyB", metaKey: true, altKey: true }), parseShortcut("Alt+Mod+B"), "darwin")).toBe(true);
    expect(matchesChord(key({ key: "!", code: "Digit1", shiftKey: true }), parseShortcut("Shift+1"), "darwin")).toBe(true);
  });

  it("follows the layout's letters rather than the physical position", () => {
    // On AZERTY, the key labeled A sits where QWERTY has Q.
    expect(matchesChord(key({ key: "a", code: "KeyQ", ctrlKey: true }), parseShortcut("Mod+A"), "win32")).toBe(true);
    expect(matchesChord(key({ key: "a", code: "KeyQ", ctrlKey: true }), parseShortcut("Mod+Q"), "win32")).toBe(false);
  });

  it("matches punctuation by character", () => {
    expect(matchesChord(key({ key: "]" }), parseShortcut("]"), "darwin")).toBe(true);
    expect(matchesChord(key({ key: "}", shiftKey: true }), parseShortcut("]"), "darwin")).toBe(false);
    expect(matchesChord(key({ key: "+", shiftKey: true, metaKey: true }), parseShortcut("Mod+Shift+Plus"), "darwin")).toBe(true);
  });
});

describe("shortcutLabel", () => {
  it("uses symbols in macOS order", () => {
    expect(shortcutLabel("darwin", "Mod+Shift+P")).toBe("⇧⌘P");
    expect(shortcutLabel("darwin", "Alt+Mod+B")).toBe("⌥⌘B");
    expect(shortcutLabel("darwin", "Mod+1")).toBe("⌘1");
    expect(shortcutLabel("darwin", "Shift+1")).toBe("⇧1");
  });

  it("spells out Ctrl elsewhere", () => {
    expect(shortcutLabel("win32", "Mod+Shift+P")).toBe("Ctrl+Shift+P");
    expect(shortcutLabel("linux", "Mod+O")).toBe("Ctrl+O");
    expect(shortcutLabel("linux", "Alt+Mod+B")).toBe("Ctrl+Alt+B");
    expect(shortcutLabel("win32", "]")).toBe("]");
  });

  it("spells shortcuts out for screen readers", () => {
    expect(shortcutAriaLabel("darwin", "Mod+Shift+P")).toBe("Shift Command P");
    expect(shortcutAriaLabel("win32", "Alt+Mod+B")).toBe("Control Alt B");
    expect(shortcutAriaLabel("darwin", "Mod+Plus")).toBe("Command Plus");
  });
});
