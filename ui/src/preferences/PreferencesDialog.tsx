/**
 * Preferences (app-wide, `preferences.json` through Rust): theme, editor command, whether
 * background updates start on for newly compiled folders, the compile thread limit, the update
 * check, and key bindings. The theme applies at once through `data-theme`; everything saves with
 * `set_preferences`. Saved theme and key bindings apply on load and whenever the preferences change.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { create } from "zustand";
import { Button, Dialog, SegmentedControl, TextField, showToast } from "../components";
import { call } from "../ipc/commands";
import type { Preferences, Theme } from "../ipc/generated";
import { engineKey, engineQueryOptions, useEngineQuery } from "../ipc/queries";
import { toUnfoldError } from "../ipc/unfold";
import {
  findConflicts,
  isWellKnownAction,
  keyBindings,
  parseKeyBindings,
  serializeKeyBindings,
  usePlatform,
  type KeyBindingOverrides,
  type KeyBindings,
} from "../keyboard";
import { applyThemeOverride } from "../theme/theme";
import { KeyboardSection } from "./KeyboardSection";
import "./PreferencesDialog.css";

const usePreferencesDialog = create<{ readonly open: boolean }>()(() => ({ open: false }));

export function openPreferences(): void {
  usePreferencesDialog.setState({ open: true });
}

function closePreferences(): void {
  usePreferencesDialog.setState({ open: false });
}

/** Applies a theme preference to the document. */
export function applyTheme(theme: Theme): void {
  applyThemeOverride(theme === "system" ? null : theme);
}

/** Applies the saved theme as soon as preferences load, and again whenever they change. */
export function usePreferencesTheme(): void {
  const preferences = useEngineQuery("get_preferences", {});
  const theme = preferences.data?.theme;
  useEffect(() => {
    if (theme) {
      applyTheme(theme);
    }
  }, [theme]);
}

/**
 * Applies the saved key bindings as soon as preferences load, and again whenever they change (the
 * window's registry re-resolves every shortcut, so the dispatcher, palette, and tooltips follow).
 */
export function usePreferencesKeyBindings(bindings: KeyBindings = keyBindings): void {
  const preferences = useEngineQuery("get_preferences", {});
  const stored = preferences.data?.keyBindings;
  useEffect(() => {
    if (stored) {
      bindings.set(stored);
    }
  }, [stored, bindings]);
}

/**
 * The stored `keyBindings` after editing: the editor's overrides for the keymap's actions, plus any
 * entries for ids this version doesn't know, carried over untouched.
 */
export function mergeKeyBindings(saved: Readonly<Record<string, string>>, overrides: KeyBindingOverrides): Record<string, string> {
  const foreign = Object.entries(saved).filter(([id]) => !isWellKnownAction(id));
  return { ...Object.fromEntries(foreign), ...serializeKeyBindings(overrides) };
}

const THEMES = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
] as const satisfies readonly { value: Theme; label: string }[];

/** Parses the thread limit field: empty means automatic; otherwise a whole number from 1 to 256. */
export function parseThreadLimit(text: string): { ok: true; value: number | null } | { ok: false; message: string } {
  const trimmed = text.trim();
  if (trimmed === "") {
    return { ok: true, value: null };
  }
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value < 1 || value > 256) {
    return { ok: false, message: "A whole number from 1 to 256, or empty for automatic." };
  }
  return { ok: true, value };
}

function PreferencesForm({ saved, onDone }: { readonly saved: Preferences; readonly onDone: () => void }) {
  const client = useQueryClient();
  const [theme, setTheme] = useState<Theme>(saved.theme);
  const [editor, setEditor] = useState(saved.editorCommand ?? "");
  const [background, setBackground] = useState(saved.backgroundUpdates);
  const [threads, setThreads] = useState(saved.threadLimit === null ? "" : String(saved.threadLimit));
  const [checkUpdates, setCheckUpdates] = useState(saved.updates.enabled);
  const [bindings, setBindings] = useState<KeyBindingOverrides>(() => parseKeyBindings(saved.keyBindings));
  const [saving, setSaving] = useState(false);
  const platform = usePlatform();
  const limit = parseThreadLimit(threads);
  const conflicts = useMemo(() => findConflicts(bindings, platform), [bindings, platform]);
  const canSave = limit.ok && conflicts.size === 0;

  const save = async () => {
    if (!limit.ok || !canSave) {
      return;
    }
    setSaving(true);
    const preferences: Preferences = {
      ...saved,
      theme,
      editorCommand: editor.trim() === "" ? null : editor.trim(),
      backgroundUpdates: background,
      threadLimit: limit.value,
      updates: { ...saved.updates, enabled: checkUpdates },
      keyBindings: mergeKeyBindings(saved.keyBindings, bindings),
    };
    try {
      const stored = await call("set_preferences", { preferences });
      client.setQueryData(engineKey("get_preferences", {}), stored);
      onDone();
    } catch (error) {
      showToast({ message: `Couldn't save preferences: ${toUnfoldError(error).message}`, tone: "danger" });
    } finally {
      setSaving(false);
    }
  };

  const cancel = () => {
    applyTheme(saved.theme);
    onDone();
  };

  return (
    <Dialog
      open
      onClose={cancel}
      title="Preferences"
      width="medium"
      footer={
        <>
          <Button variant="quiet" onClick={cancel}>
            Cancel
          </Button>
          <Button variant="primary" busy={saving} disabled={!canSave || saving} onClick={() => void save()}>
            Save
          </Button>
        </>
      }
    >
      <div className="preferences">
        <div className="preferences-row">
          <span className="preferences-label" id="preferences-theme">
            Theme
          </span>
          <SegmentedControl<Theme>
            label="Theme"
            options={THEMES}
            value={theme}
            onChange={(next) => {
              setTheme(next);
              applyTheme(next);
            }}
          />
        </div>
        <TextField
          label="Editor command"
          placeholder="code -g {path}:{line}"
          value={editor}
          onChange={(event) => {
            setEditor(event.target.value);
          }}
          hint="Runs without a shell. {path}, {line}, and {column} are replaced. Empty opens files with the system's default app."
        />
        <label className="preferences-check">
          <input
            type="checkbox"
            checked={background}
            onChange={(event) => {
              setBackground(event.target.checked);
            }}
          />
          <span>
            Update the graph in the background as files change
            <span className="preferences-note">For folders compiled from now on. Each folder remembers its own switch.</span>
          </span>
        </label>
        <TextField
          label="Compile threads"
          inputMode="numeric"
          placeholder="Automatic (every core but one)"
          value={threads}
          error={limit.ok ? null : limit.message}
          onChange={(event) => {
            setThreads(event.target.value);
          }}
          hint="Applies to folders opened afterwards."
        />
        <label className="preferences-check">
          <input
            type="checkbox"
            checked={checkUpdates}
            onChange={(event) => {
              setCheckUpdates(event.target.checked);
            }}
          />
          <span>
            Check for new versions of Unfold
            <span className="preferences-note">The only time Unfold uses the network. Nothing about you or your code is sent.</span>
          </span>
        </label>
        <KeyboardSection overrides={bindings} conflicts={conflicts} platform={platform} onChange={setBindings} />
      </div>
    </Dialog>
  );
}

/**
 * Re-reads the preferences each time the dialog opens, so it never edits a stale copy (another window
 * may have saved since); the refreshed data also re-applies this window's theme and key bindings.
 */
function FreshPreferences() {
  const preferences = useQuery({ ...engineQueryOptions("get_preferences", {}), refetchOnMount: "always" });
  if (!preferences.isFetchedAfterMount || !preferences.data) {
    return null;
  }
  return <PreferencesForm saved={preferences.data} onDone={closePreferences} />;
}

export function PreferencesDialog() {
  const open = usePreferencesDialog((state) => state.open);
  return open ? <FreshPreferences /> : null;
}
