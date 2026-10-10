/**
 * Pieces every card shares (02 §4, 10 §5): the ports arrows and links attach to (invisible: no port dots
 * are drawn), the kind glyph, the description strip (edited in place on double-click), and the note
 * line a card body shows while it loads or when its source is unavailable.
 */

import { Handle, Position } from "@xyflow/react";
import { useState, type ReactNode } from "react";
import { Icon } from "../../icons";
import type { BlockKind, Description } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { describeCard, type PageTarget } from "../../state/boards";
import { LINK_SIDES } from "../linkSides";

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

const SIDE_POSITIONS = { left: Position.Left, right: Position.Right, top: Position.Top, bottom: Position.Bottom } as const;

/** The arrow target at the header, and a source and a target handle on each side for drawn links. */
export function CardHandles() {
  return (
    <>
      <Handle id={TARGET_HANDLE} type="target" position={Position.Left} isConnectable={false} className="card-target" />
      {LINK_SIDES.map((side) => (
        <span key={side} className={`card-link-handles ${side}`}>
          <Handle id={`link-out-${side}`} type="source" position={SIDE_POSITIONS[side]} isConnectable={false} className="card-link-handle" />
          <Handle id={`link-in-${side}`} type="target" position={SIDE_POSITIONS[side]} isConnectable={false} className="card-link-handle" />
        </span>
      ))}
    </>
  );
}

const FUNCTION_KINDS: ReadonlySet<BlockKind> = new Set(["function", "method", "constructor", "getter", "setter"]);

/** `ƒ` for a function (accent on the selected card), `{}` for any other block (02 §4). */
export function CardGlyph({ kind }: { readonly kind: BlockKind | null }) {
  return (
    <span className="card-glyph" title={kind ?? undefined} aria-hidden="true">
      {kind === null || FUNCTION_KINDS.has(kind) ? "ƒ" : "{}"}
    </span>
  );
}

/** A quiet line in a card body: loading, or (with `warn`) why the code isn't shown. */
export function CardNote({ warn = false, children }: { readonly warn?: boolean; readonly children: ReactNode }) {
  return (
    <p className={`card-note${warn ? " warn" : ""}`}>
      {warn ? <Icon name="warning-triangle" size={14} /> : null}
      {children}
    </p>
  );
}

/**
 * The card's description (02 §4.2): one line with **more**, in the authored color. Double-click edits
 * it in place (Enter saves, Shift+Enter breaks the line, Escape keeps the old text); clearing it removes it.
 */
export function DescriptionStrip({ target, cardId, description }: { readonly target: PageTarget; readonly cardId: string; readonly description: Description | undefined }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const author = useAuthor();
  if (!description) {
    return null;
  }
  if (editing && author !== null) {
    const finish = (text: string | null) => {
      setEditing(false);
      if (text !== null && text.trim() !== description.text) {
        describeCard(target, cardId, text.trim(), author);
      }
    };
    return (
      <div className="card-description editing nodrag nopan">
        <textarea
          autoFocus
          className="card-description-input"
          aria-label="Description"
          defaultValue={description.text}
          rows={Math.min(6, Math.max(2, description.text.split("\n").length))}
          onClick={(event) => {
            event.stopPropagation();
          }}
          onKeyDown={(event) => {
            event.stopPropagation();
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              finish(event.currentTarget.value);
            } else if (event.key === "Escape") {
              event.preventDefault();
              finish(null);
            }
          }}
          onBlur={(event) => {
            finish(event.currentTarget.value);
          }}
        />
      </div>
    );
  }
  return (
    <div
      className={`card-description nodrag${open ? " open" : ""}`}
      onDoubleClick={(event) => {
        event.stopPropagation();
        setEditing(true);
      }}
    >
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
