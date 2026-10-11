/**
 * Pieces every card shares: the ports arrows and links attach to (invisible: no port dots are drawn),
 * the kind glyph, the card's own Diagram | Flow | Code switch, the description strip (edited in place on
 * double-click), and the note line a card body shows while it loads or when its source is unavailable.
 */

import { Handle, Position } from "@xyflow/react";
import { useState, type ReactNode } from "react";
import { AgentMark } from "../../agent";
import { SegmentedControl } from "../../components";
import { authorOf } from "../../code/text";
import { Icon } from "../../icons";
import type { BlockKind, Description, View } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { describeCard, editPage, setCardView, setCardsFolded, updatePageView, type PageTarget } from "../../state/boards";
import { NODE_PORT } from "../arrows";
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

/** The ports a Diagram node or a type card draws at its header: its node port, and every port while folded. */
export function nodeHeaderPorts(ports: readonly string[], folded: boolean): string[] {
  return ports.filter((id) => folded || id === NODE_PORT);
}

/** The three views, in the lens's order. */
export const VIEW_OPTIONS = [
  { value: "diagram", label: "Diagram" },
  { value: "flow", label: "Flow" },
  { value: "code", label: "Code" },
] as const;

/**
 * A card's own Diagram | Flow | Code switch (saved with the board). Using it while the page's lens is
 * on makes the card an exception, so it keeps its own view.
 */
export function ViewSwitch({ target, cardId, view, name }: { readonly target: PageTarget; readonly cardId: string; readonly view: View; readonly name: string }) {
  const setView = (next: View) => {
    editPage(target, (page) => {
      setCardView(page, cardId, next);
    });
    updatePageView(target, (draft) => {
      if (draft.lens !== null && !draft.lensExceptions.includes(cardId)) {
        draft.lensExceptions.push(cardId);
      }
    });
  };
  return (
    <div
      className="nodrag"
      onClick={(event) => {
        event.stopPropagation();
      }}
    >
      <SegmentedControl options={VIEW_OPTIONS} value={view} onChange={setView} label={`View of ${name}`} variant="mini" />
    </div>
  );
}

/** Folds a card to its title, or unfolds it (a double-click on its header). */
export function toggleCardFolded(target: PageTarget, cardId: string, folded: boolean): void {
  editPage(target, (page) => {
    setCardsFolded(page, [cardId], !folded);
  });
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

/** `ƒ` for a function (accent on the selected card), `{}` for any other block. */
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
 * The card's description: one line with **more**, in the authored color, and the agent mark when an
 * agent wrote it; a Diagram node's (`wrap`) shows all of it, wrapped. Double-click edits it in place
 * (Enter saves, Shift+Enter breaks the line, Escape keeps the old text), which makes it yours; clearing
 * it removes it.
 */
export function DescriptionStrip({
  target,
  cardId,
  description,
  wrap = false,
}: {
  readonly target: PageTarget;
  readonly cardId: string;
  readonly description: Description | undefined;
  readonly wrap?: boolean;
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
