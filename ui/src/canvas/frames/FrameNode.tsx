/**
 * A frame on the canvas: what the agent or the person adds around cards, drawn as quiet ground so the
 * cards inside carry the color. Expanded, it is behind what it holds, sized by `pageGeometry`: a **box**
 * is a hairline outline with its title written inside its top-left corner and its description beside it;
 * a **zone** is a gray ground with a large title and its description under it; a box in a zone has a
 * faint white fill. A title grows on screen as you zoom out, from its corner (titles.ts); a box title
 * fades out before it runs into the title of a frame around it. Only its title takes the pointer, so the
 * canvas pans and cards are clicked through it. Collapsed, it is one node like a card, a stack of them:
 * its title, how many cards it holds, and its description, with the arrows and links of what it holds
 * attached. Who drew it shows in Selected.
 *
 * Its chevron (shown beside the title while hovered or selected, with the review dot) collapses and
 * expands it (nothing collapses by itself); its title selects it, drags it with everything in it,
 * renames in place on double-click, and opens the frame menu on right-click (or Shift+F10). While an
 * agent writes its title or description, the words arrive (writing.tsx).
 */

import { useUpdateNodeInternals, type Node, type NodeProps } from "@xyflow/react";
import { memo, useLayoutEffect, useMemo, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { Icon } from "../../icons";
import type { Frame } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { clickItem, editFrame, findFrame, findPage, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import type { ItemWriting } from "../../state/boards/agent/working";
import { CardHandles, HeaderPorts } from "../cards/parts";
import type { Size } from "../layout";
import { useMeasure } from "../measure";
import { frameNodeWidth } from "../sizing";
import { WritingText, useItemWriting } from "../writing";
import { useFrameMenu } from "./frameMenu";
import { holdsText } from "./holds";
import "./frames.css";

interface FrameNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** Ports with an arrow: a collapsed frame's node port. */
  readonly ports: readonly string[];
  /** An expanded frame's box; absent for a collapsed frame's node, which sizes itself. */
  readonly box?: Size;
  /** Whether an expanded box sits in a zone (it gets a faint white fill). */
  readonly inZone?: boolean;
  /** The zoom an expanded box's title is gone by, before it runs into an enclosing frame's title (titles.ts). */
  readonly titleFadeAt?: number;
}

export type FrameNodeType = Node<FrameNodeData, "frame">;

/** Whether a key opens the frame menu from the keyboard. */
function opensMenu(event: KeyboardEvent): boolean {
  return (event.key === "F10" && event.shiftKey) || event.key === "ContextMenu";
}

/** The title, or the field that renames it in place (Enter keeps, Escape cancels; the new title is yours). */
function Title({
  target,
  frame,
  editing,
  writing,
  onDone,
}: {
  readonly target: PageTarget;
  readonly frame: Frame;
  readonly editing: boolean;
  readonly writing: ItemWriting | undefined;
  readonly onDone: () => void;
}) {
  const author = useAuthor();
  if (!editing || author === null) {
    return (
      <span className="frame-title-text">
        <WritingText text={frame.title} writing={writing} field="title" />
      </span>
    );
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
  const { target, ports, box, inZone = false, titleFadeAt } = data;
  const frame = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findFrame(page, id) : undefined;
  });
  const page = useOpenBoard(target.board, (board) => (box === undefined ? findPage(board.document, target.page) : undefined));
  const holds = useMemo(() => (page ? holdsText(page, id) : ""), [page, id]);
  const review = useOpenBoard(target.board, (board) => board.health.items.some((item) => item.page === target.page && "frame" in item.target && item.target.frame === id));
  const selected = usePageView(target, (view) => view.selection.kind === "frame" && view.selection.frameId === id);
  const writing = useItemWriting(target, id);
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
      <Title
        target={target}
        frame={frame}
        editing={editing}
        writing={writing}
        onDone={() => {
          setEditing(false);
        }}
      />
      {toggle}
      {review ? <span className="frame-review-dot" aria-label="Needs review" /> : null}
    </>
  );
  const description = frame.description ? <WritingText text={frame.description.text} writing={writing} field="description" /> : null;
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
        style={{ width: frameNodeWidth(frame.title, measure) }}
        aria-label={`Frame ${frame.title}, collapsed`}
        onContextMenu={handlers.onContextMenu}
      >
        <CardHandles />
        <header className="frame-node-head frame-drag" onClick={handlers.onClick} onDoubleClick={handlers.onDoubleClick}>
          {heading}
          <span className="frame-node-holds">{holds}</span>
          <HeaderPorts ids={ports} />
        </header>
        {description !== null ? (
          <p className="frame-node-description" onClick={handlers.onClick}>
            {description}
          </p>
        ) : null}
        {menu.element}
      </article>
    );
  }
  const zone = frame.look === "zone";
  const style = { width: box.width, height: box.height, ...(titleFadeAt !== undefined ? { "--title-fade-at": titleFadeAt } : {}) } as CSSProperties;
  return (
    <section
      className={`frame-box look-${frame.look}${inZone ? " in-zone" : ""}${titleFadeAt !== undefined ? " fades" : ""}${selected ? " selected" : ""}`}
      data-frame-id={id}
      style={style}
      aria-label={`Frame ${frame.title}`}
    >
      <CardHandles />
      <header className="frame-title frame-drag" onClick={handlers.onClick} onDoubleClick={handlers.onDoubleClick} onContextMenu={handlers.onContextMenu}>
        {heading}
        {!zone && description !== null ? (
          <p className="frame-description" title={frame.description?.text}>
            {description}
          </p>
        ) : null}
      </header>
      {zone && description !== null ? (
        <p className="frame-description" title={frame.description?.text}>
          {description}
        </p>
      ) : null}
      {menu.element}
    </section>
  );
});
