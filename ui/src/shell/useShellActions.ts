/**
 * The shell's actions: window and folder commands, Compile, the palette, view switches, and pane
 * toggles. Views register more (and may take over an id, e.g. the editor takes `workbench.closeTab`).
 */

import { windowCommand } from "../ipc/unfold";
import { ActionIds, requestFocus, runScoped, scopedHandler, useRegisterActions, type ActionSpec } from "../keyboard";
import { hasQuickOpenProvider, openPalette } from "../palette";
import { openPreferences } from "../preferences/PreferencesDialog";
import { useLayout, type SidebarView } from "../state/layout";
import { activeTab, closeTab } from "../state/tabs";
import { useWindow } from "../state/window";
import { chooseFolder, closeFolder } from "../welcome/openFolder";
import type { CompileAction } from "./useCompile";

const hasFolder = () => typeof useWindow.getState().info?.folder === "string";

const VIEW_ACTIONS: readonly { id: string; view: SidebarView; title: string }[] = [
  { id: ActionIds.showFiles, view: "files", title: "Show Files" },
  { id: ActionIds.showEntrances, view: "entrances", title: "Show Entrances" },
  { id: ActionIds.showSearch, view: "search", title: "Show Search" },
  { id: ActionIds.showBoards, view: "boards", title: "Show Boards" },
];

export function useShellActions(compile: CompileAction): void {
  const specs: ActionSpec[] = [
    {
      id: ActionIds.openFolder,
      title: "Open Folder…",
      category: "File",
      run: () => chooseFolder("this_window"),
    },
    {
      id: "workbench.openFolderInNewWindow",
      title: "Open Folder in New Window…",
      category: "File",
      run: () => chooseFolder("new_window"),
    },
    {
      id: ActionIds.newWindow,
      title: "New Window",
      category: "File",
      run: async () => {
        await windowCommand("new_window", { folder: "same" });
      },
    },
    {
      // With no tab open, Close Tab closes the window.
      id: ActionIds.closeTab,
      title: "Close Tab",
      category: "File",
      run: async () => {
        const tab = activeTab();
        if (tab) {
          await closeTab(tab.id);
        } else {
          await windowCommand("close_window");
        }
      },
    },
    {
      id: ActionIds.closeFolder,
      title: "Close Folder",
      category: "File",
      enabled: hasFolder,
      run: closeFolder,
    },
    {
      id: ActionIds.closeWindow,
      title: "Close Window",
      category: "File",
      run: async () => {
        await windowCommand("close_window");
      },
    },
    {
      id: ActionIds.compile,
      title: "Compile",
      category: "Graph",
      enabled: () => compile.canCompile,
      run: () => compile.compile("check"),
    },
    {
      id: "workbench.rebuild",
      title: "Rebuild from Scratch",
      category: "Graph",
      enabled: () => compile.canCompile,
      run: () => compile.compile("from_scratch"),
    },
    {
      id: "workbench.cancelCompile",
      title: "Cancel Compile",
      category: "Graph",
      enabled: () => compile.isCompiling,
      run: compile.cancel,
    },
    {
      id: "workbench.showIndexLog",
      title: "Show Index Log",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().showPanelTab("index_log");
      },
    },
    {
      id: "workbench.showProblems",
      title: "Show Problems",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().showPanelTab("problems");
      },
    },
    {
      id: "workbench.preferences",
      title: "Preferences…",
      category: "File",
      shortcuts: ["Mod+,"],
      run: openPreferences,
    },
    {
      id: ActionIds.commandPalette,
      title: "Show All Commands",
      category: "View",
      showInPalette: false,
      run: () => {
        openPalette("commands");
      },
    },
    {
      id: ActionIds.quickOpenFiles,
      title: "Go to File…",
      category: "Go",
      enabled: () => hasQuickOpenProvider("files"),
      run: () => {
        openPalette("files");
      },
    },
    {
      id: ActionIds.quickOpenSymbols,
      title: "Go to Symbol…",
      category: "Go",
      enabled: () => hasQuickOpenProvider("symbols"),
      run: () => {
        openPalette("symbols");
      },
    },
    ...VIEW_ACTIONS.map(
      ({ id, view, title }): ActionSpec => ({
        id,
        title,
        category: "View",
        enabled: hasFolder,
        run: () => {
          useLayout.getState().showView(view);
        },
      }),
    ),
    {
      id: ActionIds.focusSearch,
      title: "Focus Search",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().showView("search");
        requestFocus("search");
      },
    },
    {
      id: ActionIds.toggleSidebar,
      title: "Toggle Sidebar",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().toggleSidebar();
      },
    },
    {
      id: ActionIds.toggleInspector,
      title: "Toggle Inspector",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().toggleInspector();
      },
    },
    {
      id: ActionIds.togglePanel,
      title: "Toggle Panel",
      category: "View",
      enabled: hasFolder,
      run: () => {
        useLayout.getState().togglePanel();
      },
    },
    // Undo and redo go to the focused context (a board). In a text field the keys are left alone,
    // so the field's native undo (and the Edit menu's) works.
    {
      id: ActionIds.undo,
      title: "Undo",
      category: "Edit",
      allowInInput: false,
      enabled: () => scopedHandler("undo") !== null,
      run: () => {
        runScoped("undo");
      },
    },
    {
      id: ActionIds.redo,
      title: "Redo",
      category: "Edit",
      allowInInput: false,
      enabled: () => scopedHandler("redo") !== null,
      run: () => {
        runScoped("redo");
      },
    },
  ];
  useRegisterActions(specs);
}
