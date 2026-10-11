// @vitest-environment jsdom
import "../test/dom";
import { act, waitFor } from "@testing-library/react";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { Preferences } from "../ipc/generated";
import { installEngineInvalidation } from "../ipc/queries";
import { ActionIds, createKeyBindings } from "../keyboard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { usePreferencesKeyBindings, usePreferencesTheme } from "./PreferencesDialog";

const PREFERENCES: Preferences = {
  theme: "light",
  editorCommand: null,
  backgroundUpdates: true,
  threadLimit: null,
  keyBindings: {},
  updates: { enabled: true },
  agents: { enabled: false, port: 29180 },
};

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
  delete document.documentElement.dataset["theme"];
});

/** What the shell wires up in every window, folder or welcome. */
function Window({ bindings }: { readonly bindings: ReturnType<typeof createKeyBindings> }) {
  const client = useQueryClient();
  useEffect(() => installEngineInvalidation(client), [client]);
  usePreferencesTheme();
  usePreferencesKeyBindings(bindings);
  return null;
}

describe("preferences saved in another window", () => {
  it("apply the new theme and key bindings from the event, without refetching", async () => {
    const fake = installFakeEngine({ handlers: { get_preferences: () => PREFERENCES } });
    engine = fake;
    const bindings = createKeyBindings();
    renderWithClient(<Window bindings={bindings} />);
    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("light");
    });
    expect(bindings.override(ActionIds.compile)).toBeUndefined();

    act(() => {
      fake.emit({
        type: "preferences_changed",
        preferences: { ...PREFERENCES, theme: "dark", keyBindings: { [ActionIds.compile]: "mod+r" } },
      });
    });
    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBe("dark");
    });
    expect(bindings.override(ActionIds.compile)).toEqual(["Mod+R"]);

    // Back to following the OS, and the override removed.
    act(() => {
      fake.emit({ type: "preferences_changed", preferences: { ...PREFERENCES, theme: "system" } });
    });
    await waitFor(() => {
      expect(document.documentElement.dataset["theme"]).toBeUndefined();
    });
    expect(bindings.override(ActionIds.compile)).toBeUndefined();
    expect(fake.callsOf("get_preferences")).toHaveLength(1);
  });
});
