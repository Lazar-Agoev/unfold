/**
 * The application menu: standard roles plus Unfold's commands (architecture §11.3, §11.13). Add Project and
 * New Window run here; everything else is forwarded to the focused window's UI as a `menu_command`
 * event. Edit > Undo and Redo are forwarded too, so they reach the focused board; the preload keeps
 * a text field's native undo when one has focus. Keys the UI's dispatcher owns alone (`A`, `/`,
 * ⌘[ …) are not menu items.
 */

import type { BaseWindow, MenuItemConstructorOptions } from "electron";
import { Menu } from "electron";
import type { MenuCommand } from "../shared/unfold-api";

export interface MenuActions {
  /** File > Add Project…: the folder picker, then the folder joins the workspace. */
  addProject(): void;
  newWindow(): void;
  /**
   * Forwards a command to a window's UI: the window the menu acted on (Electron passes it), else the
   * focused one (ignored when there is none).
   */
  forward(command: MenuCommand, window: BaseWindow | undefined): void;
}

export function buildMenu(actions: MenuActions, options: { isMac: boolean; devTools: boolean }): Menu {
  const forward = (label: string, command: MenuCommand, accelerator?: string): MenuItemConstructorOptions => ({
    label,
    ...(accelerator ? { accelerator } : {}),
    click: (_item, window) => {
      actions.forward(command, window);
    },
  });

  const template: MenuItemConstructorOptions[] = [
    ...(options.isMac ? [{ role: "appMenu" as const }] : []),
    {
      label: "File",
      submenu: [
        { label: "Add Project…", accelerator: "CmdOrCtrl+O", click: () => { actions.addProject(); } },
        forward("Remove Project", "remove_project"),
        { type: "separator" },
        forward("New Board", "new_board", "CmdOrCtrl+N"),
        forward("New Page", "new_page", "Alt+CmdOrCtrl+N"),
        { label: "New Window", accelerator: "Shift+CmdOrCtrl+N", click: () => { actions.newWindow(); } },
        { type: "separator" },
        forward("Compile", "compile", "Shift+CmdOrCtrl+B"),
        forward("Publish", "publish", "Shift+CmdOrCtrl+E"),
        { type: "separator" },
        forward("Close Tab", "close_tab", "CmdOrCtrl+W"),
        { role: "close", label: "Close Window", accelerator: "Shift+CmdOrCtrl+W" },
        ...(options.isMac ? [] : [{ type: "separator" as const }, { role: "quit" as const }]),
      ],
    },
    {
      label: "Edit",
      submenu: [
        forward("Undo", "undo", "CmdOrCtrl+Z"),
        forward("Redo", "redo", "Shift+CmdOrCtrl+Z"),
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { role: "selectAll" },
      ],
    },
    {
      label: "View",
      submenu: [
        forward("Home", "show_home", "CmdOrCtrl+0"),
        forward("Toggle Sidebar", "toggle_sidebar", "CmdOrCtrl+B"),
        { type: "separator" },
        forward("Code", "lens_code", "CmdOrCtrl+1"),
        forward("Flow", "lens_flow", "CmdOrCtrl+2"),
        { type: "separator" },
        { role: "togglefullscreen" },
        ...(options.devTools
          ? [{ type: "separator" as const }, { role: "reload" as const }, { role: "toggleDevTools" as const }]
          : []),
      ],
    },
    {
      label: "Go",
      submenu: [
        forward("Add", "add", "CmdOrCtrl+K"),
        forward("Go to File", "add_files", "CmdOrCtrl+P"),
        forward("Go to Function", "add_functions", "CmdOrCtrl+T"),
        forward("Commands", "commands", "Shift+CmdOrCtrl+P"),
      ],
    },
    { role: "windowMenu" },
    {
      role: "help",
      submenu: [forward("Copy Diagnostics", "copy_diagnostics")],
    },
  ];
  return Menu.buildFromTemplate(template);
}
