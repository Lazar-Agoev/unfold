// @vitest-environment jsdom
import "../test/dom";
import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "../palette/CommandPalette";
import { closePalette, openPalette } from "../palette/store";
import { createKeyBindings } from "./bindings";
import { useShortcutLabel } from "./hooks";
import { ActionIds } from "./keymap";
import { createActionRegistry, type ActionRegistry } from "./registry";

afterEach(() => {
  act(() => {
    closePalette();
  });
});

function Label({ id, registry }: { readonly id: string; readonly registry: ActionRegistry }) {
  return <span data-testid="label">{useShortcutLabel(id, registry) ?? "none"}</span>;
}

describe("labels follow key bindings", () => {
  it("updates a tooltip-style label when the binding changes, even for an unregistered action", () => {
    const bindings = createKeyBindings();
    const registry = createActionRegistry({ bindings });
    render(<Label id={ActionIds.compile} registry={registry} />);
    const label = screen.getByTestId("label");
    // jsdom has no platform, so labels use the Ctrl style.
    expect(label.textContent).toBe("Ctrl+Shift+B");
    act(() => {
      bindings.set({ [ActionIds.compile]: "Mod+R" });
    });
    expect(label.textContent).toBe("Ctrl+R");
    act(() => {
      bindings.set({ [ActionIds.compile]: "" });
    });
    expect(label.textContent).toBe("none");
  });

  it("shows the effective shortcut in the command palette", () => {
    const bindings = createKeyBindings();
    bindings.set({ [ActionIds.toggleSidebar]: "Alt+Shift+S" });
    const registry = createActionRegistry({ bindings });
    registry.register({ id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run: vi.fn() });
    render(<CommandPalette registry={registry} />);
    act(() => {
      openPalette("commands");
    });
    expect(screen.getByRole("option").querySelector("kbd")?.textContent).toBe("Alt+Shift+S");
  });
});
