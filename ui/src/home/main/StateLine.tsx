/**
 * Home's state line (07, V-25, V-68): the state's glyph and words, the muted detail, its action link,
 * and an info icon whose tooltip carries the quiet notes (fallback location, watching by scanning).
 */

import { StateIndicator, TextButton, Tooltip, showToast } from "../../components";
import { Icon } from "../../icons";
import { toUnfoldError, useEngineQuery, type ProjectId, type ProjectState, type ProjectSummary } from "../../ipc";
import { cancelCompile, setBackgroundUpdates, useCompileProgress } from "../../state/compile";
import { setHomeView, useLayout } from "../../state/layout";
import { stateLine, type StateAction } from "../stateLine";

function run(action: StateAction, project: ProjectId): void {
  const fail = (title: string) => (error: unknown) => {
    showToast({ title, body: toUnfoldError(error).message, tone: "danger" });
  };
  switch (action) {
    case "cancel":
      cancelCompile(project).catch(fail("Couldn't cancel the compile"));
      return;
    case "resume":
      setBackgroundUpdates(project, true).catch(fail("Couldn't resume background updates"));
      return;
    case "health":
      setHomeView("health");
      return;
  }
}

export interface StateLineProps {
  readonly project: ProjectId;
  readonly state: ProjectState;
  readonly summary: ProjectSummary | undefined;
  readonly branch: string | null;
}

export function StateLine({ project, state, summary, branch }: StateLineProps) {
  const progress = useCompileProgress(project);
  const onHealth = useLayout((layout) => layout.home.view === "health");
  // Whether the files that don't parse still show on the graph decides the problems wording (07 row 5).
  const problems = useEngineQuery("problems", { project }, { enabled: state.kind === "problems" });
  const parse = problems.data?.filter((item) => item.category === "doesnt_parse" || item.category === "cant_read");
  const lastGood = parse === undefined ? undefined : parse.every((item) => item.fileState === "last_good");
  const line = stateLine({ state, summary, progress, branch, ...(lastGood === undefined ? {} : { lastGood }) });
  // On the Health view, "See Health" would lead where the person already is.
  const action = line.action && !(line.action.kind === "health" && onHealth) ? line.action : null;
  return (
    <div className="home-state-line" data-state={state.kind}>
      <StateIndicator state={state} variant="line" />
      <span className="home-state-headline">{line.headline}</span>
      {line.detail.length > 0 ? (
        <span className="home-state-detail">
          {line.detail.map((part, index) => (
            <span key={index} className={part.mono ? "home-state-run home-mono" : "home-state-run"}>
              {part.text}
            </span>
          ))}
        </span>
      ) : null}
      {action ? (
        <span className={action.joined ? "home-state-action home-state-action-joined" : "home-state-action"}>
          <TextButton
            onClick={() => {
              run(action.kind, project);
            }}
          >
            {action.label}
          </TextButton>
        </span>
      ) : null}
      {line.notes.length > 0 ? (
        <Tooltip
          content={
            <>
              {line.notes.map((note) => (
                <p key={note}>{note}</p>
              ))}
            </>
          }
        >
          <span className="home-state-info" role="img" aria-label="About this project's graph" tabIndex={0}>
            <Icon name="info" size={14} />
          </span>
        </Tooltip>
      ) : null}
    </div>
  );
}
