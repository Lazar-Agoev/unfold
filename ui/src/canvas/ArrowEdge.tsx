/**
 * One arrow between cards: a curve from the calling row at the card's right edge into the target's
 * header, with a filled head. Arrows to opened calls are `--color-arrow`; an entrance's arrow to its
 * handler is the gray contour. Imports and passed references are dashed. Memoized.
 *
 * The curve doesn't take the pointer. A call or reference line carries its closed label where the
 * page's label layout puts it, clear of the cards (docs/reader.md); selected, the
 * label opens into the call's contract. An arrow from a Diagram node that stands for several calls
 * reads "3 calls" and selects them (Selected lists them, each opening its contract here). A type
 * arrow, from a type card's field to the type it names, is drawn like a call's and labeled with the
 * field's written type, which selects nothing. An arrow that joins several at a collapsed frame reads
 * "3 calls" too, as quiet text. A line whose label fits nowhere at rest takes the click itself: a wide
 * invisible stroke over the curve, named like the label would be.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo, useMemo } from "react";
import type { LineLabel as LineLabelModel, TypeText } from "../ipc/generated";
import { select, type PageTarget, type SelectedCalls, type SelectedLine } from "../state/boards";
import { useLabelLayout, useLabelPlacement } from "./lines/labelLayout";
import { countForms, countText, labelForms, labelText, mergedText, typeForms } from "./lines/labelText";
import { LineContract } from "./lines/LineContract";
import { LineLabel } from "./lines/LineLabel";
import { bezierCurve } from "./lines/placeLabels";

interface ArrowData extends Record<string, unknown> {
  readonly dashed: boolean;
  /** An entrance's arrow to one of its handlers. */
  readonly handler: boolean;
  /** The line's closed label: null when it has none, absent while loading (and for other arrows). */
  readonly label?: LineLabelModel | null;
  /** The page, for an arrow whose label selects something. */
  readonly page?: PageTarget;
  /** The line it draws; for an arrow that stands for several calls, the one whose contract is open. */
  readonly line?: SelectedLine;
  /** Whether the line is selected (its contract is open). */
  readonly selected?: boolean;
  /** The calls an arrow that stands for several selects (Selected lists them). */
  readonly calls?: SelectedCalls;
  /** Whether those calls are selected. */
  readonly listed?: boolean;
  /** A type arrow's label: the field's written type. */
  readonly type?: TypeText;
  /** What an arrow that joins several at a collapsed frame stands for: its quiet count label. */
  readonly merged?: { readonly calls: number; readonly fields: number };
}

export type ArrowEdgeType = Edge<ArrowData, "arrow">;

/** A filled arrowhead pointing right whose tip is at (x, y): `length` long, `width` wide. */
function arrowHead(x: number, y: number, length: number, width: number): string {
  return `M${x - length} ${y - width / 2} L${x} ${y} L${x - length} ${y + width / 2} Z`;
}

/** The head is 7 long and 8 wide; the curve stops where it starts. */
const HEAD_LENGTH = 7;
const HEAD_WIDTH = 8;

/** What an arrow's label says and what clicking it selects. */
interface Labeled {
  readonly forms: ReturnType<typeof labelForms>;
  readonly full: string;
  readonly onSelect?: () => void;
}

function labeledOf(data: ArrowData | undefined): Labeled | null {
  const { label, page, line, calls, type, merged } = data ?? {};
  if (type) {
    return { forms: typeForms(type), full: type.text };
  }
  if (merged) {
    const full = mergedText(merged.calls, merged.fields);
    return { forms: [[{ text: full, tone: "type" }]], full };
  }
  if (calls && page) {
    return {
      forms: countForms(calls.calls.length),
      full: countText(calls.calls.length),
      onSelect: () => {
        select(page, { kind: "calls", ...calls });
      },
    };
  }
  if (label && page && line) {
    return {
      forms: labelForms(label),
      full: labelText(label, false),
      onSelect: () => {
        select(page, { kind: "line", ...line });
      },
    };
  }
  return null;
}

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
  const { page, line, selected = false, listed = false } = data ?? {};
  const labeled = useMemo(() => labeledOf(data), [data]);
  const onSelect = labeled?.onSelect;
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
            aria-label={`Line ${labeled.full}`}
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
            <title>{labeled.full}</title>
          </path>
        ) : null}
      </g>
      {labeled ? (
        <EdgeLabelRenderer>
          {/* The label stays in the layout while its contract is open, so the others don't move. */}
          <LineLabel
            id={id}
            curve={curve}
            forms={labeled.forms}
            full={labeled.full}
            placement={selected ? null : placement}
            selected={listed}
            {...(onSelect ? { onSelect } : {})}
          />
          {selected && page && line ? <LineContract target={page} line={line} x={labelX} y={labelY} curve={curve} /> : null}
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
