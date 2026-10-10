/**
 * The Health line at the bottom of the overview: what is fine with a check,
 * what needs a look in amber with its action, and the way into Health and activity.
 */

import { TextButton } from "../../components";
import { Icon } from "../../icons";
import type { ProjectId, ProjectSummary } from "../../ipc";
import { plural } from "../../lib/format";
import { setHomeView } from "../../state/layout";
import { openDialog } from "../../state/overlays";
import { reviewBoard } from "../actions";
import { reviewWords, settingsSection } from "../health/problems";

interface StripItem {
  readonly id: string;
  readonly tone: "ok" | "review" | "note";
  readonly text: string;
  readonly action?: { readonly label: string; readonly run: () => void };
}

/** The strip's items, from `project_summary.health`. */
function stripItems(project: ProjectId, summary: ProjectSummary): StripItem[] {
  const { health } = summary;
  const showHealth = { label: "See Health", run: () => { setHomeView("health"); } };
  const items: StripItem[] = [];
  items.push(
    health.doesntParse === 0
      ? { id: "parse", tone: "ok", text: "All files parse" }
      : { id: "parse", tone: "review", text: `${plural(health.doesntParse, "file")} ${health.doesntParse === 1 ? "doesn't" : "don't"} parse`, action: showHealth },
  );
  if (health.settingsError !== null) {
    const section = settingsSection(health.settingsError);
    items.push({
      id: "settings",
      tone: "review",
      text: "The settings file can't be used",
      action: { label: "Settings", run: () => { openDialog({ kind: "projectSettings", project, section }); } },
    });
  }
  items.push(
    health.unindexedLaunches === 0
      ? { id: "launches", tone: "ok", text: "Every entrance reaches indexed code" }
      : { id: "launches", tone: "note", text: `${plural(health.unindexedLaunches, "launch", "launches")} into code that isn't indexed` },
  );
  const [first] = health.boards;
  if (first && health.boardsNeedingReview === 1) {
    items.push({
      id: "boards",
      tone: "review",
      text: `${reviewWords(first.items)} in ${first.title}`,
      action: { label: "Review", run: () => void reviewBoard({ project, id: first.id }) },
    });
  } else if (health.boardsNeedingReview > 1) {
    items.push({ id: "boards", tone: "review", text: `${plural(health.boardsNeedingReview, "board")} need review`, action: showHealth });
  }
  return items;
}

export function HealthStrip({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary }) {
  return (
    <section className="home-health-strip" aria-label="Health">
      <h2 className="home-health-label">Health</h2>
      {stripItems(project, summary).map((item) => (
        <span key={item.id} className={`home-health-item home-health-item-${item.tone}`}>
          <span className="home-health-mark">{item.tone === "ok" ? <Icon name="check" size={14} /> : item.tone === "review" ? <span className="home-health-dot" /> : null}</span>
          <span className="home-health-text">{item.text}</span>
          {item.action ? (
            <>
              <span className="home-health-sep">·</span>
              <TextButton onClick={item.action.run}>{item.action.label}</TextButton>
            </>
          ) : null}
        </span>
      ))}
      <button
        type="button"
        className="home-health-link"
        onClick={() => {
          setHomeView("health");
        }}
      >
        Health and activity
        <Icon name="chevron-right-small" size={10} />
      </button>
    </section>
  );
}
