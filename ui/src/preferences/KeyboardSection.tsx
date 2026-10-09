/**
 * Preferences → Keyboard: every rebindable action, grouped by category, with its effective shortcut.
 * A row's shortcut button records a new combination (Escape cancels, Backspace clears it to none);
 * Reset returns a row to the keymap default, Reset All every row. Two actions on one combination are
 * flagged, and the dialog won't save until that's resolved.
 *
 * The section edits a draft of the overrides; the dialog saves them with `set_preferences`.
 */

import { useState, type KeyboardEvent } from "react";
import { Button, Kbd } from "../components";
import {
  ACTION_INFO,
  DEFAULT_SHORTCUTS,
  effectiveShortcuts,
  shortcutFromEvent,
  type KeyBindingOverrides,
  type Platform,
  type WellKnownActionId,
} from "../keyboard";

/** The rebindable actions by category, in the keymap's display order. */
const GROUPS: readonly { readonly category: string; readonly ids: readonly WellKnownActionId[] }[] = (() => {
  const groups: { category: string; ids: WellKnownActionId[] }[] = [];
  for (const [id, info] of Object.entries(ACTION_INFO) as [WellKnownActionId, (typeof ACTION_INFO)[WellKnownActionId]][]) {
    let group = groups.find((entry) => entry.category === info.category);
    if (!group) {
      group = { category: info.category, ids: [] };
      groups.push(group);
    }
    group.ids.push(id);
  }
  return groups;
})();

function sameShortcuts(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

/** The overrides with `id` bound to `shortcut` (`""` for none); binding the default drops the override. */
export function withBinding(
  overrides: KeyBindingOverrides,
  id: WellKnownActionId,
  shortcut: string,
): Map<WellKnownActionId, string> {
  const next = new Map(overrides);
  if (sameShortcuts(shortcut === "" ? [] : [shortcut], DEFAULT_SHORTCUTS[id])) {
    next.delete(id);
  } else {
    next.set(id, shortcut);
  }
  return next;
}

const hasModifier = (event: KeyboardEvent) => event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;

export interface KeyboardSectionProps {
  readonly overrides: KeyBindingOverrides;
  readonly conflicts: ReadonlyMap<WellKnownActionId, readonly WellKnownActionId[]>;
  readonly platform: Platform;
  readonly onChange: (next: KeyBindingOverrides) => void;
}

export function KeyboardSection({ overrides, conflicts, platform, onChange }: KeyboardSectionProps) {
  const [recording, setRecording] = useState<WellKnownActionId | null>(null);

  const record = (id: WellKnownActionId, event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "Tab" && !hasModifier(event)) {
      // Let focus move on; leaving the button stops recording.
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !hasModifier(event)) {
      setRecording(null);
      return;
    }
    if (event.key === "Backspace" && !hasModifier(event)) {
      onChange(withBinding(overrides, id, ""));
      setRecording(null);
      return;
    }
    const shortcut = shortcutFromEvent(event, platform);
    if (shortcut !== null) {
      onChange(withBinding(overrides, id, shortcut));
      setRecording(null);
    }
  };

  return (
    <section className="preferences-keys" aria-labelledby="preferences-keys-title">
      <div className="preferences-row">
        <h3 className="preferences-label preferences-heading" id="preferences-keys-title">
          Keyboard
        </h3>
        <Button
          size="small"
          variant="quiet"
          disabled={overrides.size === 0}
          onClick={() => {
            onChange(new Map());
          }}
        >
          Reset All
        </Button>
      </div>
      <div className="preferences-keys-list">
        {GROUPS.map(({ category, ids }) => (
          <div key={category} role="group" aria-label={category} className="preferences-keys-group">
            <div className="preferences-keys-category">{category}</div>
            {ids.map((id) => {
              const { title } = ACTION_INFO[id];
              const shortcuts = effectiveShortcuts(overrides, id);
              const clashes = conflicts.get(id) ?? [];
              const isRecording = recording === id;
              return (
                <div key={id} className="preferences-key" data-action={id}>
                  <span className="preferences-key-title">
                    {title}
                    {clashes.length > 0 ? (
                      <span className="preferences-key-conflict" role="alert">
                        Also used by {clashes.map((other) => ACTION_INFO[other].title).join(", ")}
                      </span>
                    ) : null}
                  </span>
                  <button
                    type="button"
                    className="preferences-key-record"
                    aria-label={`Shortcut for ${title}`}
                    aria-pressed={isRecording}
                    aria-invalid={clashes.length > 0 || undefined}
                    onClick={() => {
                      setRecording(id);
                    }}
                    onBlur={() => {
                      setRecording((current) => (current === id ? null : current));
                    }}
                    onKeyDown={
                      isRecording
                        ? (event) => {
                            record(id, event);
                          }
                        : undefined
                    }
                  >
                    {isRecording ? (
                      <span className="preferences-key-prompt">Press keys… Esc cancels, ⌫ clears</span>
                    ) : shortcuts.length === 0 ? (
                      <span className="preferences-key-none">None</span>
                    ) : (
                      shortcuts.map((shortcut) => <Kbd key={shortcut} shortcut={shortcut} />)
                    )}
                  </button>
                  <Button
                    size="small"
                    variant="quiet"
                    aria-label={`Reset ${title}`}
                    disabled={!overrides.has(id)}
                    onClick={() => {
                      const next = new Map(overrides);
                      next.delete(id);
                      onChange(next);
                    }}
                  >
                    Reset
                  </Button>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </section>
  );
}
