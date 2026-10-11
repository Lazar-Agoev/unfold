/**
 * A connection (design §4.7): a line in the authored color, in its kind's pattern (solid for HTTP,
 * RPC, reads and writes; long dashes for a queue; dots for an event; short dashes for a runtime call;
 * thin with crow's-foot ends for a relationship; dashed for other), with a filled head where the data
 * goes (into the table for `writes`, back into the code for `reads`), and a chip that says its kind and
 * what travels, with the agent mark when an agent drew it and a review dot while something about it
 * needs review. The chip joins the typed lines' label layout, so it never covers a card, a typed
 * label, or another chip; where it fits nowhere it isn't drawn at rest and the line takes the click.
 *
 * A click on the chip selects the connection; on the carried type's name it opens the type's
 * definition, as on a typed line; a double-click edits its label; its menu is the link menu. Memoized.
 */

import { EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import { memo, useLayoutEffect, useMemo, useState, type MouseEvent } from "react";
import { AgentMark } from "../../agent";
import { TypeDefinition } from "../../code/TypeDefinition";
import { Popover, useContextMenu } from "../../components";
import type { Cardinality, LinkKind, ProjectId } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { select, toggledOpenTypes, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";
import { LINK_LABEL_FIELD } from "../cards/select";
import { CHIP_CODE_FONT, CHIP_TEXT_FONT, CHIP_WORD_FONT, useMeasure } from "../measure";
import { useLabelLayout, useLabelPlacement } from "../lines/labelLayout";
import { bezierCurve, type Curve } from "../lines/placeLabels";
import type { Measure } from "../../flow/layout";
import type { ChipPart } from "./chip";
import { INTO, arrowHead, cardinalityMark, manyEnds } from "./ends";
import { linkMenuEntries } from "./linkMenu";
import "./connections.css";

/** What a connection's edge draws (`BoardCanvas` computes it from the page). */
export interface ConnectionData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** The connection's link; absent for one that stands for several at a collapsed frame (it selects nothing). */
  readonly linkId?: string;
  readonly kind: LinkKind;
  /** Its chip's forms, longest first, and its whole text (its accessible name). */
  readonly forms: readonly (readonly ChipPart[])[];
  readonly full: string;
  /** The agent that drew it ("Claude"), for the mark; null for a person. */
  readonly agent: string | null;
  readonly selected: boolean;
  /** Whether an agent that changed it is still working. */
  readonly working: boolean;
  /** What needs review about it, when something does. */
  readonly problem: string | null;
  /** A relationship's cardinality: its ends' marks. */
  readonly cardinality?: Cardinality;
  /** The type it carries, which the chip's name of it opens. */
  readonly carries?: { readonly project: ProjectId; readonly key: string };
}

export type ConnectionEdgeType = Edge<ConnectionData, "connection">;

/** The head's length and width; the line stops where it starts. */
const HEAD_LENGTH = 7;
const HEAD_WIDTH = 8;
/** The chip: its height, padding and border, and what the agent mark and the review dot add. */
const CHIP_HEIGHT = 20;
const CHIP_FRAME = 8 * 2 + 2;
const MARK_ROOM = 4 + 10 + 2;
const DOT_ROOM = 6 + 6;

const PART_FONTS: Readonly<Record<ChipPart["tone"], string>> = {
  word: CHIP_WORD_FONT,
  code: CHIP_CODE_FONT,
  text: CHIP_TEXT_FONT,
  sep: CHIP_TEXT_FONT,
};

/** A chip form's width, as connections.css draws it. */
function chipWidth(parts: readonly ChipPart[], agent: boolean, problem: boolean, measure: Measure): number {
  const text = parts.reduce((sum, part) => sum + measure(part.text, PART_FONTS[part.tone]), 0);
  const mark = agent ? MARK_ROOM + measure("agent", CHIP_TEXT_FONT) : 0;
  return Math.ceil(CHIP_FRAME + text + mark + (problem ? DOT_ROOM : 0));
}

interface ChipProps {
  readonly id: string;
  readonly curve: Curve;
  readonly data: ConnectionData;
}

/** The chip, where the page's label layout puts it, and the carried type's definition when it is opened. */
function ConnectionChip({ id, curve, data }: ChipProps) {
  const layout = useLabelLayout();
  const measure = useMeasure();
  const author = useAuthor();
  const menu = useContextMenu("Connection actions", 220);
  // The chip, while the carried type's definition is open under it.
  const [opened, setOpened] = useState<HTMLElement | null>(null);
  const [openTypes, setOpenTypes] = useState<Readonly<Record<string, number>>>({});
  const { target, linkId, forms, full, agent, problem, selected, carries } = data;
  const widths = useMemo(() => forms.map((form) => chipWidth(form, agent !== null, problem !== null, measure)), [forms, agent, problem, measure]);
  useLayoutEffect(() => {
    layout.report(id, { curve, widths, height: CHIP_HEIGHT });
  }, [layout, id, curve, widths]);
  useLayoutEffect(
    () => () => {
      layout.report(id, null);
    },
    [layout, id],
  );
  const placement = useLabelPlacement(layout, id);
  const parts = placement ? forms[placement.form] : undefined;
  if (!placement || !parts) {
    return null;
  }
  const onClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (carries && event.target instanceof Element && event.target.closest("[data-carries]")) {
      const button = event.currentTarget;
      setOpened((open) => (open ? null : button));
      return;
    }
    if (linkId !== undefined) {
      select(target, { kind: "link", linkId });
    }
  };
  return (
    <>
      <button
        type="button"
        className={`connection-chip kind-${data.kind} nodrag nopan${selected ? " selected" : ""}`}
        style={{ width: widths[placement.form], transform: `translate(-50%, -50%) translate(${placement.x}px, ${placement.y}px)` }}
        aria-label={`Connection ${full}`}
        title={full}
        data-link={id}
        onClick={onClick}
        onDoubleClick={(event) => {
          event.stopPropagation();
          if (linkId !== undefined) {
            select(target, { kind: "link", linkId });
            showInSelected(LINK_LABEL_FIELD);
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
        {parts.map((part, index) => (
          <span key={index} className={`chip-${part.tone}${part.carries ? " chip-carries" : ""}`} {...(part.carries ? { "data-carries": "" } : {})}>
            {part.text}
          </span>
        ))}
        {agent !== null ? <AgentMark name={agent} /> : null}
        {problem !== null ? <span className="chip-review" title={problem} aria-label={problem} /> : null}
      </button>
      {opened && carries ? (
        <Popover
          open
          anchor={opened}
          label="The type it carries"
          onClose={() => {
            setOpened(null);
          }}
          className="connection-carries"
        >
          <TypeDefinition
            project={carries.project}
            typeKey={carries.key}
            row="carries"
            depth={1}
            openTypes={openTypes}
            onToggle={(row, span) => {
              setOpenTypes((current) => toggledOpenTypes(current, row, span));
            }}
          />
        </Popover>
      ) : null}
      {menu.element}
    </>
  );
}

export const ConnectionEdge = memo(function ConnectionEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<ConnectionEdgeType>) {
  const kind = data?.kind ?? "other";
  const relates = kind === "relates";
  // The head goes where the data goes: back into the code for reads, else into `to`; a relationship has none.
  const headAtSource = kind === "reads";
  const out = INTO[sourcePosition];
  const into = INTO[targetPosition];
  const startX = headAtSource ? sourceX - out[0] * HEAD_LENGTH : sourceX;
  const startY = headAtSource ? sourceY - out[1] * HEAD_LENGTH : sourceY;
  const endX = relates || headAtSource ? targetX : targetX - into[0] * HEAD_LENGTH;
  const endY = relates || headAtSource ? targetY : targetY - into[1] * HEAD_LENGTH;
  const [path] = getBezierPath({ sourceX: startX, sourceY: startY, sourcePosition, targetX: endX, targetY: endY, targetPosition });
  // The same curve as `path`, for the label layout (a new object only when an end moves).
  const curve = useMemo(
    () => bezierCurve({ x: startX, y: startY }, sourcePosition, { x: endX, y: endY }, targetPosition),
    [startX, startY, sourcePosition, endX, endY, targetPosition],
  );
  const selected = data?.selected ?? false;
  let marks: string | null = null;
  if (relates) {
    const many = data?.cardinality ? manyEnds(data.cardinality) : null;
    marks = many ? `${cardinalityMark(sourceX, sourceY, sourcePosition, many.from)} ${cardinalityMark(targetX, targetY, targetPosition, many.to)}` : null;
  }
  const head = relates ? null : headAtSource ? arrowHead(sourceX, sourceY, out, HEAD_LENGTH, HEAD_WIDTH) : arrowHead(targetX, targetY, into, HEAD_LENGTH, HEAD_WIDTH);
  return (
    <>
      <g className={`board-connection kind-${kind}${selected ? " selected" : ""}${data?.working === true ? " working" : ""}`} data-link={id}>
        <path d={path} className="connection-hit" />
        <path d={path} className="connection-line" />
        {head !== null ? <path d={head} className="connection-head" /> : null}
        {marks !== null ? <path d={marks} className="connection-foot" /> : null}
      </g>
      {data ? (
        <EdgeLabelRenderer>
          <ConnectionChip id={id} curve={curve} data={data} />
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
