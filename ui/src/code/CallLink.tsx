/**
 * A call, construction, or import at its written range (architecture §10.3): hover a resolved one
 * for its signature and location, click to follow, ⌥-click to expand the callee inline. External
 * occurrences show their module and unresolved ones a dashed underline whose tooltip gives the reason.
 */

import type { MouseEvent, ReactNode } from "react";
import { useScopeProject } from "../canvas/ProjectScope";
import { HoverCard, Tooltip } from "../components";
import type { CallLink as CallLinkModel, Peek } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { linkNote, shortPath } from "./text";

/** The hover's content: the target's first written line (its signature) and location, in the enclosing card's project. */
export function PeekContent({ targetKey, hint = "Click to open · ⌥-click to expand inline" }: { readonly targetKey: string; readonly hint?: string }) {
  const project = useScopeProject();
  const peek = useEngineQuery("peek", { project: project ?? "", key: targetKey }, { enabled: project !== null });
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
  readonly link: CallLinkModel;
  /** Whether an arrow already leaves for this occurrence (its target is on the board). */
  readonly opened?: boolean;
  readonly onFollow?: (link: CallLinkModel, anchor: HTMLElement) => void;
  readonly onExpand?: (link: CallLinkModel) => void;
  readonly children: ReactNode;
}

export function CallLink({ link, opened = false, onFollow, onExpand, children }: CallLinkProps) {
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
      <HoverCard content={<PeekContent targetKey={target} />} side="bottom" align="start">
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
