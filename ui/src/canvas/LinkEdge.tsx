/**
 * A drawn link between two items: a dashed, signed curve. Selected, it is the accent at 1.75px with a
 * hollow dot where it starts; otherwise the gray contour; while an agent that changed it is working, a
 * soft authored band. Its label is a chip with the author's name (and the agent mark for an agent) in
 * the typed lines' label layout, as a connection's chip is, so it never covers a card, a typed label,
 * or a chip; where it fits nowhere it isn't drawn at rest and the line takes the click (Selected shows
 * the label). A click selects the link, a double-click edits its label in the sidebar, and its menu
 * (`linkMenuEntries`) offers Kind ▸ (which makes it a connection), Edit label, Copy link, and Remove
 * link. Memoized.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo, useLayoutEffect, useMemo } from "react";
import { AgentMark } from "../agent";
import { useContextMenu } from "../components";
import type { Measure } from "../flow/layout";
import { requestFocus } from "../keyboard";
import { useAuthor } from "../state/author";
import { select, type PageTarget } from "../state/boards";
import { setSelectedCollapsed } from "../state/layout";
import { LINK_LABEL_FIELD } from "./cards/select";
import { INTO, arrowHead } from "./connections/ends";
import { linkMenuEntries } from "./connections/linkMenu";
import { useLabelLayout, useLabelPlacement } from "./lines/labelLayout";
import { bezierCurve } from "./lines/placeLabels";
import { LINK_AUTHOR_FONT, LINK_LABEL_FONT, useMeasure } from "./measure";

interface LinkData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** The link; absent for one that stands for several at a collapsed frame (its label selects nothing). */
  readonly linkId?: string;
  readonly selected: boolean;
  readonly label: string | null;
  /** The label's author, as the chip shows it ("Lazar", "Claude"). */
  readonly author: string | null;
  /** Whether an agent wrote the label: the chip carries the agent mark. */
  readonly agent: boolean;
  /** Whether an agent that changed it is still working. */
  readonly working: boolean;
}

export type LinkEdgeType = Edge<LinkData, "link">;

/** The label's height, and its padding, border, and gap between its words and its author (canvas.css). */
const LABEL_HEIGHT = 24;
const LABEL_FRAME = 8 * 2 + 2;
const AUTHOR_GAP = 6;
/** The agent mark's spark and the gap after it (agent.css). */
const MARK_SPARK = 10 + 2;

/** The label's width, as canvas.css draws it: its words, then "· author" and the agent mark. */
function labelWidth(label: string, author: string | null, agent: boolean, measure: Measure): number {
  const words = measure(label, LINK_LABEL_FONT);
  const byline = author !== null ? AUTHOR_GAP + measure(`· ${author}`, LINK_AUTHOR_FONT) + (agent ? measure(" agent", LINK_AUTHOR_FONT) + MARK_SPARK : 0) : 0;
  return Math.ceil(LABEL_FRAME + words + byline);
}

/** Selects a link and opens its label field in the sidebar. */
function editLabel(target: PageTarget, linkId: string): void {
  select(target, { kind: "link", linkId });
  setSelectedCollapsed(false);
  requestFocus(LINK_LABEL_FIELD);
}

export const LinkEdge = memo(function LinkEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<LinkEdgeType>) {
  const menu = useContextMenu("Link actions", 196);
  const author = useAuthor();
  const layout = useLabelLayout();
  const measure = useMeasure();
  const selected = data?.selected ?? false;
  const length = selected ? 8 : 7;
  const direction = INTO[targetPosition];
  const endX = targetX - direction[0] * length;
  const endY = targetY - direction[1] * length;
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: endX, targetY: endY, targetPosition });
  // The same curve as `path`, for the label layout (a new object only when an end moves).
  const curve = useMemo(() => bezierCurve({ x: sourceX, y: sourceY }, sourcePosition, { x: endX, y: endY }, targetPosition), [sourceX, sourceY, sourcePosition, endX, endY, targetPosition]);
  const label = data?.label ?? null;
  const byline = data?.author ?? null;
  const agent = data?.agent ?? false;
  const width = useMemo(() => (label !== null ? labelWidth(label, byline, agent, measure) : null), [label, byline, agent, measure]);
  useLayoutEffect(() => {
    layout.report(id, width !== null ? { curve, widths: [width], height: LABEL_HEIGHT } : null);
  }, [layout, id, curve, width]);
  useLayoutEffect(
    () => () => {
      layout.report(id, null);
    },
    [layout, id],
  );
  const placement = useLabelPlacement(layout, id);
  const target = data?.target;
  const linkId = data?.linkId;
  return (
    <>
      <g className={`board-link${selected ? " selected" : ""}${data?.working === true ? " working" : ""}`}>
        <path d={path} className="board-link-hit" />
        <path d={path} className="board-link-line" />
        <path d={arrowHead(targetX, targetY, direction, length, selected ? 9 : 8)} className="board-link-head" />
        {selected ? <circle cx={sourceX} cy={sourceY} r={3.5} className="board-link-start" /> : null}
      </g>
      {label !== null && width !== null && placement && target ? (
        <EdgeLabelRenderer>
          <button
            type="button"
            className={`board-link-label nodrag nopan${selected ? " selected" : ""}`}
            style={{ width, transform: `translate(-50%, -50%) translate(${placement.x}px, ${placement.y}px)` }}
            onClick={(event) => {
              event.stopPropagation();
              if (linkId !== undefined) {
                select(target, { kind: "link", linkId });
              }
            }}
            onDoubleClick={(event) => {
              event.stopPropagation();
              if (linkId !== undefined) {
                editLabel(target, linkId);
              }
            }}
            onContextMenu={(event) => {
              event.stopPropagation();
              if (linkId !== undefined) {
                select(target, { kind: "link", linkId });
                menu.open(event, linkMenuEntries(target, linkId, author));
              }
            }}
          >
            <span className="board-link-text">{label}</span>
            {byline !== null ? (
              <span className="board-link-author">
                · {byline}
                {agent ? (
                  <>
                    {" "}
                    <AgentMark name={byline} />
                  </>
                ) : null}
              </span>
            ) : null}
          </button>
          {menu.element}
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
