/**
 * One arrow between cards: a curve from the call at the card's right edge (or the card's edge) into
 * the target's header, with a filled head. Call arrows are `--color-arrow`; an app's or entrance's
 * arrow to its code is the gray contour, dashed to stale code. Memoized.
 *
 * A call's arrow takes the click on a wide invisible stroke over its curve: it selects the call, and
 * Selected shows its contract. An arrow from a compact card that stands for several calls selects them
 * (Selected lists them). Start arrows, type arrows (from a type card's field to the type it names),
 * and arrows that join several at a collapsed frame select nothing.
 */

import { getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo } from "react";
import { select, type PageTarget, type SelectedCalls, type SelectedLine } from "../state/boards";

interface ArrowData extends Record<string, unknown> {
  readonly dashed: boolean;
  /** An app's or entrance's arrow to its code. */
  readonly start: boolean;
  /** The page, for an arrow that selects something. */
  readonly page?: PageTarget;
  /** The call it draws. */
  readonly line?: SelectedLine;
  /** The calls an arrow that stands for several selects (Selected lists them). */
  readonly calls?: SelectedCalls;
  /** Whether the arrow is selected: its line, or the calls it stands for. */
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

/** What clicking the arrow selects, and its accessible name. */
function selectionOf(data: ArrowData | undefined): { readonly name: string; readonly onSelect: () => void } | null {
  const { page, line, calls } = data ?? {};
  if (calls && page) {
    return {
      name: `${calls.calls.length} calls`,
      onSelect: () => {
        select(page, { kind: "calls", ...calls });
      },
    };
  }
  if (line && page) {
    return {
      name: "Call",
      onSelect: () => {
        select(page, { kind: "line", ...line });
      },
    };
  }
  return null;
}

export const ArrowEdge = memo(function ArrowEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<ArrowEdgeType>) {
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: targetX - HEAD_LENGTH, targetY, targetPosition });
  const tone = data?.start ? " start" : "";
  const selected = data?.selected === true ? " selected" : "";
  const selection = selectionOf(data);
  return (
    <g className={`board-arrow${tone}${selected}`}>
      <path d={path} className={`board-arrow-line${data?.dashed ? " dashed" : ""}`} />
      <path d={arrowHead(targetX, targetY, HEAD_LENGTH, HEAD_WIDTH)} className="board-arrow-head" />
      {selection ? (
        <path
          d={path}
          className="board-arrow-hit"
          role="button"
          tabIndex={0}
          aria-label={selection.name}
          aria-pressed={data?.selected === true}
          data-arrow={id}
          onClick={(event) => {
            event.stopPropagation();
            selection.onSelect();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              selection.onSelect();
            }
          }}
        />
      ) : null}
    </g>
  );
});
