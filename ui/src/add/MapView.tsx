/**
 * Add before typing (03, V-2, V-5, V-64): the Look in column (All projects and each project, with
 * entrance counts) and the map of each shown project's applications as lanes of entrance chips. A chip
 * on the current page is marked "on page"; the chip under the pointer or the keyboard gets a popover
 * after 300 ms with its handler, where it is declared, what it reaches, and where else it is ("Already
 * on <board> › <page>").
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { Monogram, ProgressRing, TextButton } from "../components";
import { Icon } from "../icons";
import { useEngineQuery, type ProjectId, type ProjectRow } from "../ipc";
import { methodColumn, triggerText } from "../lib/entrances";
import { plural } from "../lib/format";
import { cardRefKey, type PageTarget } from "../state/boards";
import type { LaneView, MapChip, ProjectMap } from "./map";

/** The popover waits this long on a chip (V-64). */
export const POPOVER_DELAY = 300;

interface LookInProps {
  readonly projects: readonly ProjectRow[];
  /** Each project's entrance count, once its map loaded. */
  readonly counts: ReadonlyMap<ProjectId, number>;
  readonly value: ProjectId | null;
  readonly onChange: (project: ProjectId | null) => void;
}

function LookInRow({ icon, name, count, selected, onClick }: { readonly icon: ReactNode; readonly name: string; readonly count: number | undefined; readonly selected: boolean; readonly onClick: () => void }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      className={selected ? "add-lookin-row selected" : "add-lookin-row"}
      onMouseDown={(event) => {
        event.preventDefault();
      }}
      onClick={onClick}
    >
      <span className="add-lookin-icon">{icon}</span>
      <span className="add-lookin-name">{name}</span>
      <span className="add-lookin-count">{count === undefined ? "" : count.toLocaleString("en-US")}</span>
    </button>
  );
}

export function LookIn({ projects, counts, value, onChange }: LookInProps) {
  const all = projects.every((row) => counts.has(row.id)) ? [...counts.values()].reduce((sum, count) => sum + count, 0) : undefined;
  return (
    <div className="add-lookin">
      <div className="add-lookin-label" id="add-lookin-label">
        Look in
      </div>
      <div role="radiogroup" aria-labelledby="add-lookin-label" className="add-lookin-rows">
        <LookInRow
          icon={<Icon name="layers" />}
          name="All projects"
          count={all}
          selected={value === null}
          onClick={() => {
            onChange(null);
          }}
        />
        {projects.map((row) => (
          <LookInRow
            key={row.id}
            icon={<Monogram name={row.name} size={20} />}
            name={row.name}
            count={counts.get(row.id)}
            selected={value === row.id}
            onClick={() => {
              onChange(row.id);
            }}
          />
        ))}
      </div>
      <p className="add-lookin-note">Only connected projects are searched. Connect another from Home.</p>
    </div>
  );
}

interface MapViewProps {
  readonly projects: readonly ProjectMap[];
  /** `cardRefKey`s of the cards on the page Add adds to. */
  readonly onPage: ReadonlySet<string>;
  /** Why a project's entrances can't be added here (V-62), if they can't. */
  readonly blocked: (project: ProjectId) => string | null;
  /** The keyboard's chip. */
  readonly activeChip: string | null;
  readonly onActivate: (chip: MapChip) => void;
  /** The chip the popover is about (pointer or keyboard), and whether the pointer is on one. */
  readonly onHover: (chip: Extract<MapChip, { type: "entrance" }> | null) => void;
}

function Chip({ chip, active, onPage, blocked, onActivate, onHover }: { readonly chip: MapChip; readonly active: boolean; readonly onPage: boolean; readonly blocked: string | null } & Pick<MapViewProps, "onActivate" | "onHover">) {
  const common = {
    type: "button" as const,
    tabIndex: -1,
    "data-chip-id": chip.id,
    onMouseDown: (event: { preventDefault: () => void }) => {
      event.preventDefault();
    },
  };
  if (chip.type === "more") {
    return (
      <button
        {...common}
        className={active ? "add-chip more active" : "add-chip more"}
        onClick={() => {
          onActivate(chip);
        }}
      >
        {`+${chip.count.toLocaleString("en-US")} more`}
      </button>
    );
  }
  const method = methodColumn(chip.row);
  const classes = ["add-chip", active && "active", blocked !== null && "blocked"].filter(Boolean).join(" ");
  return (
    <button
      {...common}
      className={classes}
      aria-disabled={blocked !== null || undefined}
      title={blocked ?? undefined}
      onClick={() => {
        onActivate(chip);
      }}
      onMouseEnter={() => {
        onHover(chip);
      }}
      onMouseLeave={() => {
        onHover(null);
      }}
    >
      <span className={method === "DELETE" ? "add-chip-kind danger" : "add-chip-kind"}>{method}</span>
      <span className="add-chip-name">{triggerText(chip.row)}</span>
      {onPage ? (
        <span className="add-chip-on-page">
          <span className="add-chip-dot" aria-hidden="true" />
          on page
        </span>
      ) : null}
    </button>
  );
}

function LaneRow({ view, ...props }: { readonly view: LaneView } & Omit<MapViewProps, "projects">) {
  const { lane, chips } = view;
  const blocked = props.blocked(lane.project);
  return (
    <div className="add-lane">
      <div className="add-lane-name">
        <span className="add-lane-title">{lane.name}</span>
        {lane.command === null ? null : <span className="add-lane-command">{lane.command}</span>}
      </div>
      <div className="add-lane-chips" role="group" aria-label={lane.name}>
        {chips.map((chip) => (
          <Chip
            key={chip.id}
            chip={chip}
            active={chip.id === props.activeChip}
            onPage={chip.type === "entrance" && props.onPage.has(cardRefKey(chip.project, chip.row.key))}
            blocked={blocked}
            onActivate={props.onActivate}
            onHover={props.onHover}
          />
        ))}
      </div>
    </div>
  );
}

function compileLine(row: ProjectRow): string | null {
  if (row.state.kind !== "compiling") {
    return null;
  }
  const { done, total } = row.state;
  const percent = total > 0 ? `${Math.floor((done / total) * 100)}%` : "starting";
  return `compiling · ${percent}, results may be incomplete`;
}

function ProjectGroup({ project, ...props }: { readonly project: ProjectMap } & Omit<MapViewProps, "projects">) {
  const { row } = project;
  const branch = row.git?.branch ?? row.git?.detached ?? null;
  const compiling = compileLine(row);
  return (
    <section className="add-project" aria-label={row.name}>
      <header className="add-project-header">
        <Monogram name={row.name} size={20} />
        <h3 className="add-project-name">{row.name}</h3>
        {branch === null ? null : <span className="add-project-branch">{branch}</span>}
      </header>
      {compiling === null ? null : (
        <div className="add-project-compiling">
          <ProgressRing size={12} />
          {compiling}
        </div>
      )}
      {project.status === "loading" ? <p className="add-project-note">Loading entrances…</p> : null}
      {project.status === "error" ? <p className="add-project-note">{project.message}</p> : null}
      {project.status === "ready" && project.lanes.length === 0 ? <p className="add-project-note">No entrances found.</p> : null}
      {project.status === "ready" ? project.lanes.map((view) => <LaneRow key={view.lane.id} view={view} {...props} />) : null}
    </section>
  );
}

export function MapView({ projects, ...props }: MapViewProps) {
  const scroller = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual returns fresh functions by design.
  const virtualizer = useVirtualizer({
    count: projects.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 160,
    getItemKey: (index) => projects[index]?.row.id ?? index,
    paddingStart: 16,
    paddingEnd: 16,
    gap: 20,
    overscan: 2,
  });

  // The keyboard's chip stays in view: its project first (it may not be rendered), then the chip.
  const { activeChip } = props;
  useEffect(() => {
    if (activeChip === null) {
      return;
    }
    const index = projects.findIndex((project) => project.status === "ready" && project.lanes.some((lane) => lane.chips.some((chip) => chip.id === activeChip)));
    const element = scroller.current?.querySelector(`[data-chip-id="${CSS.escape(activeChip)}"]`);
    if (element) {
      element.scrollIntoView({ block: "nearest" });
    } else if (index >= 0) {
      virtualizer.scrollToIndex(index, { align: "start" });
    }
    // Only a new chip scrolls.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeChip]);

  return (
    <div ref={scroller} className="add-map" aria-label="Entrances">
      <div className="add-map-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtual) => {
          const project = projects[virtual.index];
          return project ? (
            <div key={virtual.key} ref={virtualizer.measureElement} data-index={virtual.index} className="add-map-item" style={{ transform: `translateY(${virtual.start}px)` }}>
              <ProjectGroup project={project} {...props} />
            </div>
          ) : null;
        })}
      </div>
    </div>
  );
}

/** Whether a board ref is the page Add adds to. */
function isPage(ref: { readonly project: ProjectId; readonly board: string; readonly page: string }, page: PageTarget | null): boolean {
  return page !== null && ref.project === page.board.project && ref.board === page.board.id && ref.page === page.page;
}

/** An entrance's popover content: the facts from `preview` and where it already is (`board_refs`). */
function PopoverFacts({ chip, projectName, page, onOpenRef }: { readonly chip: Extract<MapChip, { type: "entrance" }>; readonly projectName: string; readonly page: PageTarget | null; readonly onOpenRef: (ref: { project: ProjectId; board: string; page: string }) => void }) {
  const preview = useEngineQuery("preview", { project: chip.project, key: chip.row.key });
  const refs = useEngineQuery("board_refs", { project: chip.project, key: chip.row.key });
  const data = preview.data?.kind === "entrance" ? preview.data : undefined;
  const elsewhere = (refs.data?.refs ?? []).filter((ref) => !isPage(ref, page));
  const first = elsewhere[0];
  const handler = data?.handlers[0];
  const calls = data && data.externalModules.length > 0 ? ` · calls ${data.externalModules.join(", ")}` : "";
  const declared = data ? (data.declared.line === null ? data.declared.path : `${data.declared.path}:${data.declared.line}`) : "…";
  return (
    <>
      <div className="add-popover-title">
        <span className="add-popover-kind">{methodColumn(chip.row)}</span>
        <span className="add-popover-name">{triggerText(chip.row)}</span>
        <span className="add-popover-context">{`${projectName} · ${chip.lane.name}`}</span>
      </div>
      <dl className="add-popover-facts">
        <div>
          <dt>Handler</dt>
          <dd className="add-code">{handler ? handler.name + (data.handlers.length > 1 ? ` +${data.handlers.length - 1}` : "") : data ? "none resolved" : "…"}</dd>
        </div>
        <div>
          <dt>Declared</dt>
          <dd className="add-code">{declared}</dd>
        </div>
        <div>
          <dt>Reaches</dt>
          <dd>{data ? plural(data.reaches, "function") + calls : "…"}</dd>
        </div>
      </dl>
      {first ? (
        <div className="add-popover-footer">
          <Icon name="page" size={14} />
          <span className="add-popover-already">
            {`Already on ${first.title} › ${first.pageTitle}`}
            {elsewhere.length > 1 ? ` · ${elsewhere.length - 1} more` : ""}
          </span>
          <TextButton
            onMouseDown={(event) => {
              event.preventDefault();
            }}
            onClick={() => {
              onOpenRef(first);
            }}
          >
            Open
          </TextButton>
        </div>
      ) : null}
    </>
  );
}

interface ChipPopoverProps {
  readonly chip: Extract<MapChip, { type: "entrance" }>;
  readonly projectName: string;
  readonly page: PageTarget | null;
  /** Where the chips are drawn (the popover is placed under its chip). */
  readonly container: RefObject<HTMLElement | null>;
  readonly onOpenRef: (ref: { project: ProjectId; board: string; page: string }) => void;
  /** The pointer moved into or out of the popover. */
  readonly onPointer: (inside: boolean) => void;
}

/** 6px under its chip, left edges aligned (03 §4.3); above it when there is no room below. */
export function ChipPopover({ chip, projectName, page, container, onOpenRef, onPointer }: ChipPopoverProps) {
  const layer = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const anchor = container.current?.querySelector(`[data-chip-id="${CSS.escape(chip.id)}"]`);
    const element = layer.current;
    if (!anchor || !element) {
      setPlace(null);
      return undefined;
    }
    // Placed again when its content arrives and changes its height.
    const update = () => {
      const rect = anchor.getBoundingClientRect();
      const height = element.offsetHeight;
      const below = rect.bottom + 6;
      const top = below + height > window.innerHeight - 8 ? Math.max(8, rect.top - 6 - height) : below;
      const left = Math.max(8, Math.min(rect.left, window.innerWidth - element.offsetWidth - 8));
      setPlace({ left, top });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [chip.id, container]);
  return (
    <div
      ref={layer}
      role="tooltip"
      className="add-popover"
      style={place ? { left: place.left, top: place.top } : { visibility: "hidden" }}
      onMouseEnter={() => {
        onPointer(true);
      }}
      onMouseLeave={() => {
        onPointer(false);
      }}
    >
      <PopoverFacts chip={chip} projectName={projectName} page={page} onOpenRef={onOpenRef} />
    </div>
  );
}
