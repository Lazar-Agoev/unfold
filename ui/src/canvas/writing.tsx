/**
 * An agent writing on the canvas. Nothing outlines the item: a chip floats above its top-right
 * corner, 6px off it, with a ring whose bright tail sweeps round once every 0.9 s and the agent's name;
 * it keeps its size on screen at any zoom. Only the field being written changes: its words arrive left
 * to right, settled words in their ink and the next few blurred, fainter the further ahead. The chip
 * fades in over 150 ms. When the mark settles (finish_work, or two minutes without a write) the words
 * sharpen and the chip fades out over 300 ms (writing.css). With reduced motion the ring stands still,
 * full, and the words are there at once.
 */

import { EdgeLabelRenderer } from "@xyflow/react";
import { useEffect, useState, type CSSProperties } from "react";
import type { PageTarget } from "../state/boards";
import { usePageWriting, type ItemWriting, type WrittenField } from "../state/boards/agent/working";
import type { Rect } from "./layout";
import "./writing.css";

export { useItemWriting } from "../state/boards/agent/working";

/** How fast words arrive, and how many lead them blurred: three tiers of two words, each fainter. */
const WORD_MS = 60;
const TIER_WORDS = 2;
const TIERS = 3;

/** Whether the OS asks for reduced motion. */
function reducedMotion(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The ring: an accent comet's tail sweeping round (still and full with reduced motion). */
function Ring() {
  return <span className="writing-ring" aria-hidden="true" />;
}

/** The chip over an item: the ring and the agent's name. */
function WritingChip({ writing, style }: { readonly writing: ItemWriting; readonly style: CSSProperties }) {
  return (
    <span className={`writing-chip${writing.settling ? " settling" : ""}`} style={style} role="status" aria-label={`${writing.agent} is writing`}>
      <Ring />
      {writing.agent}
    </span>
  );
}

/** The ring alone in its chip, for a connection: it floats above the chip it is placed in. */
export function WritingRing({ writing, className }: { readonly writing: ItemWriting; readonly className?: string }) {
  return (
    <span
      className={`writing-chip ring-only${writing.settling ? " settling" : ""}${className !== undefined ? ` ${className}` : ""}`}
      role="status"
      aria-label={`${writing.agent} is writing`}
    >
      <Ring />
    </span>
  );
}

/**
 * The chips over a page's items an agent is writing, each at its item's top-right corner (`rects`, as drawn),
 * moved by `offsets` where an item draws past its rectangle (an expanded frame's edge, as CSS lengths).
 */
export function WritingChips({
  target,
  rects,
  offsets,
}: {
  readonly target: PageTarget;
  readonly rects: ReadonlyMap<string, Rect>;
  readonly offsets: ReadonlyMap<string, { readonly x: string; readonly y: string }>;
}) {
  const writing = usePageWriting(target);
  const chips = Object.entries(writing).flatMap(([id, item]) => {
    const rect = rects.get(id);
    return rect && item.fields.length > 0 ? [{ id, item, rect }] : [];
  });
  if (chips.length === 0) {
    return null;
  }
  return (
    <EdgeLabelRenderer>
      {chips.map(({ id, item, rect }) => (
        <WritingChip
          key={id}
          writing={item}
          style={{
            transform: `translate(${rect.x + rect.width}px, ${rect.y}px) translate(${offsets.get(id)?.x ?? "0px"}, ${offsets.get(id)?.y ?? "0px"}) scale(var(--screen-scale, 1)) translate(6px, -6px) translate(-100%, -100%)`,
          }}
        />
      ))}
    </EdgeLabelRenderer>
  );
}

/** How many of `total` words have arrived by `now`, `at` being when they were written. */
function arrivedBy(now: number, at: number, total: number): number {
  return Math.min(total, Math.max(0, Math.floor((now - at) / WORD_MS)));
}

/** How many words have arrived since `at`, kept current while they arrive: all of them with reduced motion. */
function useArrived(at: number, total: number, active: boolean): number {
  const [arrived, setArrived] = useState(() => arrivedBy(Date.now(), at, total));
  useEffect(() => {
    if (!active) {
      return undefined;
    }
    const timer = setInterval(() => {
      const next = arrivedBy(Date.now(), at, total);
      setArrived(next);
      if (next >= total) {
        clearInterval(timer);
      }
    }, WORD_MS);
    return () => {
      clearInterval(timer);
    };
  }, [at, total, active]);
  return reducedMotion() ? total : arrived;
}

/** A word's state: settled, blurred in tier 1 to 3 (further ahead, fainter), or not arrived. */
function wordClass(index: number, arrived: number): string {
  if (index >= arrived) {
    return "writing-word ahead";
  }
  const lead = arrived - 1 - index;
  if (lead >= TIERS * TIER_WORDS) {
    return "writing-word";
  }
  return `writing-word tier-${TIERS - Math.floor(lead / TIER_WORDS)}`;
}

/** `text` as words (each with its place among them) and the spaces between them, as written. */
function words(text: string): { readonly text: string; readonly word: number | null }[] {
  let count = 0;
  return text
    .split(/(\s+)/)
    .filter((piece) => piece.length > 0)
    .map((piece) => {
      if (/^\s/.test(piece)) {
        return { text: piece, word: null };
      }
      count += 1;
      return { text: piece, word: count - 1 };
    });
}

/** Words arriving, as `writing` writes them. */
function ArrivingWords({ text, writing }: { readonly text: string; readonly writing: ItemWriting }) {
  const pieces = words(text);
  const total = pieces.filter((piece) => piece.word !== null).length;
  const arrived = useArrived(writing.at, total, !writing.settling);
  return pieces.map((piece, index) =>
    piece.word === null ? (
      piece.text
    ) : (
      <span key={index} className={writing.settling ? "writing-word" : wordClass(piece.word, arrived)}>
        {piece.text}
      </span>
    ),
  );
}

/** A field's text: arriving while an agent writes it (`writing`, its mark on the item), as it is otherwise. */
export function WritingText({ text, writing, field }: { readonly text: string; readonly writing: ItemWriting | undefined; readonly field: WrittenField }) {
  if (!writing?.fields.includes(field)) {
    return text;
  }
  return <ArrivingWords text={text} writing={writing} />;
}
