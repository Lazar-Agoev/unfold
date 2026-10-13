/**
 * Home's state line: the state's glyph and words, the muted detail, its action link,
 * and an info icon whose tooltip carries the quiet note (watching by scanning).
 */

import { StateIndicator, TextButton, Tooltip, showToast } from "../../components";
import { Icon } from "../../icons";
import { toUnfoldError, useEngineQuery, type ProjectId, type ProjectState, type ProjectSummary } from "../../ipc";
import { cancelCompile, useCompileProgress } from "../../state/compile";
import { setHomeView, useLayout } from "../../state/layout";
import { stateLine, type StateAction } from "../stateLine";

function run(action: StateAction, project: ProjectId): void {
  switch (action) {
    case "cancel":
      cancelCompile(project).catch((error: unknown) => {
        showToast({ title: "Couldn't cancel the compile", body: toUnfoldError(error).message, tone: "danger" });
      });
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
}

export function StateLine({ project, state, summary }: StateLineProps) {
  const progress = useCompileProgress(project);
  const onHealth = useLayout((layout) => layout.home.view === "health");
  // Whether the files that don't parse still show on the graph decides the problems wording.
  const problems = useEngineQuery("problems", { project }, { enabled: state.kind === "problems" });
  const parse = problems.data?.filter((item) => item.category === "doesnt_parse" || item.category === "cant_read");
  const lastGood = parse === undefined ? undefined : parse.every((item) => item.fileState === "last_good");
  const line = stateLine({ state, summary, progress, ...(lastGood === undefined ? {} : { lastGood }) });
  // On the Health view, "See Health" would lead where the person already is.
  const action = line.action && !(line.action.kind === "health" && onHealth) ? line.action : null;
  return (
    <div className="home-state-line" data-state={state.kind}>
      <StateIndicator state={state} variant="line" />
      <span className="home-state-headline">{line.headline}</span>
      {line.detail === "" ? null : <span className="home-state-detail">{line.detail}</span>}
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
