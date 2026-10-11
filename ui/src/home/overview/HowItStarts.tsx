/**
 * How it starts: the project's applications and their launches from the entrance catalog, as a
 * table by category; or, before any launch, how to tell Unfold how the project starts. Then the
 * entrances no launch reaches, collapsed.
 */

import { useState, type MouseEvent, type ReactNode } from "react";
import { TextButton, useContextMenu, type MenuEntry } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type EntranceOverview, type ProjectId, type ProjectSummary } from "../../ipc";
import { formatCount } from "../../lib/format";
import { openDialog } from "../../state/overlays";
import { LaunchTable } from "./LaunchTable";
import { ScopeRows, useScope, type OnMenu } from "./ScopeRows";
import { SetUpLaunches } from "./SetUpLaunches";
import { summaryLine } from "./words";
import "./starts.css";

/** Unreached rows per page. */
const PAGE = 100;

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
        {label} · {formatCount(count)}
      </button>
      {open ? <div className="home-collapsed-body">{children()}</div> : null}
    </div>
  );
}

/** The entrances no launch reaches, a page at a time (asked for only while the section is open). */
function Unreached({ project, onMenu }: { readonly project: ProjectId; readonly onMenu: OnMenu }) {
  const scope = useScope(project, { unreached: true }, "", PAGE);
  if (scope.error !== null) {
    return <p className="home-scope-none">{scope.error}</p>;
  }
  const more = (scope.total ?? 0) - scope.rows.length;
  return (
    <>
      <ScopeRows project={project} scope={scope} launches={[]} onMenu={onMenu} />
      {scope.more ? (
        <div className="home-scope-footer">
          <span>{formatCount(more)} more</span>
          <span className="home-scope-footer-dot">·</span>
          <TextButton onClick={scope.more}>Show more</TextButton>
        </div>
      ) : null}
    </>
  );
}

export function HowItStarts({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  const catalog = useEngineQuery("entrances", { project });
  const overview = catalog.data?.view === "overview" ? (catalog.data as EntranceOverview) : undefined;
  const menu = useContextMenu("Entrance menu");
  const onMenu = (event: MouseEvent, entries: MenuEntry[]) => {
    menu.open(event, entries);
  };
  const compiling = summary?.state.kind === "compiling";
  const empty = overview?.counts.launches === 0 && overview.counts.routes === 0;

  return (
    <section className="home-starts" aria-label="How it starts">
      <div className="home-starts-header">
        <h2 className="home-section-title">How it starts</h2>
        <p className="home-starts-summary">{catalog.error ? catalog.error.message : overview ? summaryLine(overview.counts, compiling) : ""}</p>
        <button
          type="button"
          className="home-add-launch"
          onClick={() => {
            openDialog({ kind: "launchEditor", project, mode: "add" });
          }}
        >
          <Icon name="plus" size={14} />
          Add launch
        </button>
      </div>
      {overview === undefined ? null : overview.counts.launches > 0 ? (
        <LaunchTable project={project} categories={overview.categories} onMenu={onMenu} />
      ) : empty && compiling ? null : (
        <SetUpLaunches project={project} name={summary?.name ?? "this project"} />
      )}
      {overview ? (
        <Collapsed label="Not reached from a launch" count={overview.unreached}>
          {() => <Unreached project={project} onMenu={onMenu} />}
        </Collapsed>
      ) : null}
      {menu.element}
    </section>
  );
}
