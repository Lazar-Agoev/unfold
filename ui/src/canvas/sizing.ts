/**
 * Card widths (V-57): block cards size to their content between 280 and 880 px, entrance cards between
 * 200 and 320 px, measured with the bundled fonts; longer lines soft-wrap and nothing scrolls inside a
 * card. Heights follow from the content and are measured by the canvas; before a card is drawn, its
 * height and an entrance card's handler rows are estimated from its model, so cards a step opens are
 * placed in that step. The constants follow cards.css and code.css (02 §4).
 */

import type { CardView, CodeText } from "../ipc/generated";
import type { Measure } from "../flow/layout";
import { shortPath } from "../code/text";
import { ENTRANCE_MAX_WIDTH, ENTRANCE_MIN_WIDTH, clampCardWidth, fallbackSize, type Size } from "./layout";
import { CODE_FONT, LOCATION_FONT, METHOD_FONT, TITLE_FONT } from "./measure";

/** The card's border, left and right. */
const BORDER = 2;
/** The header's and body's inline padding, each side. */
const PADDING = 12;
/** Gaps between the header's parts. */
const GAP = 8;
/** The `ƒ` glyph. */
const GLYPH = 8;
/** A cross-project card's 20px monogram. */
const MONOGRAM = 20;
/** The card's Code | Flow mini lens. */
const LENS = 84;
/** The line-number gutter: at least 40px, else the digits, 14px after them, and 4 before. */
const GUTTER_MIN = 40;
const GUTTER_PAD = 18;
/** The space the code view leaves after the text (code.css `.code-row` padding). */
const TEXT_END = 20;
/** The tab size the code view uses. */
const TAB = 2;
/** A block card's header with its mini lens, its code's top and bottom padding, and a code row. */
const BLOCK_HEADER = 44;
const CODE_PADDING = 18;
const CODE_ROW = 20;
/** A header's block padding (each side) and the rule under it. */
const HEADER_PADDING = 10;
const RULE = 1;
/** A header's line: the title's, an entrance's method badge's, or a cross-project card's monogram. */
const TITLE_LINE = 16;
const METHOD_LINE = 18;
/** An entrance card's body: its top padding, the gap between its parts, and their lines (11px/14px, 12px/16px). */
const ENTRANCE_BODY_TOP = 10;
const ENTRANCE_GAP = 8;
const SMALL_LINE = 14;
const HANDLER_ROW = 16;
const HANDLER_GAP = 4;

/** The visible length of a line in columns, after the hidden indentation (tabs count `TAB`). */
function columns(text: string, start: number, end: number): number {
  let count = 0;
  for (let index = start; index < end; index += 1) {
    count += text.charCodeAt(index) === 9 ? TAB : 1;
  }
  return count;
}

/** The width of the line-number gutter for numbers of `digits` digits. */
export function gutterWidth(digits: number, measure: Measure): number {
  return Math.max(GUTTER_MIN, Math.ceil(digits * measure("0", CODE_FONT)) + GUTTER_PAD);
}

/** The width a Code view needs to show its longest line unwrapped. */
export function codeContentWidth(code: CodeText, measure: Measure): number {
  const ch = measure("0", CODE_FONT);
  const digits = String(code.lines.at(-1)?.number ?? 1).length;
  let widest = 0;
  code.lines.forEach((line, index) => {
    const skip = index === 0 ? 0 : Math.min(code.dedent, line.indent);
    widest = Math.max(widest, columns(code.text, line.start + skip, line.end) * ch);
  });
  return Math.ceil(gutterWidth(digits, measure) + widest + TEXT_END + BORDER + 1);
}

export interface BlockWidthInput {
  readonly name: string;
  readonly location: string;
  /** Whether the header starts with the project monogram (a card from another project). */
  readonly monogram: boolean;
  /** The body's own width need (Code or Flow), or null while loading. */
  readonly body: number | null;
}

/** The width a block card's header needs: monogram, `ƒ`, name, location, and the mini lens. */
export function headerWidth(input: Omit<BlockWidthInput, "body">, measure: Measure): number {
  const monogram = input.monogram ? MONOGRAM + GAP : 0;
  const text = measure(input.name, TITLE_FONT) + GAP + measure(input.location, LOCATION_FONT);
  return Math.ceil(PADDING * 2 + BORDER + monogram + GLYPH + GAP + text + GAP + LENS);
}

/** A block card's width: its header and body need, clamped to 280–880. */
export function blockCardWidth(input: BlockWidthInput, measure: Measure): number {
  return clampCardWidth(Math.max(headerWidth(input, measure), input.body ?? 0));
}

/** A block card's size before it is drawn: its width from its model, its height from its lines. */
export function estimatedBlockSize(view: CardView | undefined, monogram: boolean, measure: Measure): Size {
  const code = view?.type === "block" && view.code?.source.state === "current" ? view.code.source : null;
  if (view?.type !== "block" || !code) {
    return fallbackSize(false);
  }
  const width = blockCardWidth(
    { name: view.header.qualifiedName, location: shortPath(view.header.path), monogram, body: codeContentWidth(code, measure) },
    measure,
  );
  // The header, the code's padding, and one row per line (long lines wrap, so this is a floor).
  return { width, height: BLOCK_HEADER + CODE_PADDING + code.lines.length * CODE_ROW };
}

/** What an entrance card shows: its method badge (with a trigger), heading, location, and handler names. */
export interface EntranceWidthInput {
  /** `POST`, or null for an entrance without a method. */
  readonly method: string | null;
  readonly title: string;
  readonly location: string;
  readonly handlers: readonly string[];
}

/** An entrance card's width: its header, location, and handler rows, clamped to 200–320 (the heading always fits). */
export function entranceCardWidth(input: EntranceWidthInput, measure: Measure): number {
  const method = input.method !== null ? measure(input.method, METHOD_FONT) + GAP : 0;
  const header = method + measure(input.title, TITLE_FONT);
  const body = Math.max(
    measure(input.location, LOCATION_FONT),
    ...input.handlers.map((name) => measure(name, CODE_FONT)),
  );
  return clampCardWidth(Math.max(header, body) + PADDING * 2 + BORDER, ENTRANCE_MIN_WIDTH, ENTRANCE_MAX_WIDTH);
}

/**
 * Where an entrance card `width` wide draws its handler rows: each row's center below the card's top,
 * in order (the header, the wrapped location, "Handlers, in order", then one 16px row per handler).
 */
export function entranceHandlerRows(input: EntranceWidthInput, monogram: boolean, width: number, measure: Measure): number[] {
  const header = Math.max(TITLE_LINE, input.method !== null ? METHOD_LINE : 0, monogram ? MONOGRAM : 0);
  const locationLines = Math.max(1, Math.ceil(measure(input.location, LOCATION_FONT) / (width - BORDER - PADDING * 2)));
  const first =
    BORDER / 2 + HEADER_PADDING * 2 + header + RULE + ENTRANCE_BODY_TOP + locationLines * SMALL_LINE + ENTRANCE_GAP + SMALL_LINE + ENTRANCE_GAP;
  return input.handlers.map((_, index) => first + index * (HANDLER_ROW + HANDLER_GAP) + HANDLER_ROW / 2);
}
