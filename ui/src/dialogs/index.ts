/**
 * One host for every dialog (UI plan §2, U5): Preferences, Project settings, the entrance editor,
 * Publish, Discard changes, Delete, the board folder prompts, and the board's Make a group (U2's).
 * `openMoveToFolder` asks for the folder a board moves to (the board menu's "New folder…").
 */

export { DialogHost } from "./DialogHost";
export { openMoveToFolder } from "./moveToFolder";
