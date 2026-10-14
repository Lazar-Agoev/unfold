/**
 * A frame on the canvas, drawn in the authored style. Expanded, it is a box behind what
 * it holds, sized by `pageGeometry`: a **box** is outlined with a title bar, a **zone** is a tinted
 * area with a large title and no border. Only its title takes the pointer, so the canvas pans and
 * cards are clicked through it. Collapsed, it is one node like a card: its icon, title, description,
 * and what it holds ("3 functions · 1 table"), with the arrows and links of what it holds attached.
 *
 * Its chevron collapses and expands it (nothing collapses by itself); its title selects it, drags it
 * with everything in it, renames in place on double-click, and opens the frame menu on right-click
 * (or Shift+F10). An agent's frame carries the agent mark.
 */

import { useUpdateNodeInternals, type Node, type NodeProps } from "@xyflow/react";
import { memo, useLayoutEffect, useMemo, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { AgentMark } from "../../agent";
import { authorOf } from "../../code/text";
import { Icon } from "../../icons";
import type { Frame } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { clickItem, editFrame, findFrame, findPage, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { usePageCards } from "../../state/cards";
import { CardHandles, HeaderPorts } from "../cards/parts";
import type { Size } from "../layout";
import { useMeasure } from "../measure";
import { frameNodeWidth } from "../sizing";
import { useFrameMenu } from "./frameMenu";
import { holdsText } from "./holds";
import "./frames.css";

interface FrameNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** Ports with an arrow: a collapsed frame's node port. */
  readonly ports: readonly string[];
  /** An expanded frame's box; absent for a collapsed frame's node, which sizes itself. */
  readonly box?: Size;
}

export type FrameNodeType = Node<FrameNodeData, "frame">;

/** A frame's icon, or the plain frame's. */
function frameIcon(frame: Frame) {
  return frame.icon !== undefined ? (`frame-${frame.icon}` as const) : "frame";
}

/** Whether a key opens the frame menu from the keyboard. */
function opensMenu(event: KeyboardEvent): boolean {
  return (event.key === "F10" && event.shiftKey) || event.key === "ContextMenu";
}

/** The title, or the field that renames it in place (Enter keeps, Escape cancels; the new title is yours). */
function Title({ target, frame, editing, onDone }: { readonly target: PageTarget; readonly frame: Frame; readonly editing: boolean; readonly onDone: () => void }) {
  const author = useAuthor();
  if (!editing || author === null) {
    return <span className="frame-title-text">{frame.title}</span>;
  }
  const finish = (title: string | null) => {
    onDone();
    if (title !== null && title.trim().length > 0 && title.trim() !== frame.title) {
      editFrame(target, frame.id, { title: title.trim(), author });
    }
  };
  return (
    <input
      autoFocus
      className="frame-title-input nodrag nopan"
      aria-label="Frame title"
      defaultValue={frame.title}
      onClick={(event) => {
        event.stopPropagation();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") {
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
  );
}

export const FrameNode = memo(function FrameNode({ id, data }: NodeProps<FrameNodeType>) {
  const { target, ports, box } = data;
  const frame = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findFrame(page, id) : undefined;
  });
  const page = useOpenBoard(target.board, (board) => (box === undefined ? findPage(board.document, target.page) : undefined));
  const views = usePageCards(target);
  const holds = useMemo(() => (page ? holdsText(page, id, views) : ""), [page, id, views]);
  const review = useOpenBoard(target.board, (board) => board.health.items.some((item) => item.page === target.page && "frame" in item.target && item.target.frame === id));
  const selected = usePageView(target, (view) => view.selection.kind === "frame" && view.selection.frameId === id);
  const [editing, setEditing] = useState(false);
  const menu = useFrameMenu(target, id, () => {
    setEditing(true);
  });
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const portsKey = ports.join("\u0000");
  const collapsed = box === undefined;
  useLayoutEffect(() => {
    updateNodeInternals(id);
  }, [updateNodeInternals, id, portsKey, collapsed]);
  if (!frame) {
    return null;
  }
  const writer = authorOf(frame.author);
  const toggle = (
    <button
      type="button"
      className="frame-toggle nodrag"
      aria-label={`${collapsed ? "Expand" : "Collapse"} the frame ${frame.title}`}
      aria-expanded={!collapsed}
      onClick={(event) => {
        event.stopPropagation();
        editFrame(target, id, { collapsed: !collapsed });
      }}
      onKeyDown={(event) => {
        if (opensMenu(event)) {
          event.preventDefault();
          menu.open(event.currentTarget);
        }
      }}
    >
      <Icon name={collapsed ? "chevron-right-small" : "chevron-down-small"} size={10} />
    </button>
  );
  const heading: ReactNode = (
    <>
      {toggle}
      <Icon name={frameIcon(frame)} className="frame-icon" />
      <Title
        target={target}
        frame={frame}
        editing={editing}
        onDone={() => {
          setEditing(false);
        }}
      />
      {writer.agent ? <AgentMark name={writer.name} /> : null}
      {review ? <span className="frame-review-dot" aria-label="Needs review" /> : null}
    </>
  );
  const handlers = {
    onClick: () => {
      clickItem(target, id, { kind: "frame", frameId: id });
    },
    onDoubleClick: () => {
      setEditing(true);
    },
    onContextMenu: (event: MouseEvent) => {
      event.stopPropagation();
      menu.open(event);
    },
  };
  if (collapsed) {
    return (
      <article
        className={`board-card frame-node look-${frame.look}${selected ? " selected" : ""}`}
        data-frame-id={id}
        style={{ width: frameNodeWidth(frame.title, writer.agent, measure) }}
        aria-label={`Frame ${frame.title}, collapsed`}
        onContextMenu={handlers.onContextMenu}
      >
        <CardHandles />
        <header className="card-header frame-drag" onClick={handlers.onClick} onDoubleClick={handlers.onDoubleClick}>
          {heading}
          <HeaderPorts ids={ports} />
        </header>
        <div className="frame-node-body" onClick={handlers.onClick}>
          {frame.description ? <p className="frame-node-description">{frame.description.text}</p> : null}
          <p className="frame-node-holds">{holds}</p>
        </div>
        {menu.element}
      </article>
    );
  }
  return (
    <section
      className={`frame-box look-${frame.look}${selected ? " selected" : ""}`}
      data-frame-id={id}
      style={{ width: box.width, height: box.height }}
      aria-label={`Frame ${frame.title}`}
    >
      <CardHandles />
      <header className="frame-title frame-drag" onClick={handlers.onClick} onDoubleClick={handlers.onDoubleClick} onContextMenu={handlers.onContextMenu}>
        <div className="frame-title-row">{heading}</div>
        {frame.description ? (
          <p className="frame-description" title={frame.description.text}>
            {frame.description.text}
          </p>
        ) : null}
      </header>
      {menu.element}
    </section>
  );
});
