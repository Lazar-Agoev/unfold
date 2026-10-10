/**
 * The window-level actions the frame registers: tabs, New board, windows,
 * Add project, Compile, the sidebar, Undo/Redo (sent to the focused board's scope), and Preferences. What
 * they do lives in `state/actions.ts`; here they get their ids, titles, and when they apply.
 */

import { ActionIds, runScoped, scopedHandler, useRegisterActions, type ActionSpec } from "../keyboard";
import { addProject, closeActiveTab, closeWindow, compileHere, currentProject, newBoardHere, newWindow, openPreferences } from "../state/actions";
import { toggleSidebar } from "../state/layout";
import { activateHome, nextTab, previousTab } from "../state/tabs";

const hasProject = () => currentProject() !== null;

export const GLOBAL_ACTIONS: readonly ActionSpec[] = [
  { id: ActionIds.home, title: "Home", category: "View", icon: "home", run: activateHome },
  { id: ActionIds.nextTab, title: "Next Tab", category: "View", run: nextTab },
  { id: ActionIds.previousTab, title: "Previous Tab", category: "View", run: previousTab },
  { id: ActionIds.closeTab, title: "Close Tab", category: "File", icon: "close", run: closeActiveTab },
  {
    id: ActionIds.newBoard,
    title: "New Board",
    category: "File",
    icon: "board",
    enabled: hasProject,
    run: async () => {
      await newBoardHere();
    },
  },
  { id: ActionIds.newWindow, title: "New Window", category: "File", run: newWindow },
  { id: ActionIds.closeWindow, title: "Close Window", category: "File", run: closeWindow },
  { id: ActionIds.addProject, title: "Add Project…", category: "File", icon: "folder", run: addProject },
  { id: ActionIds.compile, title: "Compile", category: "Project", enabled: hasProject, run: compileHere },
  { id: ActionIds.toggleSidebar, title: "Toggle Sidebar", category: "View", run: toggleSidebar },
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
  { id: ActionIds.preferences, title: "Preferences…", category: "File", icon: "gear", run: openPreferences },
];

/** Registers the frame's actions for as long as the window's app is mounted. */
export function useGlobalActions(): void {
  useRegisterActions(GLOBAL_ACTIONS);
}
