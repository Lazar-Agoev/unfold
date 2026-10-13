/**
 * The keyboard map: the well-known action ids and their default shortcuts, kept in one
 * place. Screens register actions under these ids and get the shortcut without restating it. A person
 * can rebind exactly these actions in Preferences → Keyboard (`bindings.ts`); other registered ids
 * (Add's commands, menus) keep the shortcuts their specs give.
 */

import type { MenuCommand } from "../ipc/unfold";

export const ActionIds = {
  add: "add.open",
  addCommands: "add.commands",
  addFiles: "add.files",
  addFunctions: "add.functions",
  home: "tabs.home",
  nextTab: "tabs.next",
  previousTab: "tabs.previous",
  closeTab: "tabs.close",
  newBoard: "board.new",
  newPage: "board.newPage",
  newWindow: "window.new",
  closeWindow: "window.close",
  addProject: "project.add",
  compile: "project.compile",
  lensDiagram: "board.lensDiagram",
  lensCode: "board.lensCode",
  toggleSidebar: "view.toggleSidebar",
  back: "navigation.back",
  forward: "navigation.forward",
  fit: "board.fit",
  zoomIn: "board.zoomIn",
  zoomOut: "board.zoomOut",
  arrange: "board.arrange",
  viewCode: "board.viewCode",
  undo: "edit.undo",
  redo: "edit.redo",
  preferences: "app.preferences",
  publish: "board.publish",
  removeProject: "project.remove",
  copyDiagnostics: "help.copyDiagnostics",
} as const;

export type WellKnownActionId = (typeof ActionIds)[keyof typeof ActionIds];

/** Default shortcuts. The first one is the one shown in menus, tooltips, and Add. */
export const DEFAULT_SHORTCUTS: Readonly<Record<WellKnownActionId, readonly string[]>> = {
  [ActionIds.add]: ["A", "Mod+K", "/"],
  [ActionIds.addCommands]: ["Mod+Shift+P"],
  [ActionIds.addFiles]: ["Mod+P"],
  [ActionIds.addFunctions]: ["Mod+T"],
  [ActionIds.home]: ["Mod+0"],
  [ActionIds.nextTab]: ["Ctrl+Tab", "Mod+Shift+]"],
  [ActionIds.previousTab]: ["Ctrl+Shift+Tab", "Mod+Shift+["],
  [ActionIds.closeTab]: ["Mod+W"],
  [ActionIds.newBoard]: ["Mod+N"],
  [ActionIds.newPage]: ["Alt+Mod+N"],
  [ActionIds.newWindow]: ["Mod+Shift+N"],
  [ActionIds.closeWindow]: ["Mod+Shift+W"],
  [ActionIds.addProject]: ["Mod+O"],
  [ActionIds.compile]: ["Mod+Shift+B"],
  [ActionIds.lensDiagram]: ["Mod+1"],
  [ActionIds.lensCode]: ["Mod+2"],
  [ActionIds.toggleSidebar]: ["Mod+B"],
  [ActionIds.back]: ["Mod+["],
  [ActionIds.forward]: ["Mod+]"],
  [ActionIds.fit]: ["Shift+1"],
  [ActionIds.zoomIn]: ["Mod+=", "Mod+Shift+Plus"],
  [ActionIds.zoomOut]: ["Mod+-"],
  [ActionIds.arrange]: ["Shift+A"],
  [ActionIds.viewCode]: ["C"],
  [ActionIds.undo]: ["Mod+Z"],
  [ActionIds.redo]: ["Mod+Shift+Z"],
  [ActionIds.preferences]: ["Mod+,"],
  [ActionIds.publish]: ["Mod+Shift+E"],
  [ActionIds.removeProject]: [],
  [ActionIds.copyDiagnostics]: [],
};

/**
 * How the keymap's actions are named where they're listed outside Add (Preferences → Keyboard), which
 * shows them even when the screen that registers one isn't mounted. Listed in display order; titles
 * match the registrations.
 */
export const ACTION_INFO: Readonly<Record<WellKnownActionId, { readonly title: string; readonly category: string }>> = {
  [ActionIds.newBoard]: { title: "New Board", category: "File" },
  [ActionIds.newWindow]: { title: "New Window", category: "File" },
  [ActionIds.addProject]: { title: "Add Project…", category: "File" },
  [ActionIds.closeTab]: { title: "Close Tab", category: "File" },
  [ActionIds.closeWindow]: { title: "Close Window", category: "File" },
  [ActionIds.preferences]: { title: "Preferences…", category: "File" },
  [ActionIds.undo]: { title: "Undo", category: "Edit" },
  [ActionIds.redo]: { title: "Redo", category: "Edit" },
  [ActionIds.home]: { title: "Home", category: "View" },
  [ActionIds.nextTab]: { title: "Next Tab", category: "View" },
  [ActionIds.previousTab]: { title: "Previous Tab", category: "View" },
  [ActionIds.toggleSidebar]: { title: "Toggle Sidebar", category: "View" },
  [ActionIds.lensDiagram]: { title: "Diagram", category: "View" },
  [ActionIds.lensCode]: { title: "Code", category: "View" },
  [ActionIds.add]: { title: "Add…", category: "Go" },
  [ActionIds.addCommands]: { title: "Commands…", category: "Go" },
  [ActionIds.addFiles]: { title: "Go to File…", category: "Go" },
  [ActionIds.addFunctions]: { title: "Go to Function…", category: "Go" },
  [ActionIds.back]: { title: "Back", category: "Go" },
  [ActionIds.forward]: { title: "Forward", category: "Go" },
  [ActionIds.newPage]: { title: "New Page", category: "Board" },
  [ActionIds.publish]: { title: "Publish", category: "Board" },
  [ActionIds.fit]: { title: "Fit", category: "Board" },
  [ActionIds.zoomIn]: { title: "Zoom In", category: "Board" },
  [ActionIds.zoomOut]: { title: "Zoom Out", category: "Board" },
  [ActionIds.arrange]: { title: "Arrange by Call Depth", category: "Board" },
  [ActionIds.viewCode]: { title: "View Code", category: "Board" },
  [ActionIds.compile]: { title: "Compile", category: "Project" },
  [ActionIds.removeProject]: { title: "Remove Project from Workspace…", category: "Project" },
  [ActionIds.copyDiagnostics]: { title: "Copy Diagnostics", category: "Help" },
};

/** The groups Add's commands list (`ActionSpec.group`, else its category), in display order. */
export const ADD_GROUPS = ["Board", "Share", "Project", "Go", "View", "File", "Edit", "Help"] as const;

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
  add: ActionIds.add,
  commands: ActionIds.addCommands,
  add_files: ActionIds.addFiles,
  add_functions: ActionIds.addFunctions,
  show_home: ActionIds.home,
  close_tab: ActionIds.closeTab,
  new_board: ActionIds.newBoard,
  new_page: ActionIds.newPage,
  compile: ActionIds.compile,
  lens_diagram: ActionIds.lensDiagram,
  lens_code: ActionIds.lensCode,
  toggle_sidebar: ActionIds.toggleSidebar,
  undo: ActionIds.undo,
  redo: ActionIds.redo,
  publish: ActionIds.publish,
  remove_project: ActionIds.removeProject,
  copy_diagnostics: ActionIds.copyDiagnostics,
};
