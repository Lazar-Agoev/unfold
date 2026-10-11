/**
 * The entrance browser's preview of the selected row. A launch shows its command as `folder $ command`
 * (wrapping here), what it starts and whether that was read from the command or set by hand, its
 * import path with where each folder came from, what it reaches, and where it is declared with Edit;
 * an unreadable launch says why. Any other entrance shows its handlers, declaration (with Edit when
 * it is configured), and reach from `preview`. An application (All projects, a project) shows its launches and opens with Enter.
 */

import type { ReactNode } from "react";
import { TextButton } from "../components";
import { useEngineQuery, type AppRow, type CatalogRow, type LaunchRow, type ProjectId } from "../ipc";
import { methodColumn } from "../lib/entrances";
import { plural } from "../lib/format";
import type { DialogRequest } from "../state/overlays";
import { UNREACHED_NAME, type ListItem } from "./browse";
import { AddButtons } from "./Preview";

interface BrowsePreviewProps {
  readonly item: ListItem | null;
  /** The application the list shows (its name heads the preview). */
  readonly app: AppRow | null;
  readonly projectName: (project: ProjectId) => string;
  /** On a board page: Add to this page and New page; on Home: Add to a new draft. */
  readonly onPage: boolean;
  readonly blocked: string | null;
  readonly onActivate: (newPage: boolean) => void;
  /** Opens the launch editor on the declaration. */
  readonly onEdit: (request: DialogRequest) => void;
}

const KIND_WORDS: Readonly<Record<CatalogRow["kind"], string>> = {
  process: "Launch",
  http: "Route",
  worker: "Worker",
  schedule: "Schedule",
  command: "Command",
  event: "Event",
};

function Fact({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="add-fact">
      <span className="add-fact-label">{label}</span>
      <div className="add-fact-value">{children}</div>
    </div>
  );
}

function Value({ code, children, sub }: { readonly code?: boolean; readonly children: ReactNode; readonly sub?: ReactNode }) {
  return (
    <>
      <span className={code ? "add-fact-main add-code" : "add-fact-main"}>{children}</span>
      {sub === undefined || sub === null ? null : <span className="add-fact-sub">{sub}</span>}
    </>
  );
}

function Intro({ eyebrow, title, description }: { readonly eyebrow: string; readonly title: ReactNode; readonly description: string | null }) {
  return (
    <div className="add-browse-intro">
      <span className="add-browse-eyebrow">{eyebrow}</span>
      <h3 className="add-browse-name">{title}</h3>
      {description === null ? null : <p className="add-browse-about">{description}</p>}
    </div>
  );
}

function CommandBox({ folder, command }: { readonly folder: string; readonly command: string }) {
  return (
    <div className="add-command-box add-code">
      <span className="add-command-folder">{`${folder} $`}</span>
      <span className="add-command-line">{command}</span>
    </div>
  );
}

function declaredText(path: string, line: number | null): string {
  return line === null ? path : `${path}:${line}`;
}

/** Where it is declared, with Edit when the editor can change it. */
function DeclaredIn({ text, edit, onEdit }: { readonly text: string; readonly edit: DialogRequest | null; readonly onEdit: BrowsePreviewProps["onEdit"] }) {
  return (
    <Fact label="Declared in">
      <span className="add-fact-inline">
        <span className="add-fact-main add-code">{text}</span>
        {edit === null ? null : (
          <TextButton
            onMouseDown={(event) => {
              event.preventDefault();
            }}
            onClick={() => {
              onEdit(edit);
            }}
          >
            Edit
          </TextButton>
        )}
      </span>
    </Fact>
  );
}

/** A launch's editor: its `[[entrance]]`, or the override of a detected launch. */
function launchEdit(project: ProjectId, launch: LaunchRow): DialogRequest {
  return launch.editable === "configured" && launch.id !== null
    ? { kind: "launchEditor", project, mode: "edit", id: launch.id }
    : { kind: "launchEditor", project, mode: "override", key: launch.key };
}

function eyebrow(word: string, app: AppRow | null): string {
  return app === null ? word : `${word} · ${app.name}`;
}

function LaunchPreview({ project, row, launch, app, onEdit }: { readonly project: ProjectId; readonly row: CatalogRow; readonly launch: LaunchRow; readonly app: AppRow | null; readonly onEdit: BrowsePreviewProps["onEdit"] }) {
  const { starts } = launch;
  const reaches = [launch.reaches === null ? null : plural(launch.reaches, "function"), launch.routes === null ? null : plural(launch.routes, "route")].filter((part) => part !== null);
  return (
    <>
      <Intro eyebrow={eyebrow("Launch", app)} title={launch.label ?? launch.command ?? row.title} description={launch.description} />
      {launch.problem === null && launch.note === null ? null : (
        <div className="add-launch-status">
          {launch.problem === null ? null : <span className="add-launch-problem">{launch.problem}</span>}
          {launch.note === null ? null : <span className="add-fact-sub">{launch.note}</span>}
        </div>
      )}
      {launch.command === null ? null : <CommandBox folder={launch.folder} command={launch.command} />}
      <div className="add-facts">
        {launch.schedule === null ? null : (
          <Fact label="Schedule">
            <Value code sub={launch.scheduleGloss}>
              {launch.schedule}
            </Value>
          </Fact>
        )}
        {starts === null ? null : (
          <Fact label="Starts">
            <Value code sub={`${starts.kind} · ${launch.targetSetByHand ? "set by hand" : "read from the command"}`}>
              {starts.name === null ? starts.path : `${starts.path} → ${starts.name}`}
            </Value>
          </Fact>
        )}
        {launch.importPaths.length === 0 ? null : (
          <Fact label="Imports from">
            {launch.importPaths.map((entry) => (
              <Value key={entry.path} code sub={entry.origin}>
                {entry.path}
              </Value>
            ))}
          </Fact>
        )}
        {reaches.length === 0 ? null : (
          <Fact label="Reaches">
            <Value>{reaches.join(" · ")}</Value>
          </Fact>
        )}
        <DeclaredIn text={declaredText(launch.declaredIn.path, launch.declaredIn.line)} edit={launchEdit(project, launch)} onEdit={onEdit} />
      </div>
    </>
  );
}

function EntrancePreview({ project, row, app, onEdit }: { readonly project: ProjectId; readonly row: CatalogRow; readonly app: AppRow | null; readonly onEdit: BrowsePreviewProps["onEdit"] }) {
  const preview = useEngineQuery("preview", { project, key: row.key });
  const data = preview.data?.kind === "entrance" ? preview.data : undefined;
  const handlers = data ? data.handlers.map((handler) => handler.name) : row.handler === null ? [] : [row.handler];
  const declared = data ? declaredText(data.declared.path, data.declared.line) : declaredText(row.path, row.line);
  const reaches = data ? data.reaches : row.reaches;
  const calls = data && data.externalModules.length > 0 ? ` · calls ${data.externalModules.join(", ")}` : "";
  return (
    <>
      <Intro
        eyebrow={eyebrow(KIND_WORDS[row.kind], app)}
        title={
          <>
            <span className="add-preview-kind">{methodColumn(row)}</span>
            <span className="add-code">{row.trigger ?? row.title}</span>
          </>
        }
        description={null}
      />
      {preview.error ? <p className="add-preview-status">{preview.error.message}</p> : null}
      <div className="add-facts">
        <Fact label={handlers.length > 1 ? "Handlers" : "Handler"}>
          <Value code>{handlers.length > 0 ? handlers.join(", ") : "none resolved"}</Value>
        </Fact>
        <DeclaredIn text={declared} edit={row.id === null ? null : { kind: "launchEditor", project, mode: "edit", id: row.id }} onEdit={onEdit} />
        {reaches === null ? null : (
          <Fact label="Reaches">
            <Value>{plural(reaches, "function") + calls}</Value>
          </Fact>
        )}
        {row.alsoIn.length === 0 ? null : (
          <Fact label="Also in">
            <Value>{row.alsoIn.join(", ")}</Value>
          </Fact>
        )}
      </div>
    </>
  );
}

function ApplicationPreview({ project, app }: { readonly project: string; readonly app: AppRow }) {
  return (
    <>
      <Intro eyebrow={project} title={app.name} description={app.description} />
      {app.launches.map((launch) =>
        launch.command === null ? null : <CommandBox key={launch.key} folder={launch.folder} command={launch.command} />,
      )}
      <div className="add-facts">
        <Fact label="Launches">
          <Value>{app.launches.length === 0 ? "No launch defined" : plural(app.launches.length, "launch", "launches")}</Value>
        </Fact>
        <Fact label="Entrances">
          <Value>{plural(app.entrances, "entrance")}</Value>
        </Fact>
        {app.routes === 0 ? null : (
          <Fact label="Routes">
            <Value>{plural(app.routes, "route")}</Value>
          </Fact>
        )}
      </div>
    </>
  );
}

export function BrowsePreview({ item, app, projectName, onPage, blocked, onActivate, onEdit }: BrowsePreviewProps) {
  if (item === null || item.type === "more") {
    return <aside className="add-preview add-browse-preview" aria-label="Preview" />;
  }
  if (item.type !== "entrance") {
    const project = item.scope.kind === "all" ? "" : projectName(item.scope.project);
    return (
      <aside className="add-preview add-browse-preview" aria-label="Preview">
        {item.type === "application" ? (
          <ApplicationPreview project={project} app={item.app} />
        ) : (
          <Intro eyebrow={project} title={UNREACHED_NAME} description={plural(item.count, "entrance")} />
        )}
        <div className="add-preview-spacer" />
        <AddButtons label="Show entrances" newPage={false} disabled={false} onActivate={onActivate} />
      </aside>
    );
  }
  return (
    <aside className="add-preview add-browse-preview" aria-label="Preview">
      {item.launch ? (
        <LaunchPreview project={item.project} row={item.row} launch={item.launch} app={app} onEdit={onEdit} />
      ) : (
        <EntrancePreview project={item.project} row={item.row} app={app} onEdit={onEdit} />
      )}
      <div className="add-preview-spacer" />
      {blocked === null ? null : <p className="add-preview-status">{blocked}</p>}
      <AddButtons label={onPage ? "Add to this page" : "Add to a new draft"} newPage={onPage} disabled={blocked !== null} onActivate={onActivate} />
    </aside>
  );
}
