/**
 * One arrow between cards: a curve from the calling row at the card's right edge into the target's
 * header, with a filled head. Arrows to opened calls are `--color-arrow`; an entrance's arrow to its
 * handler is the gray contour. Imports and passed references are dashed. Memoized.
 *
 * The curve never takes the pointer. A call or reference line carries its closed label at its
 * midpoint (docs/proposals/typed-lines.md §7); selected, the label opens into the call's contract.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo } from "react";
import type { LineLabel as LineLabelModel } from "../ipc/generated";
import { select, type PageTarget, type SelectedLine } from "../state/boards";
import { LineContract } from "./lines/LineContract";
import { LineLabel } from "./lines/LineLabel";

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
  const [path, labelX, labelY] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: targetX - HEAD_LENGTH, targetY, targetPosition });
  const tone = data?.handler ? " handler" : "";
  const { label, page, line, selected = false } = data ?? {};
  return (
    <>
      <g className={`board-arrow${tone}`}>
        <path d={path} className={`board-arrow-line${data?.dashed ? " dashed" : ""}`} />
        <path d={arrowHead(targetX, targetY, HEAD_LENGTH, HEAD_WIDTH)} className="board-arrow-head" />
      </g>
      {label && page && line ? (
        <EdgeLabelRenderer>
          {selected ? (
            <LineContract target={page} line={line} x={labelX} y={labelY} />
          ) : (
            <LineLabel
              id={id}
              x={labelX}
              y={labelY}
              label={label}
              onSelect={() => {
                select(page, { kind: "line", ...line });
              }}
            />
          )}
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
