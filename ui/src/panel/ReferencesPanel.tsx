/**
 * References (architecture §11.5): for the selected definition, its callers as a tree (each caller
 * expands into its own callers) and the entrances it is reached from, both from `relations`.
 */

import { useQueries, type UseQueryResult } from "@tanstack/react-query";
import { DoorOpen } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { EmptyState, TextMark, TreeView, useContextMenu, type TreeChildren, type TreeRowContent } from "../components";
import type { CallerView, EntranceRef, Relations } from "../ipc/generated";
import { engineQueryOptions, useEngineQuery } from "../ipc/queries";
import { openBlockCard, openEntrance, useActiveBoardKeys } from "../state/boards/api";
import { useSelection } from "../state/selection";
import { openSource } from "../state/tabs";
import { plural } from "../shell/status";
import { openInEditor } from "../views/actions";
import "./panel.css";

/** Callers per page: the most `relations` sends at once. */
const CALLERS_LIMIT = 500;

export type RefNode =
  | { readonly kind: "group"; readonly id: string; readonly label: string; readonly count: number; readonly items: readonly RefNode[] }
  | { readonly kind: "caller"; readonly id: string; readonly caller: CallerView; readonly depth: number }
  | { readonly kind: "entrance"; readonly id: string; readonly entrance: EntranceRef }
  | { readonly kind: "note"; readonly id: string; readonly text: string };

type Loaded = { readonly data: Relations } | { readonly error: string } | null;

function combine(results: UseQueryResult<Relations>[]): Loaded[] {
  return results.map((result) =>
    result.data !== undefined ? { data: result.data } : result.error ? { error: result.error.message } : null,
  );
}

/** Keys never contain these, so they can join a caller row's ancestry into its id. */
const STEP = "\u0000";
const OWNER = "\u0001";
const MAX_DEPTH = 32;

/** One caller row; its id carries its ancestry (so a recursive call gets its own row) and its owner. */
function callerNode(caller: CallerView, parentId: string, depth: number): RefNode {
  return { kind: "caller", id: `${parentId}${STEP}${caller.source}${OWNER}${caller.owner}`, caller, depth };
}

/** The owner whose callers an expanded caller row lists, from its id. */
export function callerOwner(id: string): string | null {
  const at = id.lastIndexOf(OWNER);
  const owner = at < 0 ? null : id.slice(at + 1);
  return owner === null || owner.includes(STEP) ? null : owner;
}

/** The callers of one relations page as nodes, with a note for callers beyond the page. */
export function callerNodes(relations: Relations, parentId: string, depth: number): RefNode[] {
  const nodes = relations.callers.map((caller) => callerNode(caller, parentId, depth));
  const rest = relations.callersTotal - relations.callers.length;
  if (rest > 0) {
    nodes.push({ kind: "note", id: `${parentId}${STEP}more`, text: `and ${plural(rest, "more caller")}` });
  }
  return nodes;
}

/** A definition's callers can be followed further; a file-level caller's can't. */
function canExpand(node: RefNode): boolean {
  if (node.kind === "group") {
    return node.count > 0;
  }
  return node.kind === "caller" && node.depth < MAX_DEPTH && node.caller.owner.includes("#");
}

export function ReferencesPanel() {
  const selection = useSelection();
  const onBoard = useActiveBoardKeys();
  const menu = useContextMenu("Reference actions");
  const key = selection && !selection.key.startsWith("entrance:") ? selection.key : null;
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set(["callers", "reached"]));
  const boardKeys = useMemo(() => [...onBoard].sort(), [onBoard]);
  const peek = useEngineQuery("peek", { key: key ?? "" }, { enabled: key !== null });

  // Expanded caller rows, in a stable order, with the owner each one lists the callers of.
  const expandedCallers = useMemo(
    () =>
      [...expanded]
        .flatMap((id) => {
          const owner = callerOwner(id);
          return owner === null ? [] : [{ id, owner, depth: id.split(STEP).length - 1 }];
        })
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)),
    [expanded],
  );

  const results = useQueries({
    queries: [
      ...(key === null ? [] : [engineQueryOptions("relations", { key, onBoard: boardKeys, callersLimit: CALLERS_LIMIT })]),
      ...expandedCallers.map(({ owner }) => engineQueryOptions("relations", { key: owner, onBoard: boardKeys, callersLimit: CALLERS_LIMIT })),
    ],
    combine,
  });

  const root = key === null ? undefined : results[0];
  const rootData = root && "data" in root ? root.data : null;

  const roots = useMemo((): RefNode[] => {
    if (!rootData) {
      return [];
    }
    return [
      { kind: "group", id: "callers", label: "Callers", count: rootData.callersTotal, items: callerNodes(rootData, "callers", 1) },
      {
        kind: "group",
        id: "reached",
        label: "Reached from",
        count: rootData.reachedFrom.length,
        items: rootData.reachedFrom.map((entrance): RefNode => ({ kind: "entrance", id: `reached${STEP}${entrance.key}`, entrance })),
      },
    ];
  }, [rootData]);

  const children = useMemo(() => {
    const map = new Map<string, TreeChildren<RefNode>>();
    expandedCallers.forEach(({ id, depth }, index) => {
      const loaded = results[(key === null ? 0 : 1) + index];
      map.set(
        id,
        !loaded ? { status: "loading" } : "error" in loaded ? { status: "error", message: loaded.error } : callerNodes(loaded.data, id, depth + 1),
      );
    });
    return map;
  }, [expandedCallers, results, key]);

  const getChildren = useCallback(
    (node: RefNode): TreeChildren<RefNode> | undefined => (node.kind === "group" ? node.items : children.get(node.id)),
    [children],
  );

  if (key === null) {
    return (
      <EmptyState
        size="compact"
        title={selection ? "Entrances have no callers" : "Nothing selected"}
        hint="Select a definition's card, or choose Show callers in the Files view."
      />
    );
  }
  if (root && "error" in root) {
    return <p className="panel-hint panel-error">{root.error}</p>;
  }
  if (!rootData) {
    return <p className="panel-hint">Loading…</p>;
  }

  const renderRow = (node: RefNode): TreeRowContent => {
    switch (node.kind) {
      case "group":
        return { label: <span className="refs-group">{node.label}</span>, marks: <TextMark>{String(node.count)}</TextMark> };
      case "caller":
        return {
          label: node.caller.ownerName,
          description: `${node.caller.path}:${node.caller.line}`,
          ...(node.caller.kind === "calls" ? {} : { marks: <TextMark>{node.caller.kind}</TextMark> }),
          title: `${node.caller.ownerName} · ${node.caller.path}:${node.caller.line}`,
        };
      case "entrance":
        return {
          label: node.entrance.title,
          icon: <DoorOpen size={12} />,
          description: node.entrance.path,
        };
      case "note":
        return { label: node.text, dimmed: true };
    }
  };

  const subject = peek.data?.qualifiedName ?? peek.data?.name ?? key;

  return (
    <div className="refs">
      <p className="refs-subject" title={key}>
        {subject}
      </p>
      <TreeView<RefNode>
        label={`References of ${subject}`}
        className="panel-tree"
        roots={roots}
        getId={(node) => node.id}
        getText={(node) =>
          node.kind === "group" ? node.label : node.kind === "caller" ? node.caller.ownerName : node.kind === "entrance" ? node.entrance.title : node.text
        }
        isExpandable={canExpand}
        getChildren={getChildren}
        renderRow={renderRow}
        expanded={expanded}
        toggleOnClick={false}
        onExpandedChange={setExpanded}
        onSelect={(node) => {
          if (node.kind === "caller") {
            openBlockCard(node.caller.owner);
          } else if (node.kind === "entrance") {
            openEntrance(node.entrance.key);
          }
        }}
        onOpen={(node) => {
          if (node.kind === "caller") {
            openSource(node.caller.path, node.caller.line);
          } else if (node.kind === "entrance") {
            openEntrance(node.entrance.key);
          }
        }}
        onContextMenu={(node, at) => {
          if (node.kind === "caller") {
            const { caller } = node;
            menu.open(at, [
              { id: "card", label: "Open card", onSelect: () => { openBlockCard(caller.owner); } },
              { id: "source", label: "Open source", onSelect: () => { openSource(caller.path, caller.line); } },
              { id: "editor", label: "Open in editor", onSelect: () => void openInEditor(caller.path, caller.line) },
            ]);
          } else if (node.kind === "entrance") {
            const { entrance } = node;
            menu.open(at, [
              { id: "card", label: "Open card", onSelect: () => { openEntrance(entrance.key); } },
              { id: "source", label: "Open source", onSelect: () => { openSource(entrance.path); } },
            ]);
          }
        }}
      />
      {menu.element}
    </div>
  );
}
