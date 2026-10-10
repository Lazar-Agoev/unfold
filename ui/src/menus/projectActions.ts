/**
 * The actions U5 registers (UI plan §8.1), so the project and board menus' commands are also in Add's
 * `>` mode and the application menu: the project ones act on the window's current project (the active
 * board's, else the one Home shows), the board ones on the active board tab.
 */

import { openMoveToFolder } from "../dialogs";
import type { ProjectRow } from "../ipc";
import { ACTION_INFO, ActionIds, useRegisterActions, type ActionSpec } from "../keyboard";
import { revealInFinder } from "../lib/files";
import { copyDiagnostics } from "../settings";
import { currentProject } from "../state/actions";
import type { BoardRef } from "../state/boards";
import { openDialog } from "../state/overlays";
import { getActiveBoard } from "../state/tabs";
import { getProjectIndex } from "../state/workspace";
import { addEntrance, backgroundUpdates, cancel, compile, duplicate, moveBackToDrafts, openInNewWindow, openSettings, removeProject, renameBoardInPlace, showHealth } from "./actions";
import { cachedBoardFacts, describeBoard } from "./boardMenu";
import { projectAbilities, type ProjectAbilities } from "./projectMenu";

function currentRow(): ProjectRow | undefined {
  const project = currentProject();
  return project === null ? undefined : getProjectIndex().byId.get(project);
}

/** A project action: runs on the current project when `when` allows its state. */
function projectAction(
  spec: Omit<ActionSpec, "run" | "enabled" | "category">,
  when: (can: ProjectAbilities) => boolean,
  run: (row: ProjectRow) => unknown,
): ActionSpec {
  return {
    ...spec,
    category: "Project",
    enabled: () => {
      const row = currentRow();
      return row !== undefined && when(projectAbilities(row.state));
    },
    run: () => {
      const row = currentRow();
      if (row) {
        void run(row);
      }
    },
  };
}

/** A board action: runs on the active board tab when `when` allows it. */
function boardAction(
  spec: Omit<ActionSpec, "run" | "enabled" | "category">,
  run: (ref: BoardRef) => unknown,
  when: (board: ReturnType<typeof describeBoard>) => boolean = () => true,
): ActionSpec {
  return {
    ...spec,
    category: "Board",
    enabled: () => {
      const ref = getActiveBoard();
      return ref !== null && when(describeBoard(cachedBoardFacts(ref)));
    },
    run: () => {
      const ref = getActiveBoard();
      if (ref) {
        void run(ref);
      }
    },
  };
}

const always = () => true;

const PROJECT_ACTIONS: readonly ActionSpec[] = [
  projectAction({ id: "project.compileAgain", title: "Compile Again" }, (can) => can.compiled && !can.compiling, (row) => compile(row.id, "check")),
  projectAction({ id: "project.rebuild", title: "Rebuild from Scratch" }, (can) => can.compiled && !can.compiling, (row) => compile(row.id, "from_scratch")),
  projectAction({ id: "project.cancelCompile", title: "Cancel Compile" }, (can) => can.compiling, (row) => cancel(row.id)),
  projectAction({ id: "project.pause", title: "Pause Background Updates" }, (can) => can.compiled && !can.paused, (row) => backgroundUpdates(row.id, false)),
  projectAction({ id: "project.resume", title: "Resume Background Updates" }, (can) => can.paused && can.available, (row) => backgroundUpdates(row.id, true)),
  projectAction({ id: "project.settings", title: "Project Settings…", icon: "gear" }, (can) => can.available, (row) => {
    openSettings(row.id);
  }),
  projectAction({ id: "project.addEntrance", title: "Add Entrance…", icon: "entrance" }, (can) => can.available, (row) => {
    addEntrance(row.id);
  }),
  projectAction({ id: "project.revealInFinder", title: "Reveal Project in Finder", icon: "reveal" }, (can) => can.available, (row) =>
    revealInFinder(row.id, ""),
  ),
  projectAction({ id: "home.showHealth", title: "Show Health and Activity", icon: "activity" }, (can) => can.compiled, (row) => showHealth(row.id)),
  projectAction({ id: ActionIds.removeProject, title: ACTION_INFO[ActionIds.removeProject].title }, always, (row) => removeProject(row)),
  {
    id: ActionIds.copyDiagnostics,
    title: ACTION_INFO[ActionIds.copyDiagnostics].title,
    category: "Help",
    icon: "copy",
    run: () => copyDiagnostics(currentProject() ?? undefined),
  },
  boardAction({ id: "board.rename", title: "Rename Board…", icon: "edit" }, renameBoardInPlace),
  boardAction({ id: "board.moveToFolder", title: "Move Board to Folder…", icon: "folder" }, (ref) => {
    openMoveToFolder({ board: ref, folder: describeBoard(cachedBoardFacts(ref)).folder, mode: "path" });
  }),
  boardAction({ id: "board.duplicate", title: "Duplicate Board", icon: "board" }, duplicate),
  boardAction({ id: "board.openInNewWindow", title: "Open Board in New Window", icon: "external-link" }, openInNewWindow),
  boardAction(
    { id: "board.discardChanges", title: "Discard Changes…" },
    (ref) => {
      openDialog({ kind: "discard", board: ref });
    },
    (board) => board.changes > 0,
  ),
  boardAction(
    { id: "board.moveBackToDrafts", title: "Move Board Back to Drafts", icon: "move-back" },
    (ref) => moveBackToDrafts(ref, describeBoard(cachedBoardFacts(ref)).title),
    (board) => board.place === "published",
  ),
  boardAction({ id: "board.delete", title: "Delete Board…" }, (ref) => {
    openDialog({ kind: "deleteBoard", board: ref });
  }),
];

/** Registers the project and board actions for as long as the window's app is mounted. */
export function useProjectActions(): void {
  useRegisterActions(PROJECT_ACTIONS);
}
