/**
 * One scope's entrance rows on How it starts (an application's, or the ones no launch reaches): its
 * pages, asked for one `nextCursor` at a time, and its rows under their section headings. A launch row
 * leads; a router mounted from a file other apps also mount collapses into one row. Hover a row to
 * Trace it onto a new draft.
 */

import { useQueries } from "@tanstack/react-query";
import { useState, type CSSProperties, type MouseEvent } from "react";
import type { MenuEntry } from "../../components";
import { Icon } from "../../icons";
import { engineQueryOptions, type CatalogRow, type Editable, type EntranceScope, type EntranceSection, type LaunchRow, type ProjectId } from "../../ipc";
import { methodColumn } from "../../lib/entrances";
import { formatCount, plural } from "../../lib/format";
import { openDialog } from "../../state/overlays";
import { traceFromHome } from "../actions";
import { useWindowed } from "../windowed";

export type OnMenu = (event: MouseEvent, entries: MenuEntry[]) => void;

/** What a row's menu acts on: the entrance, its `[[entrance]]` ID when configured, how it is edited, and its name. */
interface MenuTarget {
  readonly key: string;
  readonly id: string | null;
  readonly editable: Editable;
  readonly name: string;
}

/** A launch as its menu names it: by its label, else its command. */
export function launchTarget(launch: Pick<LaunchRow, "key" | "id" | "editable" | "label" | "command">): MenuTarget {
  return { key: launch.key, id: launch.id, editable: launch.editable, name: launch.label ?? launch.command ?? launch.key };
}

/** The editor on an entrance declared in `unfold.toml`, or on an override of a detected launch (Fix…, Edit…). */
export function editEntrance(project: ProjectId, target: Pick<MenuTarget, "key" | "id" | "editable">): void {
  if (target.editable === "configured" && target.id !== null) {
    openDialog({ kind: "launchEditor", project, mode: "edit", id: target.id });
  } else {
    openDialog({ kind: "launchEditor", project, mode: "override", key: target.key });
  }
}

/** An entrance's menu: Trace and Edit…, and Remove… for one declared in `unfold.toml`. */
export function entranceMenu(project: ProjectId, target: MenuTarget): MenuEntry[] {
  const id = target.editable === "configured" ? target.id : null;
  return [
    { id: "trace", label: "Trace on a New Board", icon: "board-draft", onSelect: () => void traceFromHome(project, target.key) },
    { kind: "separator", id: "s1" },
    { id: "edit", label: "Edit…", icon: "edit", onSelect: () => { editEntrance(project, target); } },
    ...(id === null
      ? []
      : [{ id: "remove", label: "Remove…", danger: true, onSelect: () => { openDialog({ kind: "removeEntrance", project, id, name: target.name }); } }]),
  ];
}

export type ScopeArgs = { readonly application: string } | { readonly unreached: true };

export interface Scope {
  readonly sections: readonly EntranceSection[];
  readonly rows: readonly CatalogRow[];
  /** Rows matching the filter; null until the first page arrives. */
  readonly total: number | null;
  /** Asks for the next page, when there is one. */
  readonly more: (() => void) | null;
  readonly error: string | null;
}

/** A scope's rows, page after page; a new filter starts again from the first page. */
export function useScope(project: ProjectId, scope: ScopeArgs, filter: string, limit: number): Scope {
  const [paging, setPaging] = useState<{ readonly filter: string; readonly cursors: readonly string[] }>({ filter, cursors: [] });
  const cursors = paging.filter === filter ? paging.cursors : [];
  const base = { project, ...scope, limit, ...(filter === "" ? {} : { filter }) };
  const pages = useQueries({
    queries: [null, ...cursors].map((cursor) => engineQueryOptions("entrances", cursor === null ? base : { ...base, cursor })),
  });
  const views = pages.flatMap((page) => (page.data?.view === "scope" ? [page.data as EntranceScope] : []));
  const first = views[0];
  const next = views.length === pages.length ? (views.at(-1)?.nextCursor ?? null) : null;
  return {
    sections: first?.sections ?? [],
    rows: views.flatMap((view) => view.rows),
    total: first?.total ?? null,
    more:
      next === null
        ? null
        : () => {
            setPaging({ filter, cursors: [...cursors, next] });
          },
    error: pages.find((page) => page.error)?.error?.message ?? null,
  };
}

type Item =
  | { readonly kind: "section"; readonly key: string; readonly section: EntranceSection }
  | { readonly kind: "row"; readonly key: string; readonly row: CatalogRow }
  | { readonly kind: "shared"; readonly key: string; readonly index: number; readonly section: EntranceSection; readonly rows: readonly CatalogRow[] };

/**
 * The rows in display order with a heading where each section starts (the launch section has none);
 * a section other applications also serve is one row, followed by its rows while it is open.
 */
export function scopeItems(sections: readonly EntranceSection[], rows: readonly CatalogRow[], open: ReadonlySet<number>): Item[] {
  const items: Item[] = [];
  let current = -1;
  for (const row of rows) {
    const section = sections[row.section];
    if (row.section !== current && section) {
      current = row.section;
      if (section.kind !== "launch") {
        items.push({ kind: "section", key: `section:${row.section}`, section });
      }
      if (section.alsoIn.length > 0) {
        items.push({ kind: "shared", key: `shared:${row.section}`, index: row.section, section, rows: rows.filter((other) => other.section === row.section) });
      }
    }
    if (!section || section.alsoIn.length === 0 || open.has(row.section)) {
      items.push({ kind: "row", key: row.key, row });
    }
  }
  return items;
}

const ROW = 28;
const HEADING = 24;
/** Past this many rows a scope renders only the rows in view. */
const WINDOW_THRESHOLD = 100;

function ScopeRow({ project, row, launch, onMenu, style }: {
  readonly project: ProjectId;
  readonly row: CatalogRow;
  readonly launch: LaunchRow | undefined;
  readonly onMenu: OnMenu;
  readonly style?: CSSProperties | undefined;
}) {
  const trace = () => void traceFromHome(project, row.key);
  const launchRow = row.kind === "process";
  const kind = launchRow ? "LAUNCH" : methodColumn(row);
  const name = launchRow ? (launch?.label ?? launch?.command ?? row.title) : (row.trigger ?? row.title);
  // A launch's menu, or a configured handler entrance's; detected handlers have none.
  const target = launch ? launchTarget(launch) : row.id === null ? null : { key: row.key, id: row.id, editable: "configured" as const, name };
  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`${kind} ${name}`}
      className="home-scope-row"
      style={style}
      title={row.path}
      onClick={trace}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) {
          event.preventDefault();
          trace();
        }
      }}
      onContextMenu={
        target
          ? (event) => {
              onMenu(event, entranceMenu(project, target));
            }
          : undefined
      }
    >
      <span className={row.method === "DELETE" ? "home-scope-kind home-scope-kind-danger" : "home-scope-kind"}>{kind}</span>
      <span className={launchRow ? "home-scope-name home-scope-label" : "home-scope-name"}>{name}</span>
      {launchRow ? (
        <span className="home-scope-detail home-scope-declared">declared in {launch?.declaredIn.path ?? row.path}</span>
      ) : (
        <span className="home-scope-detail">{row.handler}</span>
      )}
      <span className="home-scope-trailing">
        <span className="home-scope-count">{row.reaches === null ? "" : `${formatCount(row.reaches)} fn`}</span>
        <span className="home-scope-trace">Trace →</span>
      </span>
    </div>
  );
}

function SharedRow({ item, open, onToggle, style }: {
  readonly item: Extract<Item, { kind: "shared" }>;
  readonly open: boolean;
  readonly onToggle: () => void;
  readonly style?: CSSProperties | undefined;
}) {
  return (
    <button type="button" className="home-scope-row home-scope-shared" style={style} aria-expanded={open} onClick={onToggle}>
      <span className="home-scope-kind home-scope-shared-count">
        <Icon name={open ? "chevron-down-small" : "chevron-right-small"} size={10} />
        {formatCount(item.section.count)}
      </span>
      <span className="home-scope-name">{item.rows.map((row) => row.trigger ?? row.title).join(", ")}</span>
      <span className="home-scope-detail home-scope-also">
        also in <span className="home-scope-also-names">{item.section.alsoIn.join(", ")}</span>
      </span>
      <span className="home-scope-trailing home-scope-routes">{plural(item.section.count, "route")}</span>
    </button>
  );
}

function ItemView({ item, project, launches, open, onToggle, onMenu, style }: {
  readonly item: Item;
  readonly project: ProjectId;
  readonly launches: readonly LaunchRow[];
  readonly open: ReadonlySet<number>;
  readonly onToggle: (section: number) => void;
  readonly onMenu: OnMenu;
  readonly style?: CSSProperties | undefined;
}) {
  switch (item.kind) {
    case "section":
      return (
        <div className="home-scope-heading" style={style}>
          <span className="home-scope-heading-title">{item.section.title}</span>
          {item.section.mountedFrom === null ? null : <span className="home-scope-heading-note">· mounted from {item.section.mountedFrom}</span>}
        </div>
      );
    case "shared":
      return (
        <SharedRow
          item={item}
          open={open.has(item.index)}
          onToggle={() => {
            onToggle(item.index);
          }}
          style={style}
        />
      );
    case "row":
      return <ScopeRow project={project} row={item.row} launch={launches.find((launch) => launch.key === item.row.key)} onMenu={onMenu} style={style} />;
  }
}

/** A scope's rows; past 100 only the ones in view of `scroller` (else Home's scroll area). */
export function ScopeRows({ project, scope, launches, onMenu, scroller }: {
  readonly project: ProjectId;
  readonly scope: Pick<Scope, "sections" | "rows">;
  /** The application's launches, for the launch rows' names and menus. */
  readonly launches: readonly LaunchRow[];
  readonly onMenu: OnMenu;
  readonly scroller?: HTMLElement | null;
}) {
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  const items = scopeItems(scope.sections, scope.rows, open);
  const { attach, rows: windowRows, height, offsets } = useWindowed({
    count: items.length,
    size: (index) => (items[index]?.kind === "section" ? HEADING : ROW),
    threshold: WINDOW_THRESHOLD,
    ...(scroller === undefined ? {} : { scroller }),
  });
  const toggle = (section: number) => {
    const next = new Set(open);
    if (!next.delete(section)) {
      next.add(section);
    }
    setOpen(next);
  };
  const view = (item: Item, style?: CSSProperties) => (
    <ItemView key={item.key} item={item} project={project} launches={launches} open={open} onToggle={toggle} onMenu={onMenu} style={style} />
  );
  return (
    <div ref={attach} className="home-scope-rows" style={windowRows ? { position: "relative", height } : undefined}>
      {windowRows
        ? windowRows.map((virtual, slot) => {
            const item = items[virtual.index];
            return item ? view(item, { position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${offsets[slot] ?? 0}px)` }) : null;
          })
        : items.map((item) => view(item))}
    </div>
  );
}
