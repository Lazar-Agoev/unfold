/**
 * The launch editor's right column, "What Unfold reads": the command's parts with their meanings,
 * why it can't be read (and what the quoted part of `sh -c` filled in), what the launch starts, its
 * import path in order, and what saving changes. Every word about the command comes from Rust
 * (`check_launch`, or the catalog row of a detected launch).
 */

import type { ReactNode } from "react";
import { TextButton } from "../components";
import type { ImportGain, ImportPathRow, LaunchCheck, LaunchStatus, RoutePreview } from "../ipc";
import { Icon } from "../icons";

function Caps({ children }: { readonly children: ReactNode }) {
  return <h3 className="launch-caps">{children}</h3>;
}

/** The line under the column's heading: where the preview comes from. */
function StatusLine({ check }: { readonly check: LaunchCheck }) {
  const [tone, text] = !check.fromCurrentGraph
    ? (["muted", "Compile the project for a full preview · compiles on save"] as const)
    : check.targetSetByHand
      ? (["review", "Target set by hand · compiles on save"] as const)
      : (["ok", "Preview from the current graph · compiles on save"] as const);
  return (
    <>
      <span className={`launch-dot launch-dot-${tone}`} aria-hidden="true" />
      <span className="launch-status">{text}</span>
    </>
  );
}

export interface Fill {
  /** The quoted command the target and paths came from. */
  readonly quoted: string;
  /** Whether they're filled in now (Undo empties them again). */
  readonly filled: boolean;
}

function ProblemCard({ check, fill, onFill, onUndo }: { readonly check: LaunchCheck; readonly fill: Fill | null; readonly onFill: () => void; readonly onUndo: () => void }) {
  if (!check.problem) {
    return null;
  }
  return (
    <div className="launch-problem" role="status">
      <div className="launch-problem-title">
        <span className="launch-dot launch-dot-review" aria-hidden="true" />
        {check.problem.label}
      </div>
      <p className="launch-problem-message">{check.problem.message}</p>
      {check.suggestion ? (
        <div className="launch-problem-fill">
          <span>{fill?.filled ? "Filled from the quoted part:" : "The quoted part:"}</span>
          <code className="launch-chip launch-chip-fill">{check.suggestion.quoted}</code>
          {fill?.filled ? <TextButton onClick={onUndo}>Undo</TextButton> : <TextButton onClick={onFill}>Fill from it</TextButton>}
        </div>
      ) : null}
    </div>
  );
}

interface StartsProps {
  readonly path: string;
  readonly name: string | null;
  readonly description: string;
  /** Resolved and read from the command: ✓; otherwise !. */
  readonly sure: boolean;
}

function Starts({ path, name, description, sure }: StartsProps) {
  return (
    <section className="launch-section launch-section-ruled">
      <Caps>Starts</Caps>
      <div className="launch-box launch-starts">
        <Icon name={sure ? "check-circle" : "warning-circle"} className={sure ? "launch-starts-ok" : "launch-starts-unsure"} />
        <div className="launch-starts-text">
          <div className="launch-starts-target launch-mono">
            <span>{path}</span>
            {name === null ? null : (
              <>
                <span className="launch-starts-arrow" aria-hidden="true">
                  →
                </span>
                <strong>{name}</strong>
              </>
            )}
          </div>
          <div className="launch-starts-description">{description}</div>
        </div>
      </div>
    </section>
  );
}

function ImportPaths({ rows, setByHand }: { readonly rows: readonly ImportPathRow[]; readonly setByHand: boolean }) {
  return (
    <section className="launch-section">
      <Caps>Imports resolve from, in order</Caps>
      <ol className="launch-box launch-resolve">
        {rows.map((row, position) => (
          <li key={`${position}:${row.path}`} className="launch-resolve-row">
            <span className="launch-path-number">{position + 1}</span>
            <span className="launch-resolve-path launch-mono">{row.path}</span>
            <span className="launch-resolve-origin">{row.origin}</span>
          </li>
        ))}
      </ol>
      <p className="launch-note">
        {setByHand
          ? "Set by hand, so Unfold uses this list as written and doesn't add folders from the command."
          : "These apply to every file this launch imports, including shared code. Other launches keep their own."}
      </p>
    </section>
  );
}

/** "a, b, and c". */
function series(words: readonly string[]): string {
  if (words.length <= 2) {
    return words.join(" and ");
  }
  return `${words.slice(0, -1).join(", ")}, and ${words.at(-1) ?? ""}`;
}

function ImportsGained({ imports }: { readonly imports: ImportGain }) {
  const one = imports.count === 1;
  return (
    <>
      <div className="launch-outcome-title">
        {one ? "1 more import resolves" : `${imports.count.toLocaleString("en-US")} more imports resolve`} to code in this repository
      </div>
      {imports.examples.length > 0 ? (
        <div className="launch-outcome-body">
          {series(imports.examples)} now {imports.examples.length === 1 ? "opens" : "open"} as code instead of reading as{" "}
          {imports.examples.length === 1 ? "a package" : "packages"}.
        </div>
      ) : null}
    </>
  );
}

function RoutesJoin({ routes, application }: { readonly routes: RoutePreview; readonly application: string | null }) {
  const count = routes.count === 1 ? "1 route joins" : `${routes.count.toLocaleString("en-US")} routes join`;
  return (
    <>
      <div className="launch-outcome-title">{application === null ? `${count} this launch` : `${count} ${application}`}</div>
      {routes.shared.map((shared) => (
        <div key={shared.path} className="launch-outcome-body">
          {shared.routes === routes.count ? (routes.count === 1 ? "It comes" : "They come") : `${shared.routes} of them come`} from {shared.path}, which{" "}
          {series(shared.alsoIn)} also {shared.alsoIn.length === 1 ? "mounts" : "mount"}. Each application lists them under its own mount.
        </div>
      ))}
    </>
  );
}

interface PreviewProps {
  readonly check: LaunchCheck | undefined;
  /** The application the launch will belong to, by name ("5 routes join Dashboard builder"). */
  readonly application: string | null;
  /** Whether the draft's import paths are set by hand. */
  readonly pathsSetByHand: boolean;
  readonly fill: Fill | null;
  readonly onFill: () => void;
  readonly onUndoFill: () => void;
}

/** The preview of a launch being added or edited, from `check_launch`. */
export function LaunchPreview({ check, application, pathsSetByHand, fill, onFill, onUndoFill }: PreviewProps) {
  const empty = check?.parts.length === 0 && check.problem === null && check.starts === null;
  return (
    <aside className="launch-preview" aria-label="What Unfold reads" aria-busy={check === undefined || undefined}>
      <div className="launch-preview-head">
        <Caps>What Unfold reads</Caps>
        {check ? <StatusLine check={check} /> : null}
      </div>
      {empty ? <p className="launch-note">Paste the command to see what Unfold reads from it.</p> : null}
      {check && check.parts.length > 0 ? (
        <dl className="launch-parts">
          {check.parts.map((part, position) => (
            <div key={`${position}:${part.text}`} className={part.irrelevant ? "launch-part launch-part-irrelevant" : "launch-part"}>
              <dt>
                <code className="launch-chip">{part.text}</code>
              </dt>
              <dd>{part.meaning}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {check ? <ProblemCard check={check} fill={fill} onFill={onFill} onUndo={onUndoFill} /> : null}
      {check?.starts ? (
        <Starts
          path={check.starts.path}
          name={check.starts.name}
          description={check.starts.reason === null ? check.starts.description : `${check.starts.description} · ${check.starts.reason}`}
          sure={check.starts.status === "ok" && !check.targetSetByHand}
        />
      ) : null}
      {check && check.importPaths.length > 0 ? <ImportPaths rows={check.importPaths} setByHand={pathsSetByHand} /> : null}
      {(check?.imports ?? check?.routes) ? (
        <div className="launch-outcome">
          {check.imports ? <ImportsGained imports={check.imports} /> : null}
          {check.routes ? <RoutesJoin routes={check.routes} application={application ?? check.routes.application} /> : null}
        </div>
      ) : null}
    </aside>
  );
}

interface DetectedPreviewProps {
  readonly starts: { readonly path: string; readonly name: string | null; readonly kind: string } | null;
  readonly status: LaunchStatus;
  readonly importPaths: readonly ImportPathRow[];
  /** The file the launch is detected in. */
  readonly declaredIn: string;
}

/** What Unfold read from a detected launch (its catalog row); only its descriptive fields change. */
export function DetectedPreview({ starts, status, importPaths, declaredIn }: DetectedPreviewProps) {
  return (
    <aside className="launch-preview" aria-label="What Unfold reads">
      <div className="launch-preview-head">
        <Caps>What Unfold reads</Caps>
        <span className={`launch-dot launch-dot-${status === "ok" ? "ok" : "review"}`} aria-hidden="true" />
        <span className="launch-status">Detected in {declaredIn}</span>
      </div>
      {starts ? <Starts path={starts.path} name={starts.name} description={starts.kind} sure={status === "ok"} /> : null}
      {importPaths.length > 0 ? <ImportPaths rows={importPaths} setByHand={false} /> : null}
    </aside>
  );
}
