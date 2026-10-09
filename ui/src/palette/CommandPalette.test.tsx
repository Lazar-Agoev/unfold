// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createActionRegistry, type ActionSpec } from "../keyboard";
import { CommandPalette } from "./CommandPalette";
import { closePalette, openPalette, registerQuickOpenProvider, usePalette, type QuickOpenItem } from "./store";

function setup(specs: ActionSpec[]) {
  const registry = createActionRegistry();
  registry.register(specs);
  render(<CommandPalette registry={registry} />);
  return registry;
}

const action = (id: string, title: string, category: string, extra: Partial<ActionSpec> = {}): ActionSpec => ({
  id,
  title,
  category,
  run: vi.fn(),
  ...extra,
});

const options = () => screen.queryAllByRole("option").map((option) => option.textContent);

afterEach(() => {
  act(() => {
    closePalette();
  });
});

describe("command palette", () => {
  it("lists enabled actions with their shortcuts and hides disabled or palette-hidden ones", () => {
    setup([
      action("view.toggleInspector", "Toggle Inspector", "View"),
      action("workbench.compile", "Compile", "Graph", { enabled: () => false }),
      action("workbench.commandPalette", "Show All Commands", "View", { showInPalette: false }),
      action("view.toggleSidebar", "Toggle Sidebar", "View"),
    ]);
    act(() => {
      openPalette("commands");
    });
    expect(screen.getByRole("combobox")).toBe(document.activeElement);
    expect(options()).toHaveLength(2);
    expect(options()[0]).toContain("View: Toggle Inspector");
    // The keymap's shortcut for the inspector, in this platform's style.
    expect(screen.getAllByRole("option")[0]?.querySelector("kbd")?.textContent).toMatch(/B$/);
  });

  it("filters fuzzily as you type", async () => {
    const user = userEvent.setup();
    setup([
      action("view.toggleInspector", "Toggle Inspector", "View"),
      action("view.toggleSidebar", "Toggle Sidebar", "View"),
      action("view.showEntrances", "Show Entrances", "View"),
    ]);
    act(() => {
      openPalette("commands");
    });
    await user.keyboard("tgins");
    expect(options()).toEqual([expect.stringContaining("Toggle Inspector")]);
    await user.clear(screen.getByRole("combobox"));
    await user.keyboard("zzzz");
    expect(options()).toEqual([]);
    expect(screen.getByText("No matching commands")).toBeTruthy();
  });

  it("runs the chosen action after closing", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    const registry = setup([action("view.toggleSidebar", "Toggle Sidebar", "View", { run }), action("a.b", "Another", "Z")]);
    act(() => {
      openPalette("commands");
    });
    await user.keyboard("sidebar{Enter}");
    expect(usePalette.getState().open).toBe(false);
    expect(run).toHaveBeenCalledTimes(1);
    expect(registry.get("view.toggleSidebar")).toBeDefined();
  });

  it("closes on Escape", async () => {
    const user = userEvent.setup();
    setup([action("x", "X", "X")]);
    act(() => {
      openPalette("commands");
    });
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("combobox")).toBeNull();
  });
});

describe("quick open", () => {
  it("shows a provider's results for the typed query, cancelling superseded searches", async () => {
    const user = userEvent.setup();
    const opened = vi.fn();
    const signals: AbortSignal[] = [];
    const unregister = registerQuickOpenProvider("files", {
      placeholder: "Go to file",
      search: (query, signal) => {
        signals.push(signal);
        const items: QuickOpenItem[] = ["orders.ts", "order-service.ts", "server.ts"]
          .filter((name) => name.includes(query))
          .map((name) => ({ id: name, label: name, detail: `src/${name}`, run: () => {
            opened(name);
          },
        }));
        return Promise.resolve(items);
      },
    });
    setup([]);
    act(() => {
      openPalette("files");
    });
    expect(screen.getByPlaceholderText("Go to file")).toBeTruthy();
    await waitFor(() => {
      expect(options()).toHaveLength(3);
    });
    await user.keyboard("order");
    await waitFor(() => {
      expect(options()).toEqual([expect.stringContaining("orders.ts"), expect.stringContaining("order-service.ts")]);
    });
    expect(signals.slice(0, -1).every((signal) => signal.aborted)).toBe(true);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(opened).toHaveBeenCalledWith("order-service.ts");
    unregister();
  });

  it("reports provider failures, and `>` switches to commands", async () => {
    const user = userEvent.setup();
    const unregister = registerQuickOpenProvider("symbols", {
      placeholder: "Go to symbol",
      search: () => Promise.reject(new Error("The graph isn't compiled yet.")),
    });
    setup([action("view.toggleSidebar", "Toggle Sidebar", "View")]);
    act(() => {
      openPalette("symbols");
    });
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "The graph isn't compiled yet.");
    fireEvent.change(screen.getByRole("combobox"), { target: { value: ">side" } });
    expect(usePalette.getState().mode).toBe("commands");
    await user.keyboard("bar");
    expect(options()).toEqual([expect.stringContaining("Toggle Sidebar")]);
    unregister();
  });

  it("says when no provider is registered", () => {
    setup([]);
    act(() => {
      openPalette("symbols");
    });
    expect(screen.getByText("Quick open isn't available in this window yet.")).toBeTruthy();
  });
});
