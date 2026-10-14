/**
 * Pieces every card shares: the ports arrows and connections attach to (invisible: no port dots are
 * drawn), the header's mark, the chevron that makes a card compact or expanded, the description strip
 * (edited in place on double-click), and the note line a card body shows while it loads or when what
 * it draws is unavailable.
 */

import { Handle, Position } from "@xyflow/react";
import { useState, type ReactNode } from "react";
import { AgentMark } from "../../agent";
import { authorOf } from "../../code/text";
import { Icon } from "../../icons";
import type { Description } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { describeCard, editPage, setCardView, type PageTarget } from "../../state/boards";
import type { HeaderMark } from "../cardText";
import { LINK_SIDES } from "../linkSides";

/** The handle every arrow into a card attaches to: its header's left edge. */
export const TARGET_HANDLE = "in";

/** Source ports at the right edge of what holds them, each at its `tops` offset below its top when given. */
export function renderPorts(ids: readonly string[], tops?: ReadonlyMap<string, number>): ReactNode {
  return ids.map((id) => {
    const top = tops?.get(id);
    return <Handle key={id} id={id} type="source" position={Position.Right} isConnectable={false} className="card-port" {...(top !== undefined ? { style: { top } } : {})} />;
  });
}

/** Ports drawn at the header: any the body doesn't place. */
export function HeaderPorts({ ids }: { readonly ids: readonly string[] }) {
  return ids.length > 0 ? <span className="card-header-ports">{renderPorts(ids)}</span> : null;
}

/**
 * The chevron at the start of a card's header: it makes the card expanded (its drawn flow, an app's or
 * entrance's code) or compact again, as a board edit (saved with the board, one undo step). A click on
 * it never selects the card.
 */
export function ExpandToggle({ target, cardId, expanded, name }: { readonly target: PageTarget; readonly cardId: string; readonly expanded: boolean; readonly name: string }) {
  return (
    <button
      type="button"
      className="card-expand nodrag"
      aria-label={`${expanded ? "Collapse" : "Expand"} ${name}`}
      aria-expanded={expanded}
      onClick={(event) => {
        event.stopPropagation();
        editPage(target, (page) => {
          setCardView(page, cardId, expanded ? "compact" : "expanded");
        });
      }}
    >
      <Icon name={expanded ? "chevron-down-small" : "chevron-right-small"} size={10} />
    </button>
  );
}

const SIDE_POSITIONS = { left: Position.Left, right: Position.Right, top: Position.Top, bottom: Position.Bottom } as const;

/** The arrow target at the header, and a source and a target handle on each side for connections. */
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

/** What a header shows before the name: a glyph (`ƒ`, `λ`, `M`; accent on the selected card), or a kind's word. */
export function CardMark({ mark }: { readonly mark: HeaderMark }) {
  return (
    <span className={mark.word ? "tcard-kind" : "card-glyph"} aria-hidden={mark.word ? undefined : "true"}>
      {mark.text}
    </span>
  );
}

/** A quiet line in a card body: loading, or (with `warn`) why it isn't drawn. */
export function CardNote({ warn = false, children }: { readonly warn?: boolean; readonly children: ReactNode }) {
  return (
    <p className={`card-note${warn ? " warn" : ""}`}>
      {warn ? <Icon name="warning-triangle" size={14} /> : null}
      {children}
    </p>
  );
}

/**
 * The card's description: one line with **more**, in the authored color, and the agent mark when an
 * agent wrote it; a compact card's or a resource card's (`wrap`) shows all of it, wrapped. Double-click
 * edits it in place (Enter saves, Shift+Enter breaks the line, Escape keeps the old text), which makes
 * it yours; clearing it removes it. `onSave` writes it for an item that isn't a card.
 */
export function DescriptionStrip({
  target,
  cardId,
  description,
  wrap = false,
  onSave,
}: {
  readonly target: PageTarget;
  readonly cardId: string;
  readonly description: Description | undefined;
  readonly wrap?: boolean;
  readonly onSave?: (text: string, author: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const author = useAuthor();
  if (!description) {
    return null;
  }
  const writer = authorOf(description.author);
  if (editing && author !== null) {
    const finish = (text: string | null) => {
      setEditing(false);
      if (text !== null && text.trim() !== description.text) {
        if (onSave) {
          onSave(text.trim(), author);
        } else {
          describeCard(target, cardId, text.trim(), author);
        }
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
  const shown = open || wrap;
  return (
    <div
      className={`card-description nodrag${shown ? " open" : ""}`}
      onDoubleClick={(event) => {
        event.stopPropagation();
        setEditing(true);
      }}
    >
      <p title={shown ? undefined : description.text}>{description.text}</p>
      {writer.agent ? <AgentMark name={writer.name} /> : null}
      {wrap ? null : (
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
      )}
    </div>
  );
}
