/**
 * A call, construction, or import at its written range (docs/reader.md#code-view): hover a resolved one
 * for its signature and location, click to follow (in a card; the peek sheet adds the callee), ⌥-click
 * to expand the callee inline where the caller allows it. External
 * occurrences show their module and unresolved ones a dashed underline whose tooltip gives the reason.
 */

import type { MouseEvent, ReactNode } from "react";
import { HoverCard, Tooltip } from "../components";
import type { CallLink as CallLinkModel, Peek, ProjectId } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { linkNote, shortPath } from "./text";

/** The hover's content: the target's first written line (its signature) and location. */
function PeekContent({ project, targetKey, hint }: { readonly project: ProjectId; readonly targetKey: string; readonly hint: string }) {
  const peek = useEngineQuery("peek", { project, key: targetKey });
  return <PeekBody peek={peek.data} failed={peek.isError} hint={hint} />;
}

function PeekBody({ peek, failed, hint }: { readonly peek: Peek | undefined; readonly failed: boolean; readonly hint: string }) {
  if (failed) {
    return <div className="call-peek">This target isn't in the graph.</div>;
  }
  if (!peek) {
    return <div className="call-peek call-peek-loading">…</div>;
  }
  return (
    <div className="call-peek">
      <div className="call-peek-title">
        <strong>{peek.qualifiedName ?? peek.name}</strong>
        <span>
          {shortPath(peek.path)}
          {peek.line !== null ? `:${peek.line}` : ""}
        </span>
      </div>
      {peek.signature !== null ? <code className="call-peek-signature">{peek.signature}</code> : null}
      <div className="call-peek-hint">{hint}</div>
    </div>
  );
}

export interface CallLinkProps {
  /** The project the code is in, which the hover asks about the target. */
  readonly project: ProjectId;
  readonly link: CallLinkModel;
  /** Whether an arrow already leaves for this occurrence (its target is on the board). */
  readonly opened?: boolean;
  readonly onFollow?: (link: CallLinkModel, anchor: HTMLElement) => void;
  readonly onExpand?: (link: CallLinkModel) => void;
  /** What the hover says a click does (the peek sheet adds the callee); defaults to following it. */
  readonly hint?: string;
  readonly children: ReactNode;
}

export function CallLink({ project, link, opened = false, onFollow, onExpand, hint, children }: CallLinkProps) {
  const target = link.resolution === "resolved" ? link.target : undefined;
  if (target !== undefined && onFollow) {
    const onClick = (event: MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      if (event.altKey && onExpand) {
        onExpand(link);
      } else {
        onFollow(link, event.currentTarget);
      }
    };
    return (
      <HoverCard content={<PeekContent project={project} targetKey={target} hint={hint ?? (onExpand ? "Click to open · ⌥-click to expand inline" : "Click to open")} />} side="bottom" align="start">
        <button
          type="button"
          className={`code-link resolved${opened ? " opened" : ""} nodrag nopan`}
          data-occurrence={link.source}
          data-kind={link.kind}
          aria-label={`Follow ${link.text}`}
          onClick={onClick}
        >
          {children}
        </button>
      </HoverCard>
    );
  }
  const note = linkNote(link);
  if (note === null) {
    return (
      <span className="code-link plain" data-occurrence={link.source}>
        {children}
      </span>
    );
  }
  return (
    <Tooltip content={note} side="bottom" align="start">
      <span className={`code-link ${link.resolution}`} data-occurrence={link.source} tabIndex={-1}>
        {children}
      </span>
    </Tooltip>
  );
}
