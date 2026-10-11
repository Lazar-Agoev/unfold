// @vitest-environment jsdom
import "../test/dom";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { Preferences } from "../ipc/generated";
import { ActionIds, createKeyBindings } from "../keyboard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { mergeKeyBindings, PreferencesDialog, usePreferencesKeyBindings } from "./PreferencesDialog";

const PREFERENCES: Preferences = {
  theme: "system",
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
});

async function open(keyBindings: Record<string, string> = {}) {
  const fake = installFakeEngine({
    handlers: {
      get_preferences: () => ({ ...PREFERENCES, keyBindings }),
      set_preferences: ({ preferences }) => preferences,
    },
  });
  engine = fake;
  renderWithClient(<PreferencesDialog onClose={() => undefined} />);
  const dialog = await screen.findByRole("dialog");
  return { fake, dialog, user: userEvent.setup() };
}

const shortcutButton = (dialog: HTMLElement, title: string) => within(dialog).getByRole("button", { name: `Shortcut for ${title}` });
const saveButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Save" });

describe("Preferences → Keyboard", () => {
  it("lists actions by category with their effective shortcuts", async () => {
    const { dialog } = await open({ [ActionIds.toggleSidebar]: "Mod+K", "future.action": "Mod+Q" });
    const view = within(dialog).getByRole("group", { name: "View" });
    expect(within(view).getByRole("button", { name: "Shortcut for Toggle Sidebar" }).textContent).toBe("Ctrl+K");
    expect(shortcutButton(dialog, "Next Tab").textContent).toBe("Ctrl+TabCtrl+Shift+]");
    expect(shortcutButton(dialog, "Copy Diagnostics").textContent).toBe("None");
    expect(within(dialog).getByRole("button", { name: "Reset Toggle Sidebar" })).toHaveProperty("disabled", false);
    expect(within(dialog).getByRole("button", { name: "Reset Compile" })).toHaveProperty("disabled", true);
  });

  it("records a combination, and saves it with entries it doesn't know carried over", async () => {
    const { dialog, fake, user } = await open({ "future.action": "Mod+Q" });
    await user.click(shortcutButton(dialog, "Compile"));
    expect(shortcutButton(dialog, "Compile").textContent).toContain("Press keys");
    // Modifiers alone keep recording.
    await user.keyboard("{Control>}{Alt>}r{/Alt}{/Control}");
    expect(shortcutButton(dialog, "Compile").textContent).toBe("Ctrl+Alt+R");
    await user.click(saveButton(dialog));
    await waitFor(() => {
      expect(fake.callsOf("set_preferences")).toHaveLength(1);
    });
    const [args] = fake.callsOf("set_preferences");
    expect((args?.["preferences"] as Preferences).keyBindings).toEqual({
      "future.action": "Mod+Q",
      [ActionIds.compile]: "Mod+Alt+R",
    });
  });

  it("cancels recording on Escape without closing the dialog, and clears with Backspace", async () => {
    const { dialog, user } = await open();
    await user.click(shortcutButton(dialog, "Home"));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(shortcutButton(dialog, "Home").textContent).toBe("Ctrl+0");
    await user.click(shortcutButton(dialog, "Home"));
    await user.keyboard("{Backspace}");
    expect(shortcutButton(dialog, "Home").textContent).toBe("None");
  });

  it("flags a combination used twice and won't save until it's resolved", async () => {
    const { dialog, user } = await open();
    await user.click(shortcutButton(dialog, "Compile"));
    await user.keyboard("{Control>}b{/Control}");
    // Toggle Sidebar (View) is listed before Compile (Project).
    expect(within(dialog).getAllByRole("alert").map((alert) => alert.textContent)).toEqual([
      "Also used by Compile",
      "Also used by Toggle Sidebar",
    ]);
    expect(saveButton(dialog)).toHaveProperty("disabled", true);
    await user.click(within(dialog).getByRole("button", { name: "Reset Compile" }));
    expect(within(dialog).queryAllByRole("alert")).toHaveLength(0);
    expect(saveButton(dialog)).toHaveProperty("disabled", false);
  });

  it("resets every row with Reset All", async () => {
    const { dialog, fake, user } = await open({ [ActionIds.undo]: "Mod+U", [ActionIds.redo]: "" });
    expect(shortcutButton(dialog, "Redo").textContent).toBe("None");
    await user.click(within(dialog).getByRole("button", { name: "Reset All" }));
    expect(shortcutButton(dialog, "Undo").textContent).toBe("Ctrl+Z");
    expect(shortcutButton(dialog, "Redo").textContent).toBe("Ctrl+Shift+Z");
    await user.click(saveButton(dialog));
    await waitFor(() => {
      expect(fake.callsOf("set_preferences")).toHaveLength(1);
    });
    expect((fake.callsOf("set_preferences")[0]?.["preferences"] as Preferences).keyBindings).toEqual({});
  });

  it("drops an override that records the default again", async () => {
    const { dialog, user } = await open({ [ActionIds.toggleSidebar]: "Mod+K" });
    await user.click(shortcutButton(dialog, "Toggle Sidebar"));
    await user.keyboard("{Control>}b{/Control}");
    expect(within(dialog).getByRole("button", { name: "Reset Toggle Sidebar" })).toHaveProperty("disabled", true);
  });
});

describe("applying stored key bindings", () => {
  function Apply({ bindings }: { readonly bindings: ReturnType<typeof createKeyBindings> }) {
    usePreferencesKeyBindings(bindings);
    return null;
  }

  it("applies the saved bindings once preferences load, ignoring invalid entries", async () => {
    engine = installFakeEngine({
      handlers: {
        get_preferences: () => ({
          ...PREFERENCES,
          keyBindings: { [ActionIds.compile]: "mod+r", [ActionIds.undo]: "Mod+Nope", "x.y": "Mod+X" },
        }),
      },
    });
    const bindings = createKeyBindings();
    renderWithClient(<Apply bindings={bindings} />);
    await waitFor(() => {
      expect(bindings.override(ActionIds.compile)).toEqual(["Mod+R"]);
    });
    expect(bindings.override(ActionIds.undo)).toBeUndefined();
    expect(bindings.overrides().size).toBe(1);
  });

  it("merges edits over the stored map without losing unknown ids", () => {
    expect(mergeKeyBindings({ "x.y": "Mod+X", [ActionIds.undo]: "Mod+U" }, new Map([[ActionIds.redo, ""]]))).toEqual({
      "x.y": "Mod+X",
      [ActionIds.redo]: "",
    });
  });
});
