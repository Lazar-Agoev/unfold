/**
 * Card widths (V-57): block cards size to their content between 280 and 880 px, entrance cards between
 * 200 and 320 px, measured with the bundled fonts; longer lines soft-wrap and nothing scrolls inside a
 * card. Heights follow from the content and are measured by the canvas. The constants follow
 * cards.css and code.css (02 §4).
 */

import type { CodeText } from "../ipc/generated";
import type { Measure } from "../flow/layout";
import { ENTRANCE_MAX_WIDTH, ENTRANCE_MIN_WIDTH, clampCardWidth } from "./layout";
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
