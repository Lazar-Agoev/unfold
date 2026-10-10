/**
 * A board thumbnail's drawing (01 "How thumbnails are drawn", D-26, plan §9): the first page's cards as
 * rectangles sized by the canvas's fallback rules, fitted into a 252×138 box with 22px margins, each
 * with two to five text lines, joined by 1px edges. Pure and memoized per `Thumbnail` object.
 */

import { boundsOf, fallbackSize, type Rect } from "../../canvas/layout";
import type { Thumbnail, ThumbnailCard } from "../../ipc";

export const THUMB_WIDTH = 252;
export const THUMB_HEIGHT = 138;
const MARGIN = 22;
/** The largest scale: a page of one or two cards still reads as a sketch, not a close-up. */
const MAX_SCALE = 0.12;
const LINE_GAP = 8;
const LINE_INSET = 7.5;
const MONOGRAM = 9;

export interface ThumbLine {
  readonly x1: number;
  readonly x2: number;
  readonly y: number;
  readonly accent: boolean;
}

export interface ThumbCard {
  readonly id: string;
  /** On half pixels, for crisp 1px strokes. */
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly lines: readonly ThumbLine[];
  /** The 9px monogram square of a card from another project. */
  readonly monogram: { readonly x: number; readonly y: number } | null;
}

export interface ThumbDrawing {
  readonly cards: readonly ThumbCard[];
  /** SVG path data of graph edges and of drawn links (dashed). */
  readonly edges: string;
  readonly links: string;
}

/** The canvas size a card would have before it is measured (plan §9). */
export function cardSize(card: Pick<ThumbnailCard, "kind" | "folded" | "lines">): { width: number; height: number } {
  if (card.kind === "entrance") {
    return fallbackSize(true);
  }
  const { width } = fallbackSize(false);
  if (card.folded) {
    return { width, height: 44 };
  }
  return card.lines === null ? fallbackSize(false) : { width, height: 44 + 20 * Math.min(card.lines, 24) };
}

/** A stable 0..1 number for a string, so a card's line lengths don't change between renders. */
function unit(seed: string, salt: number): number {
  let hash = 2166136261 ^ salt;
  for (let index = 0; index < seed.length; index += 1) {
    hash = Math.imul(hash ^ seed.charCodeAt(index), 16777619);
  }
  return ((hash >>> 0) % 1000) / 1000;
}

function lineCount(height: number): number {
  return Math.max(2, Math.min(5, Math.floor((height - 8) / 10)));
}

const drawings = new WeakMap<Thumbnail, ThumbDrawing>();

export function thumbnailDrawing(thumbnail: Thumbnail): ThumbDrawing {
  const cached = drawings.get(thumbnail);
  if (cached) {
    return cached;
  }
  const drawing = draw(thumbnail);
  drawings.set(thumbnail, drawing);
  return drawing;
}

function draw(thumbnail: Thumbnail): ThumbDrawing {
  const rects: (Rect & { card: ThumbnailCard })[] = thumbnail.cards.map((card) => ({
    card,
    x: card.at[0],
    y: card.at[1],
    ...cardSize(card),
  }));
  const bounds = boundsOf(rects);
  if (!bounds) {
    return { cards: [], edges: "", links: "" };
  }
  const scale = Math.min((THUMB_WIDTH - 2 * MARGIN) / bounds.width, (THUMB_HEIGHT - 2 * MARGIN) / bounds.height, MAX_SCALE);
  const left = (THUMB_WIDTH - bounds.width * scale) / 2;
  const top = (THUMB_HEIGHT - bounds.height * scale) / 2;
  const accentCard = thumbnail.cards.find((card) => card.kind === "entrance")?.id;

  const cards = rects.map(({ card, x, y, width, height }): ThumbCard => {
    const w = Math.max(12, Math.round(width * scale));
    const h = Math.max(12, Math.round(height * scale));
    const cx = Math.round(left + (x - bounds.x) * scale) + 0.5;
    const cy = Math.round(top + (y - bounds.y) * scale) + 0.5;
    const count = lineCount(h);
    const first = Math.round(cy + h / 2 - ((count - 1) * LINE_GAP) / 2);
    const start = Math.round(cx + LINE_INSET);
    const end = cx + w - LINE_INSET;
    const lines = Array.from({ length: count }, (_, index): ThumbLine => {
      const lead = index === 0 && card.foreign ? MONOGRAM + 4 : 0;
      const room = end - start - lead;
      return {
        x1: start + lead,
        x2: Math.round(start + lead + room * (0.5 + 0.45 * unit(card.id, index))),
        y: first + index * LINE_GAP,
        accent: index === 0 && card.id === accentCard,
      };
    });
    return {
      id: card.id,
      x: cx,
      y: cy,
      width: w,
      height: h,
      lines,
      monogram: card.foreign ? { x: start, y: first - MONOGRAM / 2 } : null,
    };
  });

  const byId = new Map(cards.map((card) => [card.id, card]));
  const connect = (pairs: readonly (readonly [string, string])[]) =>
    pairs
      .flatMap(([from, to]) => {
        const a = byId.get(from);
        const b = byId.get(to);
        return a && b ? [edgePath(a, b)] : [];
      })
      .join("");
  return { cards, edges: connect(thumbnail.edges), links: connect(thumbnail.links) };
}

/** From one card's side to the other's: straight when level, else a small stepped curve (as drawn). */
function edgePath(a: ThumbCard, b: ThumbCard): string {
  const forward = b.x >= a.x;
  const x1 = Math.round(forward ? a.x + a.width : a.x);
  const x2 = Math.round(forward ? b.x : b.x + b.width);
  const y1 = Math.round(a.y + a.height / 2);
  const y2 = Math.round(b.y + b.height / 2);
  if (y1 === y2) {
    return `M${x1} ${y1}H${x2}`;
  }
  const mid = Math.round((x1 + x2) / 2);
  const step = forward ? 4 : -4;
  return `M${x1} ${y1}H${mid - step}C${mid} ${y1} ${mid} ${y2} ${mid + step} ${y2}H${x2}`;
}
