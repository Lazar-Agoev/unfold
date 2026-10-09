/**
 * The shortcut dispatcher: one window-level keydown listener that finds the registered action for a
 * key and runs it.
 *
 * - Components that handle a key themselves (tree arrows, menu navigation) call `preventDefault`, and
 *   the dispatcher leaves such events alone.
 * - Plain-key shortcuts (`/`, `]`, `Shift+1`) never fire while typing in a text field; ⌘/Ctrl ones do
 *   unless the action opts out (undo and redo, so text fields keep their native undo).
 * - While a modal is open, nothing fires.
 * - A handled key is `preventDefault`ed, which also stops Electron from running the application
 *   menu's accelerator for it, so a key runs its action once.
 */

import { useEffect } from "react";
import { isModalOpen } from "./modal";
import { currentPlatform, type Platform } from "./platform";
import { actions as windowActions, type Action, type ActionRegistry } from "./registry";
import { hasCommandModifier, isEditableTarget, matchesChord, parseShortcut, type Chord, type KeyLike } from "./shortcut";

export interface DispatchContext {
  readonly platform: Platform;
  /** Whether a modal layer is open (defaults to the window's modal state). */
  readonly modalOpen?: boolean;
}

export type DispatchableEvent = KeyLike &
  Pick<KeyboardEvent, "target" | "repeat" | "isComposing" | "defaultPrevented" | "preventDefault">;

const chords = new Map<string, Chord>();

function chordOf(shortcut: string): Chord {
  let chord = chords.get(shortcut);
  if (!chord) {
    chord = parseShortcut(shortcut);
    chords.set(shortcut, chord);
  }
  return chord;
}

/**
 * Runs the action bound to this key event, if any. Returns the id of the action that ran, or null
 * when the event was left alone.
 */
export function dispatchShortcut(
  event: DispatchableEvent,
  registry: ActionRegistry,
  context: DispatchContext,
): string | null {
  if (event.defaultPrevented || event.isComposing || (context.modalOpen ?? isModalOpen())) {
    return null;
  }
  const typing = isEditableTarget(event.target);
  // Later registrations (the focused view's) win over earlier ones (the shell's).
  const candidates = [...registry.list()].sort((a: Action, b: Action) => b.order - a.order);
  for (const action of candidates) {
    for (const shortcut of action.shortcuts) {
      const chord = chordOf(shortcut);
      if (!matchesChord(event, chord, context.platform)) {
        continue;
      }
      if (typing && !(action.allowInInput ?? hasCommandModifier(chord))) {
        continue;
      }
      if (!(action.enabled?.() ?? true)) {
        continue;
      }
      event.preventDefault();
      if (event.repeat && !action.repeatable) {
        // Swallow auto-repeat so a held ⌘B doesn't flicker the sidebar (or reach the menu).
        return null;
      }
      registry.run(action.id);
      return action.id;
    }
  }
  return null;
}

/** Installs the window's dispatcher while mounted. */
export function useShortcutDispatcher(registry: ActionRegistry = windowActions): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      dispatchShortcut(event, registry, { platform: currentPlatform() });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [registry]);
}
