/**
 * Add's preview pane: where the selection is, its first lines of code in the reader's syntax colors,
 * Reached from, Calls, Called by, and the buttons Enter and ⌘Enter press. An entrance shows its
 * handler, where it is declared, and what it reaches; a file its first 12 lines; a text match the
 * lines around it. Asked for 150 ms after the selection settles.
 */

import type { ReactNode } from "react";
import { CodeText } from "../code/CodeText";
import { spanRanges } from "../code/segments";
import { Button, Monogram } from "../components";
import { useEngineQuery, type CodeLine, type CodeSpan, type EntranceHeader, type Preview as PreviewModel } from "../ipc";
import { methodColumn } from "../lib/entrances";
import { plural } from "../lib/format";
import { baseName, type ResultItem } from "./search";

/** Lines a definition's preview shows, as drawn (the pane also holds its relations and buttons). */
const BLOCK_LINES = 5;
/** Lines of a file's top, and around a text match. */
const FILE_LINES = 12;
const AROUND_MATCH = 5;

export type PreviewItem = Exclude<ResultItem, { type: "more" }>;

interface PreviewProps {
  readonly item: PreviewItem | null;
  readonly projectName: (project: string) => string;
  /** On a board page: Add to this page and New page; on Home: Add to a new draft. */
  readonly onPage: boolean;
  readonly blocked: string | null;
  readonly onActivate: (newPage: boolean) => void;
}

interface CodeModel {
  readonly textStart: number;
  readonly text: string;
  readonly lines: readonly CodeLine[];
  readonly spans: readonly CodeSpan[];
}

function CodeBlock({ code, limit, more, current }: { readonly code: CodeModel; readonly limit: number; readonly more: number; readonly current?: number }) {
  const spans = spanRanges(code.spans, code.textStart);
  const lines = code.lines.slice(0, limit);
  return (
    <div className="add-preview-code">
      <div className="add-preview-lines">
        {lines.map((line) => (
          <div key={line.number} className={line.number === current ? "add-preview-line current" : "add-preview-line"}>
            <span className="add-preview-number">{line.number}</span>
            <span className="add-preview-text">
              <CodeText text={code.text} start={line.start - code.textStart} end={line.end - code.textStart} spans={spans} links={[]} />
            </span>
          </div>
        ))}
      </div>
      {more > 0 ? <div className="add-preview-more">{plural(more, "more line")}</div> : null}
    </div>
  );
}

function Relation({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="add-preview-relation">
      <span className="add-preview-label">{label}</span>
      <span className="add-preview-value">{children}</span>
    </div>
  );
}

/** "+2" after the first of several. */
function andMore(total: number, shown: number): string {
  return total > shown ? ` +${total - shown}` : "";
}

function BlockPreview({ preview }: { readonly preview: Extract<PreviewModel, { kind: "block" }> }) {
  const { header, code, reachedFrom, calls, calledBy } = preview;
  const lines = header.endLine - header.startLine + 1;
  const shown = Math.min(BLOCK_LINES, code.lines.length);
  const entrance = reachedFrom.items[0];
  const caller = calledBy.items[0];
  const functions = calls.items.filter((call) => call.key !== null).length;
  const modules = [...new Set(calls.items.flatMap((call) => (call.key === null && call.module !== null ? [call.module] : [])))];
  const callsText = [functions > 0 || modules.length === 0 ? plural(functions, "function") : null, modules.length > 0 ? `${modules.join(", ")} (external)` : null]
    .filter((part) => part !== null)
    .join(" · ");
  return (
    <>
      <CodeBlock code={code} limit={BLOCK_LINES} more={lines - shown} />
      <div className="add-preview-relations">
        {entrance ? (
          <Relation label="Reached from">
            <span className="add-preview-kind">{methodColumn(entrance)}</span>
            <span className="add-code">{(entrance.trigger ?? entrance.title) + andMore(reachedFrom.total, 1)}</span>
          </Relation>
        ) : null}
        {calls.total > 0 ? (
          <Relation label="Calls">
            <span className="add-preview-prose">{callsText + (calls.total > calls.items.length ? " · …" : "")}</span>
          </Relation>
        ) : null}
        {caller ? (
          <Relation label="Called by">
            <span className="add-code">{caller.name + andMore(calledBy.total, 1)}</span>
          </Relation>
        ) : null}
      </div>
    </>
  );
}

function EntrancePreview({ preview }: { readonly preview: Extract<PreviewModel, { kind: "entrance" }> }) {
  const handler = preview.handlers[0];
  const calls = preview.externalModules.length > 0 ? ` · calls ${preview.externalModules.join(", ")}` : "";
  return (
    <div className="add-preview-relations">
      {handler ? (
        <Relation label="Handler">
          <span className="add-code">{handler.name + andMore(preview.handlers.length, 1)}</span>
        </Relation>
      ) : null}
      <Relation label="Declared">
        <span className="add-code">{preview.declared.line === null ? preview.declared.path : `${preview.declared.path}:${preview.declared.line}`}</span>
      </Relation>
      <Relation label="Reaches">
        <span className="add-preview-prose">{plural(preview.reaches, "function") + calls}</span>
      </Relation>
    </div>
  );
}

function Location({ project, children }: { readonly project: string; readonly children: string }) {
  return (
    <div className="add-preview-location">
      <Monogram name={project} size={20} />
      <span>{project}</span>
      <span>·</span>
      <span className="add-code">{children}</span>
    </div>
  );
}

function Status({ children }: { readonly children: ReactNode }) {
  return <p className="add-preview-status">{children}</p>;
}

/** An entrance's trigger as people read it: the label, the trigger (`/orders…` when partial), or "path unknown". */
function triggerText(header: EntranceHeader): string {
  if (header.label) {
    return header.label;
  }
  if (header.trigger !== null && header.triggerStatus !== "unknown") {
    return header.triggerStatus === "partial" ? `${header.trigger}…` : header.trigger;
  }
  return header.kind === "http" ? "path unknown" : "trigger unknown";
}

function KeyedPreview({ item, projectName }: { readonly item: Extract<PreviewItem, { type: "entrance" | "function" }>; readonly projectName: string }) {
  const preview = useEngineQuery("preview", { project: item.hit.project, key: item.hit.key });
  const data = preview.data;
  if (preview.error) {
    return <Status>{preview.error.message}</Status>;
  }
  if (!data) {
    return null;
  }
  if (data.kind === "block") {
    return (
      <>
        <Location project={projectName}>{`${data.header.path}:${data.header.startLine}`}</Location>
        <h3 className="add-preview-title">{data.header.name}</h3>
        <BlockPreview preview={data} />
      </>
    );
  }
  const declared = data.declared.line === null ? data.declared.path : `${data.declared.path}:${data.declared.line}`;
  return (
    <>
      <Location project={projectName}>{declared}</Location>
      <h3 className="add-preview-title">
        <span className="add-preview-kind">{methodColumn(data.header)}</span>
        {triggerText(data.header)}
      </h3>
      <EntrancePreview preview={data} />
    </>
  );
}

function FilePreview({ item, projectName }: { readonly item: Extract<PreviewItem, { type: "file" | "text" }>; readonly projectName: string }) {
  const project = item.type === "file" ? item.hit.project : item.hit.file.project;
  const path = item.type === "file" ? item.hit.path : item.hit.file.path;
  const line = item.type === "text" ? item.hit.match.line : null;
  const range = line === null ? { startLine: 1, endLine: FILE_LINES } : { startLine: Math.max(1, line - AROUND_MATCH), endLine: line + AROUND_MATCH };
  const view = useEngineQuery("file_view", { project, path, range });
  const data = view.data;
  return (
    <>
      <Location project={projectName}>{line === null ? path : `${path}:${line}`}</Location>
      <h3 className="add-preview-title">{baseName(path)}</h3>
      {view.error ? <Status>{view.error.message}</Status> : null}
      {data?.unshowable ? <Status>{data.unshowable === "binary" ? "A binary file." : "Too large to show here."}</Status> : null}
      {data && !data.unshowable ? (
        <CodeBlock
          code={data}
          limit={FILE_LINES}
          more={line === null ? data.lineCount - data.lines.length : 0}
          {...(line === null ? {} : { current: line })}
        />
      ) : null}
    </>
  );
}

/** The preview's buttons: the Enter action, and New page (⌘Enter) on a board page. */
function AddButtons({ label, newPage, disabled, onActivate }: { readonly label: string; readonly newPage: boolean; readonly disabled: boolean; readonly onActivate: (newPage: boolean) => void }) {
  return (
    <div className="add-preview-buttons">
      <Button
        variant="primary"
        className="add-preview-primary"
        disabled={disabled}
        onMouseDown={(event) => {
          event.preventDefault();
        }}
        onClick={() => {
          onActivate(false);
        }}
      >
        {label}
      </Button>
      {newPage ? (
        <Button
          variant="secondary"
          disabled={disabled}
          onMouseDown={(event) => {
            event.preventDefault();
          }}
          onClick={() => {
            onActivate(true);
          }}
        >
          New page
        </Button>
      ) : null}
    </div>
  );
}

export function Preview({ item, projectName, onPage, blocked, onActivate }: PreviewProps) {
  if (item === null) {
    return <aside className="add-preview" aria-label="Preview" />;
  }
  const project = item.type === "text" ? item.hit.file.project : item.hit.project;
  const name = projectName(project);
  const opens = item.type === "file" || item.type === "text";
  return (
    <aside className="add-preview" aria-label="Preview">
      {opens ? <FilePreview item={item} projectName={name} /> : <KeyedPreview item={item} projectName={name} />}
      <div className="add-preview-spacer" />
      {blocked !== null && !opens ? <Status>{blocked}</Status> : null}
      <AddButtons label={opens ? "Open file" : onPage ? "Add to this page" : "Add to a new draft"} newPage={onPage && !opens} disabled={blocked !== null && !opens} onActivate={onActivate} />
    </aside>
  );
}
