/** Pieces every card shares: ports, the fold toggle, the kind icon, and the quiet description line. */

import { Handle, Position } from "@xyflow/react";
import { Box, Braces, FileCode, SquareFunction } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { BlockKind, Description } from "../../ipc/generated";

/** The handle every arrow into a card attaches to: its header's left edge. */
export const TARGET_HANDLE = "in";

/** Source ports, one per opened occurrence, drawn where the caller places them. */
export function renderPorts(ids: readonly string[]): ReactNode {
  return ids.map((id) => <Handle key={id} id={id} type="source" position={Position.Right} isConnectable={false} className="card-port" />);
}

/** Ports drawn at the header: every port of a folded card, and any the body can't place. */
export function HeaderPorts({ ids }: { readonly ids: readonly string[] }) {
  return ids.length > 0 ? <span className="card-header-ports">{renderPorts(ids)}</span> : null;
}

export function TargetHandle() {
  return <Handle id={TARGET_HANDLE} type="target" position={Position.Left} isConnectable={false} className="card-target" />;
}

export function FoldToggle({ folded, label, onToggle }: { readonly folded: boolean; readonly label: string; readonly onToggle: () => void }) {
  return (
    <button
      type="button"
      className={`card-fold nodrag${folded ? " folded" : ""}`}
      aria-label={`${folded ? "Unfold" : "Fold"} ${label}`}
      aria-expanded={!folded}
      title={folded ? "Unfold card" : "Fold to its title"}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      onDoubleClick={(event) => {
        event.stopPropagation();
      }}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <path d={folded ? "M3.5 2 L6.5 5 L3.5 8" : "M2 3.5 L5 6.5 L8 3.5"} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
}

const FUNCTION_KINDS: ReadonlySet<BlockKind> = new Set(["function", "method", "constructor", "getter", "setter"]);

export function KindIcon({ kind }: { readonly kind: BlockKind | null }) {
  const Icon = kind === null ? Braces : FUNCTION_KINDS.has(kind) ? SquareFunction : kind === "class" ? Box : kind === "file" ? FileCode : Braces;
  return (
    <span className="card-kind" title={kind ?? undefined}>
      <Icon size={14} aria-hidden="true" />
    </span>
  );
}

/** One quiet line of authored text with a **more** link (architecture §10.7). */
export function DescriptionLine({ description }: { readonly description: Description | undefined }) {
  const [open, setOpen] = useState(false);
  if (!description) {
    return null;
  }
  return (
    <div className={`card-description nodrag${open ? " open" : ""}`}>
      <p title={open ? undefined : description.text}>{description.text}</p>
      <button
        type="button"
        className="card-more"
        onClick={(event) => {
          event.stopPropagation();
          setOpen((value) => !value);
        }}
      >
        {open ? "less" : "more"}
      </button>
    </div>
  );
}
