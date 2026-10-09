// @vitest-environment jsdom
import "../test/dom";
import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createKeyBindings } from "./bindings";
import { useActions, useShortcutLabel } from "./hooks";
import { ActionIds } from "./keymap";
import { createActionRegistry, type ActionRegistry } from "./registry";

function Label({ id, registry }: { readonly id: string; readonly registry: ActionRegistry }) {
  return <span data-testid="label">{useShortcutLabel(id, registry) ?? "none"}</span>;
}

/** What Add's `>` mode lists: actions shown in Add, each with its shortcut. */
function Commands({ registry }: { readonly registry: ActionRegistry }) {
  const actions = useActions(registry).filter((action) => action.showInAdd !== false);
  return (
    <ul>
      {actions.map((action) => (
        <li key={action.id}>{`${action.title}: ${action.shortcuts[0] ?? "none"}`}</li>
      ))}
    </ul>
  );
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

  it("lists registered actions with their effective shortcut, as Add's commands show them", () => {
    const bindings = createKeyBindings();
    bindings.set({ [ActionIds.toggleSidebar]: "Alt+Shift+S" });
    const registry = createActionRegistry({ bindings });
    registry.register([
      { id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run: vi.fn() },
      { id: "board.foldAll", title: "Fold All Cards", category: "Board", showInAdd: true, run: vi.fn() },
      { id: "peek.close", title: "Close Peek", category: "Go", showInAdd: false, run: vi.fn() },
    ]);
    render(<Commands registry={registry} />);
    expect(screen.getAllByRole("listitem").map((item) => item.textContent)).toEqual(["Fold All Cards: none", "Toggle Sidebar: Alt+Shift+S"]);
  });
});
