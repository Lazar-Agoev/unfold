/**
 * The project menu: `⋯` on a project's sidebar row and on its Home header.
 * Every item is there in every state, disabled when it doesn't apply: Compile (again), Rebuild from
 * scratch, Cancel, Pause or Resume background updates; Settings…, Add entrance…, Health and activity;
 * Reveal in Finder; Remove from workspace….
 */

import type { JSX } from "react";
import { MenuButton, type MenuEntry } from "../components";
import type { ProjectId, ProjectRow, ProjectState } from "../ipc";
import { revealInFinder } from "../lib/files";
import { useProjectRow } from "../state/workspace";
import { addEntrance, backgroundUpdates, cancel, compile, openSettings, removeProject, showHealth } from "./actions";

/** What a project's state allows: a resting project answers as its last state did. */
interface ProjectAbilities {
  readonly compiled: boolean;
  readonly compiling: boolean;
  readonly paused: boolean;
  readonly available: boolean;
}

function projectAbilities(state: ProjectState): ProjectAbilities {
  const settled = state.kind === "resting" ? state.last : state;
  return {
    compiled: settled.kind !== "not_compiled" && settled.kind !== "unavailable",
    compiling: settled.kind === "compiling",
    paused: settled.kind === "paused",
    available: settled.kind !== "unavailable",
  };
}

export function projectMenuEntries(row: ProjectRow): MenuEntry[] {
  const project = row.id;
  const can = projectAbilities(row.state);
  return [
    {
      id: "compile",
      label: can.compiled ? "Compile again" : "Compile",
      disabled: !can.available || can.compiling,
      onSelect: () => void compile(project, "check"),
    },
    {
      id: "rebuild",
      label: "Rebuild from scratch",
      disabled: !can.compiled || can.compiling,
      onSelect: () => void compile(project, "from_scratch"),
    },
    { id: "cancel", label: "Cancel", disabled: !can.compiling, onSelect: () => void cancel(project) },
    can.paused
      ? { id: "background", label: "Resume background updates", disabled: !can.available, onSelect: () => void backgroundUpdates(project, true) }
      : { id: "background", label: "Pause background updates", disabled: !can.compiled, onSelect: () => void backgroundUpdates(project, false) },
    { kind: "separator", id: "s1" },
    {
      id: "settings",
      label: "Settings…",
      disabled: !can.available,
      onSelect: () => {
        openSettings(project);
      },
    },
    {
      id: "entrance",
      label: "Add entrance…",
      disabled: !can.available,
      onSelect: () => {
        addEntrance(project);
      },
    },
    { id: "health", label: "Health and activity", disabled: !can.compiled, onSelect: () => void showHealth(project) },
    { kind: "separator", id: "s2" },
    { id: "reveal", label: "Reveal in Finder", disabled: !can.available, onSelect: () => void revealInFinder(project, "") },
    { kind: "separator", id: "s3" },
    { id: "remove", label: "Remove from workspace…", danger: true, onSelect: () => void removeProject(row) },
  ];
}

/** `⋯` for a project: 20px on its sidebar row, 28px on its Home header; the menu is 264 wide, right-aligned. */
export function ProjectMenuButton({ project, variant }: { readonly project: ProjectId; readonly variant: "row" | "header" }): JSX.Element {
  const row = useProjectRow(project);
  return (
    <MenuButton
      variant="icon"
      icon="dots"
      label="Project menu"
      size={variant === "row" ? 20 : 28}
      entries={row ? projectMenuEntries(row) : []}
      disabled={!row}
      align="end"
      width={264}
    />
  );
}
