/** Placeholder menus (U1c) until U5 builds them from the designs. */

import type { JSX } from "react";
import { Button, MenuButton, type MenuEntry } from "../components";
import type { ProjectId, ProjectRow } from "../ipc";
import { ActionIds, useRegisterActions } from "../keyboard";
import { revealInFinder } from "../lib/files";
import { copyDiagnostics } from "../settings";
import { currentProject } from "../state/actions";
import { copyPageMermaid, exportPageMermaid, moveBoardToNewWindow, type BoardRef, type PageTarget } from "../state/boards";
import { startCompile } from "../state/compile";
import { openDialog } from "../state/overlays";
import { useProjectRow } from "../state/workspace";

export function boardMenuEntries(ref: BoardRef): MenuEntry[] {
  return [
    { id: "publish", label: "Publish…", onSelect: () => { openDialog({ kind: "publish", board: ref }); } },
    { id: "new-window", label: "Open in New Window", onSelect: () => void moveBoardToNewWindow(ref) },
    { kind: "separator", id: "s1" },
    { id: "delete", label: "Delete…", danger: true, onSelect: () => { openDialog({ kind: "deleteBoard", board: ref }); } },
  ];
}

export function shareMenuEntries(target: PageTarget): MenuEntry[] {
  return [
    { id: "export", label: "Export Mermaid…", onSelect: () => void exportPageMermaid(target) },
    { id: "copy", label: "Copy Mermaid", onSelect: () => void copyPageMermaid(target) },
  ];
}

export function BoardMenuButton({ board }: { readonly board: BoardRef }): JSX.Element {
  return <MenuButton variant="icon" icon="dots" label="Board menu" entries={boardMenuEntries(board)} align="end" />;
}

export function projectMenuEntries(row: ProjectRow): MenuEntry[] {
  return [
    { id: "compile", label: "Compile", onSelect: () => void startCompile(row.id) },
    { id: "settings", label: "Project Settings…", onSelect: () => { openDialog({ kind: "projectSettings", project: row.id }); } },
    { id: "reveal", label: "Reveal in Finder", onSelect: () => void revealInFinder(row.id, "") },
  ];
}

export function ProjectMenuButton({ project, variant }: { readonly project: ProjectId; readonly variant: "row" | "header" }): JSX.Element {
  const row = useProjectRow(project);
  return (
    <MenuButton
      variant="icon"
      icon="dots"
      label="Project menu"
      size={variant === "row" ? 20 : 28}
      entries={row ? projectMenuEntries(row) : []}
      align="end"
    />
  );
}

export function WorktreeButton({ project }: { readonly project: ProjectId }): JSX.Element {
  const branch = useProjectRow(project)?.git?.branch ?? null;
  return (
    <Button variant="ghost" icon="branch" disabled>
      {branch ?? "No branch"}
    </Button>
  );
}

/** The project-level actions, mounted once by the frame. */
export function useProjectActions(): void {
  useRegisterActions([
    {
      id: ActionIds.copyDiagnostics,
      title: "Copy Diagnostics",
      category: "Help",
      icon: "copy",
      run: () => copyDiagnostics(currentProject() ?? undefined),
    },
  ]);
}
