/**
 * The application menu: the standard macOS roles plus File › Add Project…. Add Project runs here.
 * Edit › Undo and Redo are forwarded to the window's UI as `menu_command` events, so they reach the
 * board; the preload keeps a text field's native undo when one has focus. Cut, Copy, Paste, and
 * Select All are roles.
 */

import type { BaseWindow, MenuItemConstructorOptions } from "electron";
import { Menu } from "electron";
import type { MenuCommand } from "../shared/unfold-api";

export interface MenuActions {
  /** File › Add Project…: the folder picker, then the folder joins the workspace. */
  addProject(): void;
  /** Forwards a command to the window the menu acted on (Electron passes it; undefined when none is focused). */
  forward(command: MenuCommand, window: BaseWindow | undefined): void;
}

export function buildMenu(actions: MenuActions, options: { devTools: boolean }): Menu {
  const forward = (label: string, command: MenuCommand, accelerator: string): MenuItemConstructorOptions => ({
    label,
    accelerator,
    click: (_item, window) => {
      actions.forward(command, window);
    },
  });

  const template: MenuItemConstructorOptions[] = [
    { role: "appMenu" },
    {
      label: "File",
      submenu: [
        { label: "Add Project…", click: () => { actions.addProject(); } },
        { type: "separator" },
        { role: "close", label: "Close Window" },
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
        { role: "togglefullscreen" },
        ...(options.devTools
          ? [{ type: "separator" as const }, { role: "reload" as const }, { role: "toggleDevTools" as const }]
          : []),
      ],
    },
    { role: "windowMenu" },
  ];
  return Menu.buildFromTemplate(template);
}
