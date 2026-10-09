/**
 * The Entrances view (architecture §11.3): applications named after their startup, their launches,
 * then their entrances with a fixed method column; then "Not reached from a launch" and "Launches
 * into code that isn't indexed". Clicking an entrance opens its card and its handler cards. The
 * `…` menu adds a manual entrance; a manual entrance's context menu edits or removes it.
 */

/** The header's `…` menu. */
const HEADER_MENU: readonly MenuEntry[] = [
  { id: "add", label: "Add Entrance…", onSelect: () => { openEntranceEditor(); } },
  { id: "settings", label: "Folder Settings…", onSelect: openFolderSettings },
];

function HeaderMenu() {
  return <MenuButton variant="icon" icon={MoreHorizontal} label="Entrance actions" size="small" entries={HEADER_MENU} />;
}

import { DoorOpen, MoreHorizontal, Play } from "lucide-react";
import { useCallback, useMemo } from "react";
import { create } from "zustand";
import {
  Button,
  DotMark,
  EmptyState,
  MenuButton,
  TextMark,
  TreeView,
  useContextMenu,
  type MenuEntry,
  type TreeRowContent,
} from "../../components";
import type { ApplicationNode, EntranceRow, EntranceTree, LaunchRow } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { ActionIds, actions } from "../../keyboard";
import { confirmRemoveEntrance, openEntranceEditor, openFolderSettings } from "../../settings";
import { openEntrance, useActiveBoardKeys } from "../../state/boards/api";
import { openSource } from "../../state/tabs";
import { plural } from "../../shell/status";
import { openInEditor } from "../actions";
import { LanguageTag } from "../glyphs";
import { ViewHeader } from "../ViewHeader";
import "../views.css";

export type EntranceNode =
  | { readonly kind: "app"; readonly id: string; readonly app: ApplicationNode }
  | { readonly kind: "group"; readonly id: string; readonly label: string; readonly count: number; readonly items: readonly EntranceNode[] }
  | { readonly kind: "launch"; readonly id: string; readonly launch: LaunchRow }
  | { readonly kind: "entrance"; readonly id: string; readonly row: EntranceRow; readonly tagged: boolean };

const launchNode = (launch: LaunchRow): EntranceNode => ({ kind: "launch", id: `launch:${launch.key}`, launch });
const entranceNode = (row: EntranceRow, tagged: boolean): EntranceNode => ({ kind: "entrance", id: `entrance:${row.key}`, row, tagged });

/** The tree's roots: one per application, then the two groups. */
export function entranceRoots(tree: EntranceTree): EntranceNode[] {
  return [
    ...tree.applications.map((app): EntranceNode => ({ kind: "app", id: `app:${app.startup}`, app })),
    {
      kind: "group",
      id: "group:unreached",
      label: "Not reached from a launch",
      count: tree.unreached.length,
      items: tree.unreached.map((row) => entranceNode(row, true)),
    },
    {
      kind: "group",
      id: "group:unindexed",
      label: "Launches into code that isn't indexed",
      count: tree.unindexedLaunches.length,
      items: tree.unindexedLaunches.map(launchNode),
    },
  ];
}

function nodeChildren(node: EntranceNode): readonly EntranceNode[] | undefined {
  switch (node.kind) {
    case "app":
      return [...node.app.launches.map(launchNode), ...node.app.entrances.map((row) => entranceNode(row, false))];
    case "group":
      return node.items;
    default:
      return undefined;
  }
}

/** The method column: the HTTP method, or the kind for other entrances. */
export function methodColumn(row: EntranceRow): string {
  return row.method ?? row.kind.toUpperCase();
}

/** The trigger as people read it: the label, the trigger, or "path unknown". */
export function triggerText(row: EntranceRow): string {
  if (row.label) {
    return row.label;
  }
  if (row.trigger !== null && row.triggerStatus !== "unknown") {
    return row.triggerStatus === "partial" ? `${row.trigger}…` : row.trigger;
  }
  return row.kind === "http" ? "path unknown" : "trigger unknown";
}

interface EntrancesViewState {
  /** Null until the first tree arrives; then the first application starts open. */
  readonly expanded: ReadonlySet<string> | null;
}

const useEntrancesView = create<EntrancesViewState>()(() => ({ expanded: null }));

export function EntrancesView() {
  const query = useEngineQuery("entrance_tree", {});
  const onBoard = useActiveBoardKeys();
  const stored = useEntrancesView((state) => state.expanded);
  const menu = useContextMenu("Entrance actions");

  const roots = useMemo(() => (query.data ? entranceRoots(query.data) : []), [query.data]);
  const expanded = useMemo(() => stored ?? new Set(roots[0]?.kind === "app" ? [roots[0].id] : []), [stored, roots]);
  const getChildren = useCallback((node: EntranceNode) => nodeChildren(node), []);

  if (query.error) {
    return (
      <section className="view" aria-labelledby="entrances-title">
        <ViewHeader title="Entrances" id="entrances-title">
          <HeaderMenu />
        </ViewHeader>
        {query.error.code === "not_compiled" ? (
          <EmptyState
            size="compact"
            icon={DoorOpen}
            title="No entrances yet"
            hint="Compile the folder to find its routes, workers, schedules, and launches."
            actions={
              <Button
                size="small"
                onClick={() => {
                  actions.run(ActionIds.compile);
                }}
              >
                Compile
              </Button>
            }
          />
        ) : (
          <p className="view-error">{query.error.message}</p>
        )}
      </section>
    );
  }

  const renderRow = (node: EntranceNode): TreeRowContent => {
    switch (node.kind) {
      case "app": {
        const { app } = node;
        // The name stays whole; the startup's path gives way when the sidebar is narrow.
        return {
          label: (
            <span className="entrance-app">
              <span className="entrance-app-name">{app.name}</span>
              <LanguageTag language={app.language} />
              <span className="entrance-app-path">{app.path}</span>
            </span>
          ),
          marks: <TextMark>{String(app.entrances.length)}</TextMark>,
          title: `${app.name} · ${app.path}`,
        };
      }
      case "group":
        return { label: node.label, marks: <TextMark>{String(node.count)}</TextMark> };
      case "launch":
        return {
          label: <span className="entrance-launch">{node.launch.title}</span>,
          icon: <Play size={11} />,
          ...(node.launch.note ? { description: node.launch.note } : {}),
          title: `${node.launch.title} · ${node.launch.path}${node.launch.startup ? ` · starts ${node.launch.startup}` : ""}`,
        };
      case "entrance": {
        const { row } = node;
        return {
          label: (
            <span className="entrance-row">
              <span className={`entrance-method entrance-method-${row.kind}`}>{methodColumn(row)}</span>
              <span className="entrance-trigger">{triggerText(row)}</span>
            </span>
          ),
          marks: (
            <>
              {node.tagged && row.language ? <LanguageTag language={row.language} /> : null}
              {onBoard.has(row.key) ? <DotMark tone="accent" label="On the open board" /> : null}
            </>
          ),
          title: `${row.title} · ${row.path}${row.line === null ? "" : `:${row.line}`}`,
        };
      }
    }
  };

  const openNode = (node: EntranceNode) => {
    if (node.kind === "entrance") {
      openEntrance(node.row.key);
    } else if (node.kind === "launch") {
      openEntrance(node.launch.key);
    }
  };

  const total = query.data ? query.data.applications.reduce((sum, app) => sum + app.entrances.length, 0) + query.data.unreached.length : 0;

  return (
    <section className="view" aria-labelledby="entrances-title">
      <ViewHeader title="Entrances" id="entrances-title">
        {query.data ? <span className="view-count">{plural(total, "entrance")}</span> : null}
        <HeaderMenu />
      </ViewHeader>
      {query.isPending ? <p className="view-hint">Loading…</p> : null}
      {query.data && roots.length === 2 && total === 0 && query.data.unindexedLaunches.length === 0 ? (
        <EmptyState
          size="compact"
          icon={DoorOpen}
          title="No entrances found"
          hint="Add manual entrances for ones the detectors can't see. They're saved in .unfold/config.toml."
          actions={
            <Button
              size="small"
              onClick={() => {
                openEntranceEditor();
              }}
            >
              Add Entrance…
            </Button>
          }
        />
      ) : (
        <TreeView<EntranceNode>
          label="Entrances"
          className="view-tree"
          roots={roots}
          getId={(node) => node.id}
          getText={(node) =>
            node.kind === "app" ? node.app.name : node.kind === "group" ? node.label : node.kind === "launch" ? node.launch.title : triggerText(node.row)
          }
          isExpandable={(node) => node.kind === "app" || (node.kind === "group" && node.count > 0)}
          getChildren={getChildren}
          renderRow={renderRow}
          expanded={expanded}
          onExpandedChange={(next) => {
            useEntrancesView.setState({ expanded: next });
          }}
          onSelect={openNode}
          onContextMenu={(node, at) => {
            if (node.kind !== "entrance" && node.kind !== "launch") {
              return;
            }
            const key = node.kind === "entrance" ? node.row.key : node.launch.key;
            const path = node.kind === "entrance" ? node.row.path : node.launch.path;
            const line = node.kind === "entrance" ? node.row.line : null;
            const entries: MenuEntry[] = [
              { id: "card", label: "Open card", onSelect: () => { openEntrance(key); } },
              { id: "source", label: "Open source", onSelect: () => { openSource(path, line); } },
              { id: "editor", label: "Open in editor", onSelect: () => void openInEditor(path, line) },
            ];
            if (node.kind === "entrance" && node.row.detector === "manual") {
              const row = node.row;
              entries.push(
                { kind: "separator", id: "manual" },
                { id: "edit", label: "Edit…", onSelect: () => { openEntranceEditor({ mode: "edit", row }); } },
                { id: "remove", label: "Remove", danger: true, onSelect: () => { confirmRemoveEntrance(row); } },
              );
            }
            menu.open(at, entries);
          }}
        />
      )}
      {menu.element}
    </section>
  );
}
