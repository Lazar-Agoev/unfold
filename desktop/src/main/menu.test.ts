import type { MenuItemConstructorOptions } from "electron";
import { describe, expect, it, vi } from "vitest";
import type { MenuCommand } from "../shared/unfold-api";
import { buildMenu, type MenuActions } from "./menu";

// The template is what matters here: building the menu returns it as is.
vi.mock("electron", () => ({ Menu: { buildFromTemplate: (template: unknown) => template } }));

/** Every menu command (the record makes the compiler check none is missing). */
const COMMANDS: Record<MenuCommand, true> = {
  remove_project: true,
  new_board: true,
  new_page: true,
  compile: true,
  publish: true,
  close_tab: true,
  show_home: true,
  toggle_sidebar: true,
  lens_code: true,
  lens_flow: true,
  add: true,
  add_files: true,
  add_functions: true,
  commands: true,
  undo: true,
  redo: true,
  copy_diagnostics: true,
};

/** The redesign's keyboard table (§8, contract §7.4) as menu accelerators, by item label. */
const ACCELERATORS: Readonly<Record<string, string>> = {
  "Add Project…": "CmdOrCtrl+O",
  "New Board": "CmdOrCtrl+N",
  "New Page": "Alt+CmdOrCtrl+N",
  "New Window": "Shift+CmdOrCtrl+N",
  Compile: "Shift+CmdOrCtrl+B",
  Publish: "Shift+CmdOrCtrl+E",
  "Close Tab": "CmdOrCtrl+W",
  "Close Window": "Shift+CmdOrCtrl+W",
  Undo: "CmdOrCtrl+Z",
  Redo: "Shift+CmdOrCtrl+Z",
  Home: "CmdOrCtrl+0",
  "Toggle Sidebar": "CmdOrCtrl+B",
  Code: "CmdOrCtrl+1",
  Flow: "CmdOrCtrl+2",
  Add: "CmdOrCtrl+K",
  "Go to File": "CmdOrCtrl+P",
  "Go to Function": "CmdOrCtrl+T",
  Commands: "Shift+CmdOrCtrl+P",
};

function items(template: readonly MenuItemConstructorOptions[]): MenuItemConstructorOptions[] {
  return template.flatMap((item) => [item, ...(Array.isArray(item.submenu) ? items(item.submenu) : [])]);
}

function menu(isMac: boolean) {
  const actions = { addProject: vi.fn(), newWindow: vi.fn(), forward: vi.fn<MenuActions["forward"]>() };
  const template = buildMenu(actions, { isMac, devTools: false }) as unknown as MenuItemConstructorOptions[];
  return { actions, all: items(template) };
}

function click(item: MenuItemConstructorOptions): void {
  (item.click as (() => void) | undefined)?.();
}

describe("the application menu", () => {
  it.each([true, false])("forwards every menu command exactly once (macOS: %s)", (isMac) => {
    const { actions, all } = menu(isMac);
    for (const item of all) {
      click(item);
    }
    const forwarded = actions.forward.mock.calls.map(([command]) => command);
    expect([...forwarded].sort()).toEqual(Object.keys(COMMANDS).sort());
  });

  it("adds a project and opens a window in the main process", () => {
    const { actions, all } = menu(true);
    click(all.find((item) => item.label === "Add Project…") ?? {});
    click(all.find((item) => item.label === "New Window") ?? {});
    expect(actions.addProject).toHaveBeenCalledTimes(1);
    expect(actions.newWindow).toHaveBeenCalledTimes(1);
    expect(actions.forward).not.toHaveBeenCalled();
  });

  it("shows the redesign's shortcuts, and no item is disabled", () => {
    const { all } = menu(true);
    const labelled = all.filter((item) => item.label !== undefined && item.accelerator !== undefined);
    expect(Object.fromEntries(labelled.map((item) => [item.label, item.accelerator]))).toEqual(ACCELERATORS);
    expect(all.filter((item) => item.enabled === false)).toEqual([]);
  });
});
