/**
 * A connection: a slate line in its kind's pattern (solid for HTTP, reads and writes; long dashes for
 * a queue; short dashes for a runtime call; dashed for other), drawn by the page's router
 * (`useLineRoute`): orthogonal, around the cards it doesn't join, spread along the side it enters
 * beside the page's other connections there. A filled head goes where the data goes (into the table
 * for `writes`, back into the code for `reads`); a relationship is a blue line with an ERD's ends (two
 * bars for one, a crow's foot for many), row to row, a "C" between stacked tables. At rest its chip, borderless on the canvas,
 * says its kind's word and what it touches, with a review dot while something about it needs review.
 * Hovered or selected, the line turns the accent and its card floats where the chip is: everything it
 * says, and its description. The chip joins the page's label layout, so it never covers a card, a
 * link's label, or another chip; where it fits nowhere it isn't drawn at rest and the line takes the
 * click. Below 60% chips fade out (canvas.css), and hovering still shows the card. While an agent
 * writes it, what it touches blurs and a ring floats above the chip.
 *
 * A click on the chip or the card selects the connection; a double-click edits its label; its menu is
 * the link menu. Memoized.
 */

import { EdgeLabelRenderer, type Edge, type EdgeProps } from "@xyflow/react";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { useContextMenu } from "../../components";
import type { Cardinality, ConnectionKind } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { select, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";
import { CHIP_CODE_FONT, CHIP_TEXT_FONT, CHIP_WORD_FONT, useMeasure } from "../measure";
import { useLabelLayout, useLabelPlacement } from "../lines/labelLayout";
import { useLineRoute } from "../lines/lineRoutes";
import { lineMiddle, roundedPath, trimmed } from "../lines/paths";
import type { Point } from "../layout";
import type { Measure } from "../../flow/layout";
import type { ItemWriting } from "../../state/boards/agent/working";
import { WritingRing, useItemWriting } from "../writing";
import type { ChipPart } from "./chip";
import { INTO, arrowHead, cardinalityMark, manyEnds } from "./ends";
import { LINK_LABEL_FIELD, linkMenuEntries } from "./linkMenu";
import "./connections.css";

/** What a connection's edge draws (`BoardCanvas` computes it from the page). */
export interface ConnectionData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** The connection; absent for one that stands for several at a collapsed frame (it selects nothing). */
  readonly linkId?: string;
  readonly kind: ConnectionKind;
  /** Its chip's forms at rest, longest first (none for a relationship). */
  readonly forms: readonly (readonly ChipPart[])[];
  /** All it says in a line (its card's first line), and as text (its accessible name). */
  readonly title: readonly ChipPart[];
  readonly full: string;
  /** What it means, in words: its card's second line. */
  readonly description: string | null;
  readonly selected: boolean;
  /** What needs review about it, when something does. */
  readonly problem: string | null;
  /** A relationship's cardinality: its ends' marks. */
  readonly cardinality?: Cardinality;
}

export type ConnectionEdgeType = Edge<ConnectionData, "connection">;

/** The head's length and width; the line stops where it starts. */
const HEAD_LENGTH = 8;
const HEAD_WIDTH = 8;
/** The chip: its height, its padding, the gap between its parts, and what the review dot adds. */
const CHIP_HEIGHT = 20;
const CHIP_PADDING = 6 * 2;
const CHIP_GAP = 5;
const DOT_ROOM = 6 + 6;
/** How long the card stays after the pointer leaves the line, so it can move onto the card. */
const LEAVE_MS = 120;

const PART_FONTS: Readonly<Record<ChipPart["tone"], string>> = {
  word: CHIP_WORD_FONT,
  code: CHIP_CODE_FONT,
  text: CHIP_TEXT_FONT,
};

/** A chip form's width, as connections.css draws it. */
function chipWidth(parts: readonly ChipPart[], problem: boolean, measure: Measure): number {
  const text = parts.reduce((sum, part) => sum + measure(part.text, PART_FONTS[part.tone]), 0);
  return Math.ceil(CHIP_PADDING + text + CHIP_GAP * Math.max(0, parts.length - 1) + (problem ? DOT_ROOM : 0));
}

/** How what a connection touches shows while an agent writes it: blurred, then sharpening as the mark settles. */
function targetClass(writing: ItemWriting | undefined): string {
  if (!writing) {
    return "";
  }
  return writing.settling ? " writing-target" : " writing-target blurred";
}

/** The parts of a chip or a card's first line: a part after what it touches follows a " · ". */
function Parts({ parts, writing }: { readonly parts: readonly ChipPart[]; readonly writing: ItemWriting | undefined }) {
  return parts.map((part, index) => (
    <span key={index} className={`chip-${part.tone}${part.tone === "code" && index === 1 ? targetClass(writing) : ""}`}>
      {index > 1 || (index === 1 && part.tone !== "code") ? `· ${part.text}` : part.text}
    </span>
  ));
}

interface ChipProps {
  readonly id: string;
  /** Its line's points. */
  readonly points: readonly Point[];
  readonly data: ConnectionData;
  /** Whether the pointer is on its line, chip, or card. */
  readonly hovered: boolean;
  readonly onHover: (hovered: boolean) => void;
}

/** The chip, where the page's label layout puts it; hovered or selected, the card in its place. */
function ConnectionChip({ id, points, data, hovered, onHover }: ChipProps) {
  const layout = useLabelLayout();
  const measure = useMeasure();
  const author = useAuthor();
  const menu = useContextMenu("Connection actions", 220);
  const { target, linkId, forms, title, full, description, problem, selected } = data;
  const writing = useItemWriting(target, linkId ?? id);
  const widths = useMemo(() => forms.map((form) => chipWidth(form, problem !== null, measure)), [forms, problem, measure]);
  useLayoutEffect(() => {
    layout.report(id, widths.length > 0 ? { points, widths, height: CHIP_HEIGHT } : null);
  }, [layout, id, points, widths]);
  useLayoutEffect(
    () => () => {
      layout.report(id, null);
    },
    [layout, id],
  );
  const placement = useLabelPlacement(layout, id);
  const parts = placement ? forms[placement.form] : undefined;
  const open = hovered || selected;
  if (!open && (!placement || !parts)) {
    return null;
  }
  const at = placement ?? lineMiddle(points);
  const handlers = {
    onMouseEnter: () => {
      onHover(true);
    },
    onMouseLeave: () => {
      onHover(false);
    },
    onClick: (event: MouseEvent) => {
      event.stopPropagation();
      if (linkId !== undefined) {
        select(target, { kind: "link", linkId });
      }
    },
    onDoubleClick: (event: MouseEvent) => {
      event.stopPropagation();
      if (linkId !== undefined) {
        select(target, { kind: "link", linkId });
        showInSelected(LINK_LABEL_FIELD);
      }
    },
    onContextMenu: (event: MouseEvent) => {
      event.stopPropagation();
      if (linkId !== undefined) {
        select(target, { kind: "link", linkId });
        menu.open(event, linkMenuEntries(target, linkId, author));
      }
    },
  };
  const review = problem !== null ? <span className="chip-review" title={problem} aria-label={problem} /> : null;
  const ring = writing ? <WritingRing writing={writing} className="connection-writing" /> : null;
  return (
    <>
      {open ? (
        <div
          role="button"
          tabIndex={-1}
          className={`connection-card kind-${data.kind} nodrag nopan${selected ? " selected" : ""}`}
          style={{ transform: `translate(${at.x}px, ${at.y}px) scale(var(--hover-scale, 1)) translate(-50%, -15px)` }}
          aria-label={`Connection ${full}`}
          data-link={id}
          {...handlers}
        >
          <p className="connection-card-title">
            <Parts parts={title} writing={writing} />
            {review}
          </p>
          {description !== null ? <p className="connection-card-description">{description}</p> : null}
          {ring}
        </div>
      ) : (
        <button
          type="button"
          className={`connection-chip kind-${data.kind} nodrag nopan`}
          style={{ width: widths[placement?.form ?? 0], transform: `translate(-50%, -50%) translate(${at.x}px, ${at.y}px)` }}
          aria-label={`Connection ${full}`}
          data-link={id}
          {...handlers}
        >
          <Parts parts={parts ?? []} writing={writing} />
          {review}
          {ring}
        </button>
      )}
      {menu.element}
    </>
  );
}

/** Whether the pointer is on a connection's line, chip, or card, with a short grace after it leaves. */
function useHover(): readonly [boolean, (hovered: boolean) => void] {
  const [hovered, setHovered] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
      }
    },
    [],
  );
  const onHover = useCallback((next: boolean) => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    if (next) {
      setHovered(true);
    } else {
      timer.current = setTimeout(() => {
        timer.current = null;
        setHovered(false);
      }, LEAVE_MS);
    }
  }, []);
  return [hovered, onHover];
}

export const ConnectionEdge = memo(function ConnectionEdge({ id, sourceX, sourceY, targetX, targetY, data }: EdgeProps<ConnectionEdgeType>) {
  const kind = data?.kind ?? "other";
  const relates = kind === "relates";
  // The head goes where the data goes: back into the code for reads, else into `to`; a relationship has none.
  const headAtSource = kind === "reads";
  const route = useLineRoute(id);
  const middle = (sourceX + targetX) / 2;
  // Until it is routed: across, then down the gap.
  const points = useMemo(
    () =>
      route?.points ?? [
        { x: sourceX, y: sourceY },
        { x: middle, y: sourceY },
        { x: middle, y: targetY },
        { x: targetX, y: targetY },
      ],
    [route, sourceX, sourceY, middle, targetX, targetY],
  );
  const from = route?.from ?? "right";
  const to = route?.to ?? "left";
  const start = points[0] ?? { x: sourceX, y: sourceY };
  const end = points[points.length - 1] ?? { x: targetX, y: targetY };
  const path = roundedPath(relates ? points : trimmed(points, headAtSource ? HEAD_LENGTH : 0, headAtSource ? 0 : HEAD_LENGTH));
  const selected = data?.selected ?? false;
  const [hovered, onHover] = useHover();
  let marks: string | null = null;
  if (relates) {
    const many = data?.cardinality ? manyEnds(data.cardinality) : null;
    marks = many ? `${cardinalityMark(start.x, start.y, from, many.from)} ${cardinalityMark(end.x, end.y, to, many.to)}` : null;
  }
  const head = relates ? null : headAtSource ? arrowHead(start.x, start.y, INTO[from], HEAD_LENGTH, HEAD_WIDTH) : arrowHead(end.x, end.y, INTO[to], HEAD_LENGTH, HEAD_WIDTH);
  return (
    <>
      <g
        className={`board-connection kind-${kind}${selected || hovered ? " open" : ""}`}
        data-link={id}
        onMouseEnter={() => {
          onHover(true);
        }}
        onMouseLeave={() => {
          onHover(false);
        }}
      >
        <path d={path} className="connection-hit" />
        <path d={path} className="connection-line" />
        {head !== null ? <path d={head} className="connection-head" /> : null}
        {marks !== null ? <path d={marks} className="connection-foot" /> : null}
      </g>
      {data ? (
        <EdgeLabelRenderer>
          <ConnectionChip id={id} points={points} data={data} hovered={hovered} onHover={onHover} />
        </EdgeLabelRenderer>
      ) : null}
    </>
  );
});
