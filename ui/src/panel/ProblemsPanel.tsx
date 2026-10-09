/**
 * Problems (architecture §11.5, U-28): files that don't parse or can't be read, settings errors,
 * discovery and input problems, a failed compile, and open boards' health. Unresolved calls are
 * facts about the code, not problems, so they never appear here. A file entry opens a source tab at
 * its line. A board entry of a board open in this window offers its repairs (activating it shows
 * what each does before applying it, the context menu lists them); any other board entry opens the
 * board, or focuses the window that has it.
 */

import { CircleAlert, TriangleAlert, Wrench } from "lucide-react";
import { useMemo } from "react";
import { useShallow } from "zustand/react/shallow";
import { openBoardDialog } from "../boards/dialogs";
import { EmptyState, VirtualList, showToast, useContextMenu, type ListGroup, type MenuEntry } from "../components";
import type { HealthItem, ProblemCategory, ProblemItem } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { toUnfoldError } from "../ipc/unfold";
import { repairLabel } from "../state/boards/health";
import { getBoard, openBoard, useBoards } from "../state/boards/store";
import { openBoardTab, openSource } from "../state/tabs";
import "./panel.css";

const CATEGORY_LABELS: Readonly<Record<ProblemCategory, string>> = {
  compile_failed: "Compile",
  settings: "Settings",
  doesnt_parse: "Files that don't parse",
  cant_read: "Files that can't be read",
  discovery: "Discovery",
  input: "Configuration inputs",
  cache: "Cache",
  board: "Boards",
};

const ORDER: readonly ProblemCategory[] = ["compile_failed", "settings", "doesnt_parse", "cant_read", "discovery", "input", "cache", "board"];

/** Problems grouped by category, in a fixed order, keeping the engine's order inside each. */
export function problemGroups(problems: readonly ProblemItem[]): ListGroup<{ id: string; item: ProblemItem }>[] {
  return ORDER.flatMap((category) => {
    const items = problems
      .map((item, index) => ({ id: `${index}`, item }))
      .filter(({ item }) => item.category === category);
    return items.length === 0 ? [] : [{ id: category, label: CATEGORY_LABELS[category], count: String(items.length), items }];
  });
}

/** `services/payment.ts:42`, or the path alone, or nothing. */
export function problemLocation(item: ProblemItem): string {
  if (item.path === "") {
    return "";
  }
  return item.line === null ? item.path : `${item.path}:${item.line}`;
}

/**
 * The health item a board problem is about, when its board is open in this window: the engine lists
 * one problem per health item, in the health's order, so the n-th problem of a board is its n-th item.
 */
export function boardHealthItem(problems: readonly ProblemItem[], index: number): HealthItem | null {
  const problem = problems[index];
  if (problem?.category !== "board") {
    return null;
  }
  const board = getBoard(problem.path);
  if (!board) {
    return null;
  }
  const nth = problems.slice(0, index).filter((item) => item.category === "board" && item.path === problem.path).length;
  const item = board.health.items[nth];
  return item && problem.message.endsWith(item.message) ? item : null;
}

function showBoard(id: string): void {
  openBoard(id)
    .then((outcome) => {
      if (outcome.status === "opened") {
        openBoardTab(outcome.id);
      }
    })
    .catch((error: unknown) => {
      showToast({ message: `Couldn't open the board: ${toUnfoldError(error).message}`, tone: "danger" });
    });
}

export function openProblem(item: ProblemItem, health: HealthItem | null = null): void {
  if (item.path === "") {
    return;
  }
  if (item.category === "board") {
    if (health && health.repairs.length > 0) {
      openBoardDialog({ kind: "repair", boardId: item.path, item: health });
    } else {
      showBoard(item.path);
    }
    return;
  }
  openSource(item.path, item.line);
}

export function ProblemsPanel() {
  const query = useEngineQuery("problems", {});
  const problems = useMemo(() => query.data ?? [], [query.data]);
  const groups = useMemo(() => problemGroups(problems), [problems]);
  // Re-render when open boards' health changes, so repairs match the current items.
  useBoards(useShallow((state) => state.order.map((id) => state.boards[id]?.health)));
  const menu = useContextMenu("Problem actions");

  const boardMenu = (item: ProblemItem, health: HealthItem | null): MenuEntry[] => [
    { id: "open", label: "Show Board", onSelect: () => { showBoard(item.path); } },
    ...(health && health.repairs.length > 0
      ? [
          { kind: "separator" as const, id: "s1" },
          ...health.repairs.map(
            (repair): MenuEntry => ({
              id: repair,
              label: repairLabel(repair).replace(/…$/, "") + "…",
              onSelect: () => {
                openBoardDialog({ kind: "repair", boardId: item.path, item: health, repair });
              },
            }),
          ),
        ]
      : []),
  ];

  return (
    <>
      <VirtualList<{ id: string; item: ProblemItem }>
        label="Problems"
        className="panel-list"
        groups={groups}
        getId={(entry) => entry.id}
        getText={({ item }) => `${problemLocation(item)} ${item.message}`.trim()}
        renderItem={({ id, item }) => {
          const severe = item.category === "compile_failed" || item.fileState === "failed" || item.category === "cant_read";
          const Icon = severe ? CircleAlert : TriangleAlert;
          const repairable = (boardHealthItem(problems, Number(id))?.repairs.length ?? 0) > 0;
          return (
            <div className="panel-row">
              <Icon size={13} className={severe ? "panel-icon-failed" : "panel-icon-warn"} aria-hidden="true" />
              {item.path === "" ? null : <span className="panel-location">{problemLocation(item)}</span>}
              <span className="panel-message">
                {item.message}
                {item.fileState === "last_good" ? " · showing its last version that parsed" : ""}
              </span>
              {repairable ? (
                <span className="panel-repair">
                  <Wrench size={11} aria-hidden="true" />
                  Repair…
                </span>
              ) : null}
            </div>
          );
        }}
        onActivate={({ id, item }) => {
          openProblem(item, boardHealthItem(problems, Number(id)));
        }}
        onContextMenu={({ id, item }, at) => {
          if (item.category === "board") {
            menu.open(at, boardMenu(item, boardHealthItem(problems, Number(id))));
          }
        }}
        empty={
          query.error ? (
            <p className="panel-hint panel-error">{query.error.message}</p>
          ) : (
            <EmptyState size="compact" title="No problems" hint="Files that don't parse, settings errors, and board health show up here." />
          )
        }
      />
      {menu.element}
    </>
  );
}
