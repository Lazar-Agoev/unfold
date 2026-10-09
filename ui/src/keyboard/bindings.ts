/**
 * A person's key bindings: overrides of the keymap's default shortcuts, stored in the app
 * preferences (`Preferences.keyBindings`, `preferences.json`) as action id → shortcut.
 *
 * - The shortcut is written as in the keymap (`Mod+Shift+P`, case-insensitive); an empty string
 *   means the action has no shortcut. An override replaces all of the action's default shortcuts.
 * - Only the keymap's actions (`ActionIds`) can be rebound. Entries for other ids and shortcuts that
 *   don't parse are ignored, so a hand-edited preferences file never breaks the keyboard.
 *
 * The window's registry reads the bindings store, so the dispatcher, Add's commands, and tooltips
 * all follow the effective shortcut.
 */

import { DEFAULT_SHORTCUTS, defaultShortcuts, isWellKnownAction, type WellKnownActionId } from "./keymap";
import type { Platform } from "./platform";
import { chordIdentity, normalizeShortcut } from "./shortcut";

/** Validated overrides: action id → normalized shortcut, `""` for none. */
export type KeyBindingOverrides = ReadonlyMap<WellKnownActionId, string>;

/** Validates stored key bindings, dropping unknown action ids and unparseable shortcuts. */
export function parseKeyBindings(raw: unknown): Map<WellKnownActionId, string> {
  const overrides = new Map<WellKnownActionId, string>();
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return overrides;
  }
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!isWellKnownAction(id) || typeof value !== "string") {
      continue;
    }
    if (value.trim() === "") {
      overrides.set(id, "");
      continue;
    }
    const shortcut = normalizeShortcut(value);
    if (shortcut !== null) {
      overrides.set(id, shortcut);
    }
  }
  return overrides;
}

/** The shortcuts an override stands for. */
export function overrideShortcuts(override: string): readonly string[] {
  return override === "" ? [] : [override];
}

/** An action's effective shortcuts under these overrides. */
export function effectiveShortcuts(overrides: KeyBindingOverrides, id: string): readonly string[] {
  const override = isWellKnownAction(id) ? overrides.get(id) : undefined;
  return override === undefined ? defaultShortcuts(id) : overrideShortcuts(override);
}

/** Overrides in the stored shape, sorted by id. */
export function serializeKeyBindings(overrides: KeyBindingOverrides): Record<string, string> {
  return Object.fromEntries([...overrides].sort(([a], [b]) => a.localeCompare(b)));
}

/**
 * Every rebindable action whose effective shortcut is also another's, with the other actions that
 * share it. Empty when the bindings have no conflicts.
 */
export function findConflicts(overrides: KeyBindingOverrides, platform: Platform): Map<WellKnownActionId, WellKnownActionId[]> {
  const byChord = new Map<string, WellKnownActionId[]>();
  for (const id of Object.keys(DEFAULT_SHORTCUTS) as WellKnownActionId[]) {
    for (const shortcut of new Set(effectiveShortcuts(overrides, id).map((value) => chordIdentity(platform, value)))) {
      const ids = byChord.get(shortcut) ?? [];
      ids.push(id);
      byChord.set(shortcut, ids);
    }
  }
  const conflicts = new Map<WellKnownActionId, WellKnownActionId[]>();
  for (const ids of byChord.values()) {
    if (ids.length < 2) {
      continue;
    }
    for (const id of ids) {
      const others = conflicts.get(id) ?? [];
      others.push(...ids.filter((other) => other !== id && !others.includes(other)));
      conflicts.set(id, others);
    }
  }
  return conflicts;
}

/** The window's current overrides, observable. */
export interface KeyBindings {
  /** The override for `id`'s shortcuts, or undefined when it uses the defaults. */
  readonly override: (id: string) => readonly string[] | undefined;
  readonly overrides: () => KeyBindingOverrides;
  /** Replaces the overrides with validated stored bindings. Notifies only when something changed. */
  readonly set: (raw: unknown) => void;
  readonly subscribe: (listener: () => void) => () => void;
}

export function createKeyBindings(): KeyBindings {
  let current: KeyBindingOverrides = new Map();
  let signature = "{}";
  const resolved = new Map<string, readonly string[]>();
  const listeners = new Set<() => void>();
  return {
    override: (id) => {
      if (!isWellKnownAction(id)) {
        return undefined;
      }
      const value = current.get(id);
      if (value === undefined) {
        return undefined;
      }
      // Stable arrays, so registry snapshots compare equal until the bindings change.
      let shortcuts = resolved.get(id);
      if (!shortcuts) {
        shortcuts = overrideShortcuts(value);
        resolved.set(id, shortcuts);
      }
      return shortcuts;
    },
    overrides: () => current,
    set: (raw) => {
      const next = parseKeyBindings(raw);
      const nextSignature = JSON.stringify(serializeKeyBindings(next));
      if (nextSignature === signature) {
        return;
      }
      current = next;
      signature = nextSignature;
      resolved.clear();
      for (const listener of [...listeners]) {
        listener();
      }
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** The window's key bindings, applied from preferences by the shell. */
export const keyBindings: KeyBindings = createKeyBindings();
