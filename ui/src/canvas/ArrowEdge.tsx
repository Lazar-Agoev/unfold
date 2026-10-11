/**
 * One arrow between cards: a curve from the calling row at the card's right edge into the target's
 * header, with a filled head. Arrows to opened calls are `--color-arrow`; an entrance's arrow to its
 * handler is the gray contour. Imports and passed references are dashed. Memoized.
 *
 * The curve doesn't take the pointer. A call or reference line carries its closed label where the
 * page's label layout puts it, clear of the cards (docs/proposals/typed-lines.md §7); selected, the
 * label opens into the call's contract. A line whose label fits nowhere at rest takes the click
 * itself: a wide invisible stroke over the curve, named like the label would be.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo, useMemo } from "react";
import type { LineLabel as LineLabelModel } from "../ipc/generated";
import { select, type PageTarget, type SelectedLine } from "../state/boards";
import { useLabelLayout, useLabelPlacement } from "./lines/labelLayout";
import { labelText } from "./lines/labelText";
import { LineContract } from "./lines/LineContract";
import { LineLabel } from "./lines/LineLabel";
import { bezierCurve } from "./lines/placeLabels";

interface ArrowData extends Record<string, unknown> {
  readonly dashed: boolean;
  /** An entrance's arrow to one of its handlers. */
  readonly handler: boolean;
  /** The line's closed label: null when it has none, absent while loading (and for handler arrows). */
  readonly label?: LineLabelModel | null;
  /** The page and the line as a selection, for a line that can carry a label. */
  readonly page?: PageTarget;
  readonly line?: SelectedLine;
  /** Whether the line is selected (its contract is open). */
  readonly selected?: boolean;
}

export type ArrowEdgeType = Edge<ArrowData, "arrow">;

/** A filled arrowhead pointing right whose tip is at (x, y): `length` long, `width` wide. */
function arrowHead(x: number, y: number, length: number, width: number): string {
  return `M${x - length} ${y - width / 2} L${x} ${y} L${x - length} ${y + width / 2} Z`;
}

/** The head is 7 long and 8 wide; the curve stops where it starts. */
const HEAD_LENGTH = 7;
const HEAD_WIDTH = 8;

export const ArrowEdge = memo(function ArrowEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<ArrowEdgeType>) {
  const endX = targetX - HEAD_LENGTH;
  const [path, labelX, labelY] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: endX, targetY, targetPosition });
  // The same curve as `path`, for the label layout (a new object only when an end moves).
  const curve = useMemo(
    () => bezierCurve({ x: sourceX, y: sourceY }, sourcePosition, { x: endX, y: targetY }, targetPosition),
    [sourceX, sourceY, sourcePosition, endX, targetY, targetPosition],
  );
  const placement = useLabelPlacement(useLabelLayout(), id);
  const tone = data?.handler ? " handler" : "";
  const { label, page, line, selected = false } = data ?? {};
  const labeled = label && page && line ? { label, page, line } : null;
  const onSelect = labeled
    ? () => {
        select(labeled.page, { kind: "line", ...labeled.line });
      }
    : undefined;
  return (
    <>
      <g className={`board-arrow${tone}`}>
        <path d={path} className={`board-arrow-line${data?.dashed ? " dashed" : ""}`} />
        <path d={arrowHead(targetX, targetY, HEAD_LENGTH, HEAD_WIDTH)} className="board-arrow-head" />
        {labeled && onSelect && !selected && placement === null ? (
          <path
            d={path}
            className="board-arrow-hit"
            role="button"
            tabIndex={0}
            aria-label={`Line ${labelText(labeled.label, false)}`}
            data-arrow={id}
            onClick={(event) => {
              event.stopPropagation();
              onSelect();
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect();
              }
            }}
          >
            <title>{labelText(labeled.label, false)}</title>
          </path>
        ) : null}
      </g>
      {labeled && onSelect ? (
        <EdgeLabelRenderer>
          {/* The label stays in the layout while its contract is open, so the others don't move. */}
          <LineLabel id={id} curve={curve} label={labeled.label} placement={selected ? null : placement} onSelect={onSelect} />
          {selected ? <LineContract target={labeled.page} line={labeled.line} x={labelX} y={labelY} curve={curve} /> : null}
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
