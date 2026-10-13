import type { MenuItemConstructorOptions } from "electron";
import { describe, expect, it, vi } from "vitest";
import { buildMenu, type MenuActions } from "./menu";

// The template is what matters here: building the menu returns it as is.
vi.mock("electron", () => ({ Menu: { buildFromTemplate: (template: unknown) => template } }));

function items(template: readonly MenuItemConstructorOptions[]): MenuItemConstructorOptions[] {
  return template.flatMap((item) => [item, ...(Array.isArray(item.submenu) ? items(item.submenu) : [])]);
}

function menu(devTools = false) {
  const actions = { addProject: vi.fn(), forward: vi.fn<MenuActions["forward"]>() };
  const template = buildMenu(actions, { devTools }) as unknown as MenuItemConstructorOptions[];
  return { actions, template, all: items(template) };
}

function click(item: MenuItemConstructorOptions | undefined, window?: unknown): void {
  (item?.click as ((menuItem: unknown, window: unknown) => void) | undefined)?.(item, window);
}

/** What a menu shows, item by item: a role, a label, or a separator. */
function shown(submenu: MenuItemConstructorOptions["submenu"]): string[] {
  return (submenu as MenuItemConstructorOptions[]).map((item) => item.label ?? item.role ?? item.type ?? "");
}

describe("the application menu", () => {
  it("holds the standard macOS menus, File › Add Project…, and Close Window", () => {
    const { template } = menu();
    expect(template.map((item) => item.label ?? item.role)).toEqual(["appMenu", "File", "Edit", "View", "windowMenu"]);
    expect(shown(template[1]?.submenu)).toEqual(["Add Project…", "separator", "Close Window"]);
    expect(shown(template[2]?.submenu)).toEqual(["Undo", "Redo", "separator", "cut", "copy", "paste", "selectAll"]);
    expect(shown(template[3]?.submenu)).toEqual(["togglefullscreen"]);
    expect(template[1]?.submenu).toContainEqual({ role: "close", label: "Close Window" });
  });

  it("adds a project in the main process", () => {
    const { actions, all } = menu();
    click(all.find((item) => item.label === "Add Project…"));
    expect(actions.addProject).toHaveBeenCalledTimes(1);
    expect(actions.forward).not.toHaveBeenCalled();
  });

  it("forwards Undo and Redo, with their shortcuts, to the window the menu acted on", () => {
    const { actions, all } = menu();
    const window = { id: "the window" };
    const undo = all.find((item) => item.label === "Undo");
    const redo = all.find((item) => item.label === "Redo");
    click(undo, window);
    click(redo, window);
    expect(actions.forward.mock.calls).toEqual([
      ["undo", window],
      ["redo", window],
    ]);
    expect([undo?.accelerator, redo?.accelerator]).toEqual(["CmdOrCtrl+Z", "Shift+CmdOrCtrl+Z"]);
  });

  it("has no other shortcut items, and nothing is disabled", () => {
    const { all } = menu();
    expect(all.filter((item) => item.accelerator !== undefined).map((item) => item.label)).toEqual(["Undo", "Redo"]);
    expect(all.filter((item) => item.enabled === false)).toEqual([]);
  });

  it("offers reload and the developer tools only in development", () => {
    expect(shown(menu(true).template[3]?.submenu)).toEqual(["togglefullscreen", "separator", "reload", "toggleDevTools"]);
  });
});
