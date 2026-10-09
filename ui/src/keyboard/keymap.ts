/**
 * The keyboard map (architecture §11.7): well-known action ids and their default shortcuts, kept in one
 * place. Views register actions under these ids and get the shortcut without restating it. A person
 * can rebind these actions in Preferences → Keyboard (`bindings.ts`); other ids keep their shortcuts.
 */

import type { MenuCommand } from "../ipc/unfold";

export const ActionIds = {
  openFolder: "workbench.openFolder",
  newWindow: "workbench.newWindow",
  closeTab: "workbench.closeTab",
  closeFolder: "workbench.closeFolder",
  closeWindow: "workbench.closeWindow",
  compile: "workbench.compile",
  quickOpenFiles: "workbench.quickOpenFiles",
  quickOpenSymbols: "workbench.quickOpenSymbols",
  commandPalette: "workbench.commandPalette",
  showFiles: "view.showFiles",
  showEntrances: "view.showEntrances",
  showSearch: "view.showSearch",
  showBoards: "view.showBoards",
  focusSearch: "view.focusSearch",
  toggleSidebar: "view.toggleSidebar",
  toggleInspector: "view.toggleInspector",
  togglePanel: "view.togglePanel",
  back: "navigation.back",
  forward: "navigation.forward",
  fitBoard: "board.fit",
  zoomIn: "board.zoomIn",
  zoomOut: "board.zoomOut",
  undo: "edit.undo",
  redo: "edit.redo",
  copyDiagnostics: "help.copyDiagnostics",
} as const;

export type WellKnownActionId = (typeof ActionIds)[keyof typeof ActionIds];

/** Default shortcuts. The first one is the one shown in menus, tooltips, and the palette. */
export const DEFAULT_SHORTCUTS: Readonly<Record<WellKnownActionId, readonly string[]>> = {
  [ActionIds.openFolder]: ["Mod+O"],
  [ActionIds.newWindow]: ["Mod+Shift+N"],
  [ActionIds.closeTab]: ["Mod+W"],
  [ActionIds.closeFolder]: [],
  [ActionIds.closeWindow]: ["Mod+Shift+W"],
  [ActionIds.compile]: ["Mod+Shift+B"],
  [ActionIds.quickOpenFiles]: ["Mod+P"],
  [ActionIds.quickOpenSymbols]: ["Mod+T"],
  [ActionIds.commandPalette]: ["Mod+Shift+P"],
  [ActionIds.showFiles]: ["Mod+1"],
  [ActionIds.showEntrances]: ["Mod+2"],
  [ActionIds.showSearch]: ["Mod+3"],
  [ActionIds.showBoards]: ["Mod+4"],
  [ActionIds.focusSearch]: ["/"],
  [ActionIds.toggleSidebar]: ["Mod+B"],
  [ActionIds.toggleInspector]: ["Alt+Mod+B", "]"],
  [ActionIds.togglePanel]: ["Mod+J"],
  [ActionIds.back]: ["Mod+["],
  [ActionIds.forward]: ["Mod+]"],
  [ActionIds.fitBoard]: ["Shift+1"],
  [ActionIds.zoomIn]: ["Mod+=", "Mod+Shift+Plus"],
  [ActionIds.zoomOut]: ["Mod+-"],
  [ActionIds.undo]: ["Mod+Z"],
  [ActionIds.redo]: ["Mod+Shift+Z"],
  [ActionIds.copyDiagnostics]: [],
};

/**
 * How the keymap's actions are named where they're listed outside the palette (the Preferences
 * dialog's Keyboard section), which shows them even when the view that registers one isn't open.
 * Listed in display order; titles match the registrations.
 */
export const ACTION_INFO: Readonly<Record<WellKnownActionId, { readonly title: string; readonly category: string }>> = {
  [ActionIds.openFolder]: { title: "Open Folder…", category: "File" },
  [ActionIds.newWindow]: { title: "New Window", category: "File" },
  [ActionIds.closeTab]: { title: "Close Tab", category: "File" },
  [ActionIds.closeFolder]: { title: "Close Folder", category: "File" },
  [ActionIds.closeWindow]: { title: "Close Window", category: "File" },
  [ActionIds.undo]: { title: "Undo", category: "Edit" },
  [ActionIds.redo]: { title: "Redo", category: "Edit" },
  [ActionIds.commandPalette]: { title: "Show All Commands", category: "View" },
  [ActionIds.showFiles]: { title: "Show Files", category: "View" },
  [ActionIds.showEntrances]: { title: "Show Entrances", category: "View" },
  [ActionIds.showSearch]: { title: "Show Search", category: "View" },
  [ActionIds.showBoards]: { title: "Show Boards", category: "View" },
  [ActionIds.focusSearch]: { title: "Focus Search", category: "View" },
  [ActionIds.toggleSidebar]: { title: "Toggle Sidebar", category: "View" },
  [ActionIds.toggleInspector]: { title: "Toggle Inspector", category: "View" },
  [ActionIds.togglePanel]: { title: "Toggle Panel", category: "View" },
  [ActionIds.quickOpenFiles]: { title: "Go to File…", category: "Go" },
  [ActionIds.quickOpenSymbols]: { title: "Go to Symbol…", category: "Go" },
  [ActionIds.back]: { title: "Back", category: "Go" },
  [ActionIds.forward]: { title: "Forward", category: "Go" },
  [ActionIds.compile]: { title: "Compile", category: "Graph" },
  [ActionIds.fitBoard]: { title: "Fit Board", category: "Board" },
  [ActionIds.zoomIn]: { title: "Zoom In", category: "Board" },
  [ActionIds.zoomOut]: { title: "Zoom Out", category: "Board" },
  [ActionIds.copyDiagnostics]: { title: "Copy Diagnostics", category: "Help" },
};

/** Whether `id` is one of the keymap's actions (the ones a person can rebind). */
export function isWellKnownAction(id: string): id is WellKnownActionId {
  return Object.hasOwn(DEFAULT_SHORTCUTS, id);
}

/** The default shortcuts for an action id, or none for ids outside the map. */
export function defaultShortcuts(id: string): readonly string[] {
  return isWellKnownAction(id) ? DEFAULT_SHORTCUTS[id] : [];
}

/** Which action each application-menu command runs, so the menu and the keyboard run the same code. */
export const MENU_COMMAND_ACTIONS: Readonly<Record<MenuCommand, WellKnownActionId>> = {
  open_folder: ActionIds.openFolder,
  close_folder: ActionIds.closeFolder,
  compile: ActionIds.compile,
  close_tab: ActionIds.closeTab,
  command_palette: ActionIds.commandPalette,
  toggle_sidebar: ActionIds.toggleSidebar,
  toggle_inspector: ActionIds.toggleInspector,
  toggle_panel: ActionIds.togglePanel,
  show_files: ActionIds.showFiles,
  show_entrances: ActionIds.showEntrances,
  show_search: ActionIds.showSearch,
  show_boards: ActionIds.showBoards,
  undo: ActionIds.undo,
  redo: ActionIds.redo,
  copy_diagnostics: ActionIds.copyDiagnostics,
};
