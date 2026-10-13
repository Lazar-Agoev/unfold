/**
 * Health and activity: a mode of Home under the project header with a Health |
 * Activity switch showing one list at a time. Health: every problem with its action, the boards
 * that need review, and a quiet note. Activity: the index log as a table, newest first.
 */

import { useMemo, type CSSProperties } from "react";
import { SegmentedControl } from "../../components";
import { Icon } from "../../icons";
import { useEngineQuery, type ProjectId, type ProjectSummary } from "../../ipc";
import { setHomeView } from "../../state/layout";
import { reviewBoard } from "../actions";
import { useWindowed } from "../windowed";
import { logRows } from "./activity";
import { HealthRowView, actionLabel, runHealthAction } from "./HealthRow";
import { boardHealthRows, healthRows } from "./problems";
import "./health.css";

/** Row heights as drawn, and how many rows render in full before only those in view do. */
const HEALTH_ROW = 63;
const LOG_ROW = 32;
const WINDOW_THRESHOLD = 200;

function HealthList({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  const problems = useEngineQuery("problems", { project });
  const rows = useMemo(() => healthRows(problems.data ?? []), [problems.data]);
  const boards = useMemo(() => boardHealthRows(summary?.health.boards ?? [], problems.data ?? []), [summary, problems.data]);
  const { attach, rows: windowRows, height: windowHeight, offsets } = useWindowed({ count: rows.length, size: () => HEALTH_ROW, threshold: WINDOW_THRESHOLD });

  if (problems.error) {
    return <p className="home-health-quiet">{problems.error.message}</p>;
  }
  if (!problems.data) {
    return null;
  }
  const render = (index: number, style?: CSSProperties) => {
    const row = rows[index];
    if (!row) {
      return null;
    }
    const { action } = row;
    return (
      <HealthRowView
        key={row.id}
        row={row}
        action={action ? { label: actionLabel(action), run: () => { runHealthAction(project, action); } } : null}
        {...(style ? { style } : {})}
      />
    );
  };
  return (
    <>
      {rows.length > 0 ? (
        <div ref={attach} className="home-health-list" aria-label="Problems" style={windowRows ? { position: "relative", height: windowHeight } : undefined}>
          {windowRows
            ? windowRows.map((item, slot) => render(item.index, { position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${offsets[slot] ?? 0}px)` }))
            : rows.map((_, index) => render(index))}
        </div>
      ) : boards.length === 0 ? (
        <div className="home-health-list">
          <HealthRowView icon="check" row={{ title: "Nothing needs a look", explanation: "Every file parses and every board is healthy.", location: "" }} />
        </div>
      ) : null}
      {boards.length > 0 ? (
        <div className="home-health-boards">
          <h3 className="home-caps home-health-boards-label">Boards</h3>
          {boards.map((board) => (
            <HealthRowView
              key={board.id}
              icon="board"
              inline
              row={{ title: board.title, explanation: board.explanation, location: board.board }}
              action={{ label: "Review", run: () => void reviewBoard({ project, id: board.id }) }}
            />
          ))}
        </div>
      ) : null}
      <p className="home-health-quiet">
        <Icon name="info" size={14} />
        Calls Unfold can't follow aren't problems; they're shown on the cards.
      </p>
    </>
  );
}

function ActivityLog({ project }: { readonly project: ProjectId }) {
  const log = useEngineQuery("index_log", { project });
  const rows = useMemo(() => logRows(log.data?.entries ?? []), [log.data]);
  const { attach, rows: windowRows, height: windowHeight, offsets } = useWindowed({ count: rows.length, size: () => LOG_ROW, threshold: WINDOW_THRESHOLD });
  const render = (index: number, style?: CSSProperties) => {
    const row = rows[index];
    return row ? (
      <div key={`${row.id}:${index}`} role="row" className="home-log-row" style={style}>
        <span role="cell" className="home-log-time">{row.time}</span>
        <span role="cell" className="home-log-what" title={row.what === row.action ? undefined : row.action}>
          {row.what}
        </span>
        <span role="cell" className="home-log-files">{row.files}</span>
        <span role="cell" className="home-log-duration">{row.duration}</span>
      </div>
    ) : null;
  };
  return (
    <div className="home-activity">
      <div className="home-activity-header">
        <p className="home-activity-note">Every update to the graph, newest first</p>
      </div>
      {log.error ? <p className="home-health-quiet">{log.error.message}</p> : null}
      {log.data && rows.length === 0 ? <p className="home-health-quiet">Nothing compiled yet. Each compile and background update is listed here.</p> : null}
      {rows.length > 0 ? (
        <div className="home-log" role="table" aria-label="Activity">
          <div role="row" className="home-log-row home-log-head">
            <span role="columnheader" className="home-log-time">Time</span>
            <span role="columnheader" className="home-log-what">What</span>
            <span role="columnheader" className="home-log-files">Files read / cached / failed</span>
            <span role="columnheader" className="home-log-duration">Duration</span>
          </div>
          <div ref={attach} role="rowgroup" style={windowRows ? { position: "relative", height: windowHeight } : undefined}>
            {windowRows
              ? windowRows.map((item, slot) => render(item.index, { position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${offsets[slot] ?? 0}px)` }))
              : rows.map((_, index) => render(index))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export interface HealthActivityProps {
  readonly project: ProjectId;
  readonly view: "health" | "activity";
  readonly summary: ProjectSummary | undefined;
}

export function HealthActivity({ project, view, summary }: HealthActivityProps) {
  const problems = useEngineQuery("problems", { project });
  const listed = useMemo(() => (problems.data ? healthRows(problems.data).length : undefined), [problems.data]);
  const count = listed === undefined ? undefined : listed + (summary?.health.boards.length ?? 0);
  return (
    <section className="home-health" aria-label="Health and activity">
      <div className="home-health-header">
        <h2 className="home-section-title">Health and activity</h2>
        <SegmentedControl
          variant="switch"
          label="Show"
          value={view}
          onChange={setHomeView}
          options={[{ value: "health", label: "Health", ...(count === undefined ? {} : { count }) }, { value: "activity", label: "Activity" }]}
        />
        <button
          type="button"
          className="home-health-back"
          onClick={() => {
            setHomeView("overview");
          }}
        >
          <Icon name="chevron-left" size={14} />
          Back to overview
        </button>
      </div>
      {view === "health" ? <HealthList project={project} summary={summary} /> : <ActivityLog project={project} />}
    </section>
  );
}
