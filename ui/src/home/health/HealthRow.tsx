/** One Health row (11 "Health list"): warning icon, title and explanation, location, and its action. */

import type { CSSProperties } from "react";
import { Button } from "../../components";
import { Icon, type IconName } from "../../icons";
import type { ProjectId } from "../../ipc";
import { openDialog, openPeek } from "../../state/overlays";
import type { HealthAction, HealthRow } from "./problems";

export function runHealthAction(project: ProjectId, action: HealthAction): void {
  if (action.kind === "open") {
    openPeek({ project, path: action.path, ...(action.line === null ? {} : { line: action.line }) });
  } else {
    openDialog({ kind: "projectSettings", project, section: action.section });
  }
}

export interface HealthRowViewProps {
  readonly row: Pick<HealthRow, "title" | "explanation" | "location">;
  readonly icon?: IconName;
  /** "board" rows set title and explanation on one line (11 "Boards health"). */
  readonly inline?: boolean;
  readonly action?: { readonly label: string; readonly run: () => void } | null;
  readonly style?: CSSProperties;
}

export function HealthRowView({ row, icon = "warning-circle", inline = false, action = null, style }: HealthRowViewProps) {
  return (
    <div className={inline ? "home-health-row home-health-row-inline" : "home-health-row"} style={style}>
      <span className={icon === "warning-circle" ? "home-health-icon home-health-icon-review" : "home-health-icon"}>
        <Icon name={icon} />
      </span>
      <span className="home-health-words">
        <span className="home-health-title">{row.title}</span>
        {row.explanation === "" ? null : <span className="home-health-explanation">{row.explanation}</span>}
      </span>
      <span className="home-health-location">{row.location}</span>
      <span className="home-health-action">
        {action ? (
          <Button variant="secondary" size="sm" onClick={action.run}>
            {action.label}
          </Button>
        ) : null}
      </span>
    </div>
  );
}

/** The label of a row's action. */
export function actionLabel(action: HealthAction): string {
  return action.kind === "open" ? "Open" : "Settings";
}
