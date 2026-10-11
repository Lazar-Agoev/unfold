/**
 * A drawn link between two items: a dashed, signed curve. Selected, it is the accent at 1.75px with a
 * hollow dot where it starts; otherwise the gray contour; while an agent that changed it is working, a
 * soft authored band. Its label sits on the curve's midpoint as a chip with the author's name (and the
 * agent mark for an agent), outside the typed lines' label layout; a click selects the link, a
 * double-click edits its label in the sidebar, and its menu (`linkMenuEntries`) offers Kind ▸ (which
 * makes it a connection), Edit label, Copy link, and Remove link. Memoized.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo } from "react";
import { AgentMark } from "../agent";
import { useContextMenu } from "../components";
import { requestFocus } from "../keyboard";
import { useAuthor } from "../state/author";
import { select, type PageTarget } from "../state/boards";
import { setSelectedCollapsed } from "../state/layout";
import { LINK_LABEL_FIELD } from "./cards/select";
import { INTO, arrowHead } from "./connections/ends";
import { linkMenuEntries } from "./connections/linkMenu";

interface LinkData extends Record<string, unknown> {
  readonly target: PageTarget;
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

/** Selects a link and opens its label field in the sidebar. */
function editLabel(target: PageTarget, linkId: string): void {
  select(target, { kind: "link", linkId });
  setSelectedCollapsed(false);
  requestFocus(LINK_LABEL_FIELD);
}

export const LinkEdge = memo(function LinkEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<LinkEdgeType>) {
  const menu = useContextMenu("Link actions", 196);
  const author = useAuthor();
  const selected = data?.selected ?? false;
  const length = selected ? 8 : 7;
  const direction = INTO[targetPosition];
  const [path, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX: targetX - direction[0] * length,
    targetY: targetY - direction[1] * length,
    targetPosition,
  });
  const target = data?.target;
  return (
    <>
      <g className={`board-link${selected ? " selected" : ""}${data?.working === true ? " working" : ""}`}>
        <path d={path} className="board-link-hit" />
        <path d={path} className="board-link-line" />
        <path d={arrowHead(targetX, targetY, direction, length, selected ? 9 : 8)} className="board-link-head" />
        {selected ? <circle cx={sourceX} cy={sourceY} r={3.5} className="board-link-start" /> : null}
      </g>
      {typeof data?.label === "string" && target ? (
        <EdgeLabelRenderer>
          <button
            type="button"
            className={`board-link-label nodrag nopan${selected ? " selected" : ""}`}
            style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
            onClick={(event) => {
              event.stopPropagation();
              select(target, { kind: "link", linkId: id });
            }}
            onDoubleClick={(event) => {
              event.stopPropagation();
              editLabel(target, id);
            }}
            onContextMenu={(event) => {
              event.stopPropagation();
              select(target, { kind: "link", linkId: id });
              menu.open(event, linkMenuEntries(target, id, author));
            }}
          >
            <span className="board-link-text">{data.label}</span>
            {data.author !== null ? (
              <span className="board-link-author">
                · {data.author}
                {data.agent ? (
                  <>
                    {" "}
                    <AgentMark name={data.author} />
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
