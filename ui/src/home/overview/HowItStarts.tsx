/**
 * How it starts (01, V-48, V-19): one card per application with its launch command and its first
 * eight entrances (hover one to Trace it onto a new draft); "N more · Filter" expands the card in place
 * to every entrance with a filter; then the entrances no launch reaches and the launches into code that
 * isn't indexed, each collapsed.
 */

import { useEffect, useState, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { SearchField, TextButton, useContextMenu, type MenuEntry } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type ApplicationEntrances, type EntranceRow, type LaunchRow, type ProjectId, type ProjectSummary } from "../../ipc";
import { methodColumn, triggerText } from "../../lib/entrances";
import { plural } from "../../lib/format";
import { openDialog } from "../../state/overlays";
import { traceFromHome } from "../actions";
import { useWindowed } from "../windowed";
import { applicationRows, launchCommand, overflowWords, widestApplication } from "./entrances";

/** Entrances per application on Home (V-19), and in an expanded card (contract ≤ 500). */
const SHOWN = 8;
const ALL = 500;
const ROW = 28;
/** Past this many rows an expanded card renders only the rows in view. */
const WINDOW_THRESHOLD = 100;
const FILTER_DELAY = 120;

function addEntrance(project: ProjectId): void {
  openDialog({ kind: "entranceEditor", project, mode: "add" });
}

/** A manual entrance's menu (V-n: only manual entrances can be edited or removed here). */
function entranceMenu(project: ProjectId, row: EntranceRow): MenuEntry[] {
  const entrance = { key: row.key, detector: row.detector, title: row.title };
  return [
    { id: "trace", label: "Trace on a New Board", icon: "board-draft", onSelect: () => void traceFromHome(project, row.key) },
    { kind: "separator", id: "s1" },
    { id: "edit", label: "Edit…", icon: "edit", onSelect: () => { openDialog({ kind: "entranceEditor", project, mode: "edit", entrance }); } },
    { id: "remove", label: "Remove…", danger: true, onSelect: () => { openDialog({ kind: "removeEntrance", project, entrance }); } },
  ];
}

interface EntranceRowProps {
  readonly project: ProjectId;
  readonly row: EntranceRow;
  readonly onMenu: (event: MouseEvent, entries: MenuEntry[]) => void;
  readonly style?: CSSProperties;
}

function EntranceRowView({ project, row, onMenu, style }: EntranceRowProps) {
  const method = methodColumn(row);
  const trace = () => void traceFromHome(project, row.key);
  return (
    <div
      role="button"
      tabIndex={0}
      className="home-entrance-row"
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
        row.detector === "manual"
          ? (event) => {
              onMenu(event, entranceMenu(project, row));
            }
          : undefined
      }
    >
      <span className={method === "DELETE" ? "home-entrance-method home-entrance-method-danger" : "home-entrance-method"}>{method}</span>
      <span className="home-entrance-name">{triggerText(row)}</span>
      <span className="home-entrance-trailing">
        <span className="home-entrance-count">{row.reaches === null ? "" : `${row.reaches.toLocaleString("en-US")} fn`}</span>
        <span className="home-entrance-trace">Trace →</span>
      </span>
    </div>
  );
}

function useDebounced(value: string, delay: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      setDebounced(value);
    }, delay);
    return () => {
      window.clearTimeout(timer);
    };
  }, [value, delay]);
  return debounced;
}

/** An expanded application (V-48): a filter and every entrance, scrolling inside the card. */
function AllEntrances({ project, application, onMenu, onCollapse }: {
  readonly project: ProjectId;
  readonly application: ApplicationEntrances;
  readonly onMenu: EntranceRowProps["onMenu"];
  readonly onCollapse: () => void;
}) {
  const [filter, setFilter] = useState("");
  const query = useDebounced(filter.trim(), FILTER_DELAY);
  const result = useEngineQuery("entrances", {
    project,
    application: application.startup,
    perApplication: ALL,
    ...(query === "" ? {} : { filter: query }),
  });
  const match = result.data?.applications[0];
  const rows = match?.entrances ?? [];
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const { attach, rows: windowRows, height: windowHeight, offsets } = useWindowed({ count: rows.length, size: () => ROW, threshold: WINDOW_THRESHOLD, scroller });
  return (
    <>
      <div className="home-entrance-filter">
        <SearchField value={filter} onChange={setFilter} label={`Filter ${application.name}'s entrances`} placeholder="Filter" autoFocus />
      </div>
      <div ref={setScroller} className="home-entrance-list home-entrance-list-all">
        <div ref={attach} style={windowRows ? { position: "relative", height: windowHeight } : undefined}>
          {windowRows
            ? windowRows.map((item, slot) => {
                const row = rows[item.index];
                return row ? (
                  <EntranceRowView
                    key={row.key}
                    project={project}
                    row={row}
                    onMenu={onMenu}
                    style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${offsets[slot] ?? 0}px)` }}
                  />
                ) : null;
              })
            : rows.map((row) => <EntranceRowView key={row.key} project={project} row={row} onMenu={onMenu} />)}
        </div>
        {result.data && rows.length === 0 ? <p className="home-entrance-none">No entrance matches “{query}”.</p> : null}
      </div>
      <div className="home-application-footer">
        <span>{plural(match?.total ?? application.total, "entrance")}</span>
        <span>·</span>
        <TextButton onClick={onCollapse}>Show fewer</TextButton>
      </div>
    </>
  );
}

function ApplicationCard({ project, application, wide, expanded, onExpand, onCollapse, onMenu }: {
  readonly project: ProjectId;
  readonly application: ApplicationEntrances;
  readonly wide: boolean;
  readonly expanded: boolean;
  readonly onExpand: () => void;
  readonly onCollapse: () => void;
  readonly onMenu: EntranceRowProps["onMenu"];
}) {
  const launch = application.launches[0];
  const more = application.total > application.entrances.length;
  return (
    <section
      className="home-application"
      style={{ flexGrow: wide ? 1.4 : 1 }}
      aria-label={application.name}
      onContextMenu={(event) => {
        if (!event.defaultPrevented) {
          onMenu(event, [{ id: "add", label: "Add Entrance…", icon: "plus", onSelect: () => { addEntrance(project); } }]);
        }
      }}
    >
      <header className="home-application-header">
        <h3 className="home-application-name">{application.name}</h3>
        {launch ? <span className="home-application-launch">{launchCommand(launch)}</span> : null}
      </header>
      {expanded ? (
        <AllEntrances project={project} application={application} onMenu={onMenu} onCollapse={onCollapse} />
      ) : (
        <>
          <div className="home-entrance-list">
            {application.entrances.map((row) => (
              <EntranceRowView key={row.key} project={project} row={row} onMenu={onMenu} />
            ))}
          </div>
          {more ? (
            <div className="home-application-footer">
              <span>{overflowWords(application)}</span>
              <span>·</span>
              <TextButton onClick={onExpand}>Filter</TextButton>
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}

function LaunchRowView({ launch }: { readonly launch: LaunchRow }) {
  return (
    <div className="home-launch-row" title={launch.note ?? undefined}>
      <span className="home-entrance-name">{launchCommand(launch)}</span>
      <span className="home-launch-path">{launch.note ?? launch.startup ?? launch.path}</span>
    </div>
  );
}

function Collapsed({ label, count, children }: { readonly label: string; readonly count: number; readonly children: () => ReactNode }) {
  const [open, setOpen] = useState(false);
  if (count === 0) {
    return null;
  }
  return (
    <div className="home-collapsed">
      <button
        type="button"
        className="home-collapsed-row"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span className="home-collapsed-chevron">
          <Icon name={open ? "chevron-down-small" : "chevron-right-small"} size={10} />
        </span>
        {label} · {count.toLocaleString("en-US")}
      </button>
      {open ? <div className="home-collapsed-body">{children()}</div> : null}
    </div>
  );
}

/** The entrances no launch reaches, every one (asked for only while the section is open). */
function UnreachedRows({ project, onMenu }: { readonly project: ProjectId; readonly onMenu: EntranceRowProps["onMenu"] }) {
  const all = useEngineQuery("entrances", { project, perApplication: ALL });
  return (
    <div className="home-entrance-list">
      {(all.data?.unreached.entrances ?? []).map((row) => (
        <EntranceRowView key={row.key} project={project} row={row} onMenu={onMenu} />
      ))}
    </div>
  );
}

function headline(summary: ProjectSummary | undefined, compiling: boolean): string {
  if (!summary) {
    return "";
  }
  if (summary.entrances === 0) {
    return compiling ? "Entrances show up here as the compile finds them." : "No entrances found yet. Add one, or check the project's settings.";
  }
  // Entrances no launch reaches belong to no application: don't count "0 applications".
  const where = summary.applications === 0 ? "" : ` in ${plural(summary.applications, "application")}`;
  return `${plural(summary.entrances, "entrance")}${where}. Hover one to trace it on a new board.`;
}

export function HowItStarts({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  const map = useEngineQuery("entrances", { project, perApplication: SHOWN });
  const [expanded, setExpanded] = useState<string | null>(null);
  const menu = useContextMenu("Entrance menu");
  const applications = map.data?.applications ?? [];
  const onMenu = (event: MouseEvent, entries: MenuEntry[]) => {
    menu.open(event, entries);
  };
  const compiling = summary?.state.kind === "compiling";
  const launches = map.data?.unindexedLaunches ?? [];

  return (
    <section className="home-starts" aria-label="How it starts">
      <div className="home-starts-header">
        <h2 className="home-section-title">How it starts</h2>
        <p className="home-starts-summary">{map.error ? map.error.message : headline(summary, compiling)}</p>
        <button
          type="button"
          className="home-add-entrance"
          onClick={() => {
            addEntrance(project);
          }}
        >
          <Icon name="plus" size={14} />
          Add entrance
        </button>
      </div>
      {applications.length > 0 ? (
        <div className="home-applications">
          {applicationRows(applications).map((row) => {
            const wide = widestApplication(row);
            return (
              <div key={row[0]?.startup} className="home-applications-row">
                {row.map((application) => (
                  <ApplicationCard
                    key={application.startup}
                    project={project}
                    application={application}
                    wide={application.startup === wide}
                    expanded={expanded === application.startup}
                    onExpand={() => {
                      setExpanded(application.startup);
                    }}
                    onCollapse={() => {
                      setExpanded(null);
                    }}
                    onMenu={onMenu}
                  />
                ))}
              </div>
            );
          })}
        </div>
      ) : null}
      {map.data ? (
        <div className="home-collapsed-sections">
          <Collapsed label="Not reached from a launch" count={map.data.unreached.total}>
            {() => <UnreachedRows project={project} onMenu={onMenu} />}
          </Collapsed>
          <Collapsed label="Launches into code that isn't indexed" count={launches.length}>
            {() => (
              <div className="home-entrance-list">
                {launches.map((launch) => (
                  <LaunchRowView key={launch.key} launch={launch} />
                ))}
              </div>
            )}
          </Collapsed>
        </div>
      ) : null}
      {menu.element}
    </section>
  );
}
