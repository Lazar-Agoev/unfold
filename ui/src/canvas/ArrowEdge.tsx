/**
 * One arrow between cards: the page's router draws it (`useLineRoute`), orthogonal with rounded
 * corners, from its port (a call's row, a compact card's header) into the target's header, around the
 * cards in between; arrows into one card join into one trunk there. It ends in a filled head pointing
 * into the side it enters. Call arrows are `--color-arrow`; an app's or entrance's arrow to its code is
 * the gray contour, dashed to stale code. Memoized.
 *
 * A call's arrow takes the click on a wide invisible stroke over its curve: it selects the call, and
 * Selected shows its contract. An arrow from a compact card that stands for several calls selects them
 * (Selected lists them). Start arrows, type arrows (from a type card's field to the type it names),
 * and arrows that join several at a collapsed frame select nothing.
 */

import type { Edge, EdgeProps } from "@xyflow/react";
import { memo } from "react";
import { select, type PageTarget, type SelectedCalls, type SelectedLine } from "../state/boards";
import { INTO, arrowHead } from "./connections/ends";
import { useLineRoute } from "./lines/lineRoutes";
import { roundedPath, trimmed } from "./lines/paths";

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

/** The head is 7 long and 8 wide; the line stops where it starts. */
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

export const ArrowEdge = memo(function ArrowEdge({ id, sourceX, sourceY, targetX, targetY, data }: EdgeProps<ArrowEdgeType>) {
  const route = useLineRoute(id);
  // Until it is routed: across from the port, then down the gap into the header.
  const middle = (sourceX + targetX) / 2;
  const points = route?.points ?? [
    { x: sourceX, y: sourceY },
    { x: middle, y: sourceY },
    { x: middle, y: targetY },
    { x: targetX, y: targetY },
  ];
  const end = points[points.length - 1] ?? { x: targetX, y: targetY };
  const path = roundedPath(trimmed(points, 0, HEAD_LENGTH));
  const tone = data?.start ? " start" : "";
  const selected = data?.selected === true ? " selected" : "";
  const selection = selectionOf(data);
  return (
    <g className={`board-arrow${tone}${selected}`}>
      <path d={path} className={`board-arrow-line${data?.dashed ? " dashed" : ""}`} />
      <path d={arrowHead(end.x, end.y, INTO[route?.to ?? "left"], HEAD_LENGTH, HEAD_WIDTH)} className="board-arrow-head" />
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
