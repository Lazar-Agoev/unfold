/**
 * The project menu: `⋯` on a project's sidebar row and on its Home header. Compile again, Cancel (only
 * while compiling); Open `unfold.toml`; Remove from workspace….
 */

import type { JSX } from "react";
import { MenuButton, type MenuEntry } from "../components";
import type { ProjectId, ProjectRow } from "../ipc";
import { openInEditor } from "../lib/files";
import { useProjectRow } from "../state/workspace";
import { cancel, compile, removeProject } from "./actions";

export function projectMenuEntries(row: ProjectRow): MenuEntry[] {
  const project = row.id;
  const available = row.state.kind !== "unavailable";
  const compiling = row.state.kind === "compiling";
  return [
    { id: "compile", label: "Compile again", disabled: !available || compiling, onSelect: () => void compile(project) },
    { id: "cancel", label: "Cancel", disabled: !compiling, onSelect: () => void cancel(project) },
    { kind: "separator", id: "s1" },
    { id: "config", label: "Open unfold.toml", disabled: !available, onSelect: () => void openInEditor(project, "unfold.toml") },
    { kind: "separator", id: "s2" },
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
