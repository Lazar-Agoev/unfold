/**
 * Health: a mode of Home under the project header. Every problem with its action, the boards that need
 * review, and a quiet note.
 */

import { useMemo, type CSSProperties } from "react";
import { Icon } from "../../icons";
import { useEngineQuery, type ProjectId, type ProjectSummary } from "../../ipc";
import { setHomeView } from "../../state/layout";
import { reviewBoard } from "../actions";
import { useWindowed } from "../windowed";
import { ACTION_LABEL, HealthRowView, runHealthAction } from "./HealthRow";
import { boardHealthRows, healthRows } from "./problems";
import "./health.css";

/** A row's height as drawn, and how many rows render in full before only those in view do. */
const HEALTH_ROW = 63;
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
        action={action ? { label: ACTION_LABEL, run: () => { runHealthAction(project, action); } } : null}
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

export function HealthView({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  return (
    <section className="home-health" aria-label="Health">
      <div className="home-health-header">
        <h2 className="home-section-title">Health</h2>
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
      <HealthList project={project} summary={summary} />
    </section>
  );
}
