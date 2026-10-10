/**
 * Shortcuts: parsing, matching key events, and labels in the platform's style.
 *
 * A shortcut is written as `+`-separated parts, modifiers first: `Mod+Shift+P`, `Alt+Mod+B`, `]`,
 * `Shift+1`, `Mod+Plus`. `Mod` is ⌘ on macOS and Ctrl elsewhere; `Ctrl` is the Control key on every
 * platform; `Alt` is ⌥ on macOS. `Plus` names the `+` key and `Space` the space bar.
 *
 * Shortcuts in code are parsed strictly (`parseShortcut`); shortcuts a person wrote or recorded go
 * through `normalizeShortcut`, which is lenient about case and returns null instead of throwing.
 */

import type { Platform } from "./platform";

export interface Chord {
  readonly mod: boolean;
  readonly ctrl: boolean;
  readonly alt: boolean;
  readonly shift: boolean;
  /** Lowercase for letters; `event.key` spelling for named keys (`Escape`, `ArrowUp`, `F2`). */
  readonly key: string;
}

const MODIFIERS = new Set(["Mod", "Ctrl", "Alt", "Shift"]);

/** Parses a shortcut. Throws on a malformed one: shortcuts are written in code, never by users. */
export function parseShortcut(shortcut: string): Chord {
  const parts = shortcut.split("+");
  const key = parts.at(-1);
  if (key === undefined || key === "" || MODIFIERS.has(key)) {
    throw new Error(`Shortcut "${shortcut}" has no key`);
  }
  const modifiers = new Set(parts.slice(0, -1));
  for (const modifier of modifiers) {
    if (!MODIFIERS.has(modifier)) {
      throw new Error(`Shortcut "${shortcut}" has an unknown modifier "${modifier}"`);
    }
  }
  return {
    mod: modifiers.has("Mod"),
    ctrl: modifiers.has("Ctrl"),
    alt: modifiers.has("Alt"),
    shift: modifiers.has("Shift"),
    key: key === "Plus" ? "+" : key === "Space" ? " " : key.length === 1 ? key.toLowerCase() : key,
  };
}

/** Named keys a person's shortcut may use, by lowercase spelling. */
const NAMED_KEYS: ReadonlyMap<string, string> = new Map(
  [
    "Plus",
    "Space",
    "Escape",
    "Enter",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    ...Array.from({ length: 24 }, (_, index) => `F${String(index + 1)}`),
  ].map((name) => [name.toLowerCase(), name]),
);

/** Modifiers in the order a normalized shortcut writes them. */
const MODIFIER_ORDER = ["Mod", "Ctrl", "Alt", "Shift"] as const;

/**
 * Normalizes a shortcut a person wrote (`mod+shift+p`, `Shift+Mod+P`) to the keymap's spelling
 * (`Mod+Shift+P`): modifiers in a fixed order, letters upper-case, named keys in their canonical case.
 * Returns null for anything that isn't one key with optional modifiers.
 */
export function normalizeShortcut(text: string): string | null {
  const parts = text.trim().split("+").map((part) => part.trim());
  const rawKey = parts.at(-1);
  if (rawKey === undefined) {
    return null;
  }
  const modifiers = new Set<string>();
  for (const part of parts.slice(0, -1)) {
    const modifier = MODIFIER_ORDER.find((name) => name.toLowerCase() === part.toLowerCase());
    if (modifier === undefined || modifiers.has(modifier)) {
      return null;
    }
    modifiers.add(modifier);
  }
  let key: string | undefined;
  if (/^\S$/u.test(rawKey)) {
    key = rawKey.toUpperCase();
  } else {
    key = NAMED_KEYS.get(rawKey.toLowerCase());
  }
  if (key === undefined) {
    return null;
  }
  return [...MODIFIER_ORDER.filter((modifier) => modifiers.has(modifier)), key].join("+");
}

/**
 * Punctuation that Shift turns into another character (`]` into `}` on US layouts). With Shift, these
 * match and record by physical key, so `Mod+Shift+]` works on every layout.
 */
const PHYSICAL_KEYS: ReadonlyMap<string, string> = new Map([
  ["[", "BracketLeft"],
  ["]", "BracketRight"],
  ["=", "Equal"],
  ["-", "Minus"],
  [",", "Comma"],
]);
const KEYS_BY_CODE: ReadonlyMap<string, string> = new Map([...PHYSICAL_KEYS].map(([key, code]) => [code, key]));

/** What a recorded key event reads (so tests can pass plain objects). */
export type RecordableKey = KeyLike;

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "AltGraph", "Meta", "OS", "CapsLock", "Fn", "FnLock", "Hyper", "Super"]);

/** The key part of a recorded event, read the same way `keyMatches` matches it. */
function recordedKey(event: RecordableKey): string | null {
  const typed = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const letter = /^Key([A-Z])$/.exec(event.code)?.[1];
  const digit = /^Digit([0-9])$/.exec(event.code)?.[1];
  if (/^[a-z]$/.test(typed)) {
    return typed.toUpperCase();
  }
  if (letter !== undefined && !/^[0-9]$/.test(typed)) {
    return letter;
  }
  if (/^[0-9]$/.test(typed)) {
    return typed;
  }
  if (digit !== undefined) {
    return digit;
  }
  if (event.key === " ") {
    return "Space";
  }
  if (event.key === "+") {
    return "Plus";
  }
  const physical = KEYS_BY_CODE.get(event.code);
  if (event.shiftKey && physical !== undefined) {
    return physical;
  }
  if (event.key.length === 1) {
    return event.key.toUpperCase();
  }
  return NAMED_KEYS.get(event.key.toLowerCase()) ?? null;
}

/**
 * The shortcut a key event spells on this platform (`Mod+Shift+K`), or null while only modifiers are
 * down or for keys a shortcut can't name (dead keys, the Windows key).
 */
export function shortcutFromEvent(event: RecordableKey, platform: Platform): string | null {
  if (MODIFIER_KEYS.has(event.key) || event.key === "Dead" || event.key === "Unidentified" || event.key === "Process") {
    return null;
  }
  const mac = platform === "darwin";
  if (!mac && event.metaKey) {
    return null;
  }
  const key = recordedKey(event);
  if (key === null) {
    return null;
  }
  const modifiers = [
    (mac ? event.metaKey : event.ctrlKey) ? "Mod" : null,
    mac && event.ctrlKey ? "Ctrl" : null,
    event.altKey ? "Alt" : null,
    event.shiftKey ? "Shift" : null,
  ];
  return [...modifiers.filter((modifier) => modifier !== null), key].join("+");
}

/**
 * A string that is equal for two shortcuts exactly when they match the same key events on this
 * platform (`Ctrl+B` and `Mod+B` are the same chord outside macOS). For conflict checks.
 */
export function chordIdentity(platform: Platform, shortcut: string): string {
  const chord = parseShortcut(shortcut);
  const mac = platform === "darwin";
  const mod = chord.mod || (!mac && chord.ctrl);
  const ctrl = mac && chord.ctrl;
  return `${mod ? "M" : ""}${ctrl ? "C" : ""}${chord.alt ? "A" : ""}${chord.shift ? "S" : ""}:${chord.key}`;
}

/** The subset of a keyboard event that matching reads (so tests can pass plain objects). */
export type KeyLike = Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * Letters and digits match by the character typed, so shortcuts follow the user's layout; when a
 * modifier changes the character (⌥ on macOS, Shift on digits, a non-Latin layout), they fall back to
 * the physical key. So do `[ ] = - ,` when Shift changes them.
 */
function keyMatches(event: KeyLike, key: string): boolean {
  const typed = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (/^[a-z]$/.test(key)) {
    return /^[a-z]$/.test(typed) ? typed === key : event.code === `Key${key.toUpperCase()}`;
  }
  if (/^[0-9]$/.test(key)) {
    return /^[0-9]$/.test(typed) ? typed === key : event.code === `Digit${key}`;
  }
  const code = PHYSICAL_KEYS.get(key);
  if (code !== undefined && event.shiftKey && typed !== key) {
    return event.code === code;
  }
  return typed === key;
}

/** Whether a key event is this chord on this platform. Modifiers must match exactly. */
export function matchesChord(event: KeyLike, chord: Chord, platform: Platform): boolean {
  const mac = platform === "darwin";
  const wantMeta = mac && chord.mod;
  const wantCtrl = mac ? chord.ctrl : chord.mod || chord.ctrl;
  return (
    event.metaKey === wantMeta &&
    event.ctrlKey === wantCtrl &&
    event.altKey === chord.alt &&
    event.shiftKey === chord.shift &&
    keyMatches(event, chord.key)
  );
}

/** Whether the chord uses ⌘/Ctrl, which typing never does (so it may fire inside text fields). */
export function hasCommandModifier(chord: Chord): boolean {
  return chord.mod || chord.ctrl;
}

const KEY_LABELS: Readonly<Record<string, string>> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Escape: "Esc",
  Enter: "↵",
  Backspace: "⌫",
  Delete: "Del",
  " ": "Space",
};

function keyLabel(key: string): string {
  return KEY_LABELS[key] ?? (key.length === 1 ? key.toUpperCase() : key);
}

/** Formats a shortcut: `⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere. */
export function shortcutLabel(platform: Platform, shortcut: string): string {
  const chord = parseShortcut(shortcut);
  const key = keyLabel(chord.key);
  if (platform === "darwin") {
    // macOS orders modifier symbols ⌃⌥⇧⌘.
    return `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}${chord.shift ? "⇧" : ""}${chord.mod ? "⌘" : ""}${key}`;
  }
  const parts = [chord.mod || chord.ctrl ? "Ctrl" : null, chord.alt ? "Alt" : null, chord.shift ? "Shift" : null];
  return [...parts.filter((part) => part !== null), key].join("+");
}

/** Spells a shortcut out for screen readers, which read ⌘ as "place of interest sign". */
export function shortcutAriaLabel(platform: Platform, shortcut: string): string {
  const chord = parseShortcut(shortcut);
  const mac = platform === "darwin";
  const words = [
    chord.ctrl || (!mac && chord.mod) ? "Control" : null,
    chord.alt ? (mac ? "Option" : "Alt") : null,
    chord.shift ? "Shift" : null,
    mac && chord.mod ? "Command" : null,
  ];
  const key = chord.key === "+" ? "Plus" : keyLabel(chord.key);
  return [...words.filter((word) => word !== null), key].join(" ");
}

/** Whether keys typed at `target` edit text (so plain-key shortcuts must not fire). */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) {
    return true;
  }
  if (target instanceof HTMLInputElement) {
    return !["button", "checkbox", "radio", "range", "reset", "submit", "color", "file"].includes(target.type);
  }
  return false;
}
