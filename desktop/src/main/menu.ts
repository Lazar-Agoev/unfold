/**
 * The application menu: standard roles plus Unfold's commands (architecture §11.7). Window actions
 * run here; everything else is forwarded to the focused window's UI as a `menu_command` event.
 * Edit > Undo and Redo are forwarded too, so they reach the focused board; the preload keeps a text
 * field's native undo when one has focus.
 */

import type { BaseWindow, MenuItemConstructorOptions } from "electron";
import { Menu } from "electron";
import type { MenuCommand } from "../shared/unfold-api";

export interface MenuActions {
  /** File > Open Folder…, for the window the menu acted on (Electron passes it), if any. */
  openFolder(window: BaseWindow | undefined): void;
  openFolderInNewWindow(): void;
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
        { label: "Open Folder…", accelerator: "CmdOrCtrl+O", click: (_item, window) => { actions.openFolder(window); } },
        { label: "Open Folder in New Window…", click: () => { actions.openFolderInNewWindow(); } },
        { label: "New Window", accelerator: "Shift+CmdOrCtrl+N", click: () => { actions.newWindow(); } },
        { type: "separator" },
        forward("Compile", "compile", "Shift+CmdOrCtrl+B"),
        { type: "separator" },
        forward("Close Tab", "close_tab", "CmdOrCtrl+W"),
        forward("Close Folder", "close_folder"),
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
        forward("Command Palette…", "command_palette", "Shift+CmdOrCtrl+P"),
        { type: "separator" },
        forward("Files", "show_files", "CmdOrCtrl+1"),
        forward("Entrances", "show_entrances", "CmdOrCtrl+2"),
        forward("Search", "show_search", "CmdOrCtrl+3"),
        forward("Boards", "show_boards", "CmdOrCtrl+4"),
        { type: "separator" },
        forward("Toggle Sidebar", "toggle_sidebar", "CmdOrCtrl+B"),
        forward("Toggle Inspector", "toggle_inspector", "Alt+CmdOrCtrl+B"),
        forward("Toggle Panel", "toggle_panel", "CmdOrCtrl+J"),
        { type: "separator" },
        { role: "togglefullscreen" },
        ...(options.devTools
          ? [{ type: "separator" as const }, { role: "reload" as const }, { role: "toggleDevTools" as const }]
          : []),
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
