/**
 * How it starts for a project with launches: one row per application under its category (Needs a
 * look, Services, Workers, Scheduled), each launch as `folder $ command` with its schedule and what it
 * handles. An application with entrances besides its launches expands to its launch and routes.
 */

import { useEffect, useState } from "react";
import { SearchField, TextButton } from "../../components";
import { Icon } from "../../icons";
import type { AppRow, Category, CategoryGroup, LaunchRow, ProjectId } from "../../ipc";
import { formatCount, plural } from "../../lib/format";
import { editEntrance, entranceMenu, launchTarget, ScopeRows, useScope, type OnMenu } from "./ScopeRows";
import { moreWords } from "./words";

/** Rows an expanded application shows before "N more · Filter", and per page after it. */
const PREVIEW = 12;
const PAGE = 100;
const FILTER_DELAY = 120;

const BANDS: Readonly<Record<Category, { readonly title: string; readonly note?: string }>> = {
  needs_look: { title: "Needs a look", note: "Unfold can't tell what a launch runs until you point it at the code" },
  services: { title: "Services" },
  workers: { title: "Workers" },
  scheduled: { title: "Scheduled", note: "Schedules as written in unfold.toml · times in the scheduler's zone" },
};

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

/** What a launch handles: the routes it serves, else the functions it reaches. */
function handles(launch: LaunchRow): string {
  if (launch.status === "not_indexed") {
    return "not indexed";
  }
  if (launch.status === "not_found") {
    return "not found";
  }
  if (launch.routes !== null) {
    return plural(launch.routes, "route");
  }
  return launch.reaches === null ? "" : `${formatCount(launch.reaches)} fn`;
}

function LaunchLine({ project, launch, onMenu }: { readonly project: ProjectId; readonly launch: LaunchRow; readonly onMenu: OnMenu }) {
  const note = launch.note ?? (launch.targetSetByHand ? "Target set by hand" : null);
  return (
    <div
      className="home-launch"
      onContextMenu={(event) => {
        onMenu(event, entranceMenu(project, launchTarget(launch)));
      }}
    >
      <div className="home-launch-how">
        <div className="home-launch-command" title={launch.command ?? undefined}>
          <span className="home-launch-folder">{launch.folder} $</span>
          <span className="home-launch-text">{launch.command ?? launch.starts?.path}</span>
        </div>
        {note === null ? null : <div className="home-launch-note">{note}</div>}
      </div>
      {launch.status === "unreadable" ? (
        <div className="home-launch-problem">
          <span className="home-launch-problem-text">{launch.problem}</span>
          <TextButton
            onClick={(event) => {
              event.stopPropagation();
              editEntrance(project, launch);
            }}
          >
            Fix…
          </TextButton>
        </div>
      ) : (
        <>
          <div className="home-launch-when">
            {launch.schedule === null ? null : <span className="home-launch-schedule">{launch.schedule}</span>}
            {launch.scheduleGloss === null ? null : <span className="home-launch-gloss">{launch.scheduleGloss}</span>}
          </div>
          <div className={launch.status === "ok" ? "home-launch-handles" : "home-launch-handles home-launch-handles-quiet"}>{handles(launch)}</div>
        </>
      )}
    </div>
  );
}

/** An expanded application: its launch and routes, then "N more routes · Filter" for the rest. */
function ApplicationEntrances({ project, application, onMenu }: { readonly project: ProjectId; readonly application: AppRow; readonly onMenu: OnMenu }) {
  const [all, setAll] = useState(false);
  const [filter, setFilter] = useState("");
  const query = useDebounced(filter.trim(), FILTER_DELAY);
  const scope = useScope(project, { application: application.id }, all ? query : "", all ? PAGE : PREVIEW);
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const shown = scope.rows.length;
  const total = scope.total ?? 0;

  if (scope.error !== null) {
    return <p className="home-app-entrances home-scope-none">{scope.error}</p>;
  }
  if (!all) {
    return (
      <div className="home-app-entrances">
        <ScopeRows project={project} scope={scope} launches={application.launches} onMenu={onMenu} />
        {total > shown ? (
          <div className="home-scope-footer">
            <span>{moreWords(total - shown, scope.rows)}</span>
            <span className="home-scope-footer-dot">·</span>
            <TextButton
              onClick={() => {
                setAll(true);
              }}
            >
              Filter
            </TextButton>
          </div>
        ) : null}
      </div>
    );
  }
  return (
    <div className="home-app-entrances">
      <div className="home-scope-filter">
        <SearchField value={filter} onChange={setFilter} label={`Filter ${application.name}'s entrances`} placeholder="Filter" autoFocus />
      </div>
      <div ref={setScroller} className="home-scope-scroller">
        <ScopeRows project={project} scope={scope} launches={application.launches} onMenu={onMenu} scroller={scroller} />
        {scope.total === 0 ? <p className="home-scope-none">No entrance matches “{query}”.</p> : null}
      </div>
      <div className="home-scope-footer">
        <span>{plural(total, "entrance")}</span>
        {scope.more ? (
          <>
            <span className="home-scope-footer-dot">·</span>
            <TextButton onClick={scope.more}>Show {formatCount(Math.min(PAGE, total - shown))} more</TextButton>
          </>
        ) : null}
        <span className="home-scope-footer-dot">·</span>
        <TextButton
          onClick={() => {
            setAll(false);
            setFilter("");
          }}
        >
          Show fewer
        </TextButton>
      </div>
    </div>
  );
}

function ApplicationRow({ project, application, expanded, onToggle, onMenu }: {
  readonly project: ProjectId;
  readonly application: AppRow;
  readonly expanded: boolean | null;
  readonly onToggle: () => void;
  readonly onMenu: OnMenu;
}) {
  return (
    <section className="home-app" aria-label={application.name}>
      <div className="home-app-row" onClick={expanded === null ? undefined : onToggle}>
        <span className="home-app-chevron">
          {expanded === null ? null : (
            <button type="button" className="home-app-toggle" aria-expanded={expanded} aria-label={`${application.name}'s entrances`}>
              <Icon name={expanded ? "chevron-down-small" : "chevron-right-small"} size={10} />
            </button>
          )}
        </span>
        <div className="home-app-name">
          <h4 className="home-app-title">{application.name}</h4>
          {application.description === null ? null : <p className="home-app-description">{application.description}</p>}
        </div>
        <div className="home-app-launches">
          {application.launches.length > 0 ? (
            application.launches.map((launch) => <LaunchLine key={launch.key} project={project} launch={launch} onMenu={onMenu} />)
          ) : (
            <div className="home-launch">
              <div className="home-launch-how home-launch-none">No launch defined</div>
              <div className="home-launch-when" />
              <div className="home-launch-handles">{application.routes > 0 ? plural(application.routes, "route") : ""}</div>
            </div>
          )}
        </div>
      </div>
      {expanded ? <ApplicationEntrances project={project} application={application} onMenu={onMenu} /> : null}
    </section>
  );
}

function Band({ group }: { readonly group: CategoryGroup }) {
  const band = BANDS[group.category];
  return (
    <div className={`home-band home-band-${group.category}`}>
      <span className="home-band-mark" />
      <h3 className="home-band-title">{band.title}</h3>
      <span className="home-band-count">{formatCount(group.applications.length)}</span>
      {band.note === undefined ? null : <span className="home-band-note">{band.note}</span>}
    </div>
  );
}

export function LaunchTable({ project, categories, onMenu }: { readonly project: ProjectId; readonly categories: readonly CategoryGroup[]; readonly onMenu: OnMenu }) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const toggle = (id: string) => {
    const next = new Set(expanded);
    if (!next.delete(id)) {
      next.add(id);
    }
    setExpanded(next);
  };
  return (
    <div className="home-launch-table">
      <div className="home-launch-columns" aria-hidden="true">
        <span className="home-app-chevron" />
        <span className="home-launch-column-app">Application</span>
        <span className="home-launch-column-how">How it starts</span>
        <span className="home-launch-column-when">When</span>
        <span className="home-launch-column-handles">Handles</span>
      </div>
      {categories.map((group) => (
        <div key={group.category} className="home-band-group">
          <Band group={group} />
          {group.applications.map((application) => (
            <ApplicationRow
              key={application.id}
              project={project}
              application={application}
              expanded={application.entrances > application.launches.length ? expanded.has(application.id) : null}
              onToggle={() => {
                toggle(application.id);
              }}
              onMenu={onMenu}
            />
          ))}
        </div>
      ))}
    </div>
  );
}
