/**
 * The Health line at the bottom of the overview: what needs a look in amber, what is fine with a check.
 * An amber item leads to it: the one board that needs review, else the Health view; entrances whose code
 * isn't found, to How it starts' problems. A project with no apps says nothing about entrances.
 */

import { Icon } from "../../icons";
import { useEngineQuery, type ProjectId, type ProjectSummary } from "../../ipc";
import { plural } from "../../lib/format";
import { setHomeView } from "../../state/layout";
import { reviewBoard } from "../actions";

interface LineItem {
  readonly id: string;
  readonly text: string;
  /** What an amber item opens; a fine item has none. */
  readonly open?: () => void;
}

function showHealth(): void {
  setHomeView("health");
}

/** The line's items, from `project_summary.health`; `entrances` says whether the project lists any apps. */
function healthItems(project: ProjectId, { health }: ProjectSummary, entrances: boolean, showEntrances: () => void): LineItem[] {
  const items: LineItem[] = [];
  const [first] = health.boards;
  if (health.boardsNeedingReview === 1 && first) {
    items.push({ id: "boards", text: "1 board needs review", open: () => void reviewBoard({ project, id: first.id }) });
  } else if (health.boardsNeedingReview > 1) {
    items.push({ id: "boards", text: `${plural(health.boardsNeedingReview, "board")} need review`, open: showHealth });
  }
  items.push(
    health.doesntParse === 0
      ? { id: "parse", text: "All files parse" }
      : { id: "parse", text: `${plural(health.doesntParse, "file")} ${health.doesntParse === 1 ? "doesn't" : "don't"} parse`, open: showHealth },
  );
  const notFound = health.entrancesNotFound;
  if (notFound > 0) {
    items.push({ id: "entrances", text: `${notFound} ${notFound === 1 ? "entrance's" : "entrances'"} code isn't found`, open: showEntrances });
  } else if (entrances) {
    items.push({ id: "entrances", text: "Every entrance finds its code" });
  }
  return items;
}

export interface HealthLineProps {
  readonly project: ProjectId;
  readonly summary: ProjectSummary;
  /** Shows How it starts' problems. */
  readonly onShowEntrances: () => void;
}

export function HealthLine({ project, summary, onShowEntrances }: HealthLineProps) {
  const apps = useEngineQuery("entrances", { project }).data?.apps.length ?? 0;
  return (
    <section className="home-health-line" aria-label="Health">
      <h2 className="home-health-label">Health</h2>
      {healthItems(project, summary, apps > 0, onShowEntrances).map((item) =>
        item.open ? (
          <button key={item.id} type="button" className="home-health-item home-health-item-review" onClick={item.open}>
            <span className="home-health-mark">
              <span className="home-health-dot" />
            </span>
            <span className="home-health-text">{item.text}</span>
          </button>
        ) : (
          <span key={item.id} className="home-health-item">
            <span className="home-health-mark">
              <Icon name="check" size={14} />
            </span>
            <span className="home-health-text">{item.text}</span>
          </span>
        ),
      )}
    </section>
  );
}
