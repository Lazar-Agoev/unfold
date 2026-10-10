/**
 * One arrow between cards (02 §4.7): a curve from the calling row at the card's right edge into the
 * target's header, with a filled head. Arrows to opened calls are `--color-arrow`; an entrance's arrow
 * to its handler is the gray contour. Imports and passed references are dashed. Memoized.
 */

import { getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo } from "react";

interface ArrowData extends Record<string, unknown> {
  readonly dashed: boolean;
  /** An entrance's arrow to one of its handlers. */
  readonly handler: boolean;
}

export type ArrowEdgeType = Edge<ArrowData, "arrow">;

/** A filled arrowhead pointing right whose tip is at (x, y): `length` long, `width` wide. */
function arrowHead(x: number, y: number, length: number, width: number): string {
  return `M${x - length} ${y - width / 2} L${x} ${y} L${x - length} ${y + width / 2} Z`;
}

/** The head is 7 long and 8 wide; the curve stops where it starts. */
const HEAD_LENGTH = 7;
const HEAD_WIDTH = 8;

export const ArrowEdge = memo(function ArrowEdge({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<ArrowEdgeType>) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: targetX - HEAD_LENGTH, targetY, targetPosition });
  const tone = data?.handler ? " handler" : "";
  return (
    <g className={`board-arrow${tone}`}>
      <path d={path} className={`board-arrow-line${data?.dashed ? " dashed" : ""}`} />
      <path d={arrowHead(targetX, targetY, HEAD_LENGTH, HEAD_WIDTH)} className="board-arrow-head" />
    </g>
  );
});
