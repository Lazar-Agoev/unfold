/**
 * Card widths (architecture §10.2): cards size to their content between 360 and 880 px, measured
 * with the bundled fonts; longer lines soft-wrap and nothing scrolls inside a card. Heights follow
 * from the content and are measured by the canvas.
 */

import type { CodeText } from "../ipc/generated";
import type { Measure } from "../flow/layout";
import { clampCardWidth } from "./layout";
import { CODE_FONT, TITLE_FONT, UI_FONT } from "./measure";

/** The card's border, left and right. */
const BORDER = 2;
/** The fold column and the space after line numbers (code.css). */
const GUTTER_PX = 24;
/** The space the code view leaves after the text (code.css `.code-row` padding). */
const TEXT_END = 20;
/** A calling line with a port also draws a leader to the card's edge (code.css `.code-leader`). */
const LEADER = 42;
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

/** The width a Code view needs to show its longest line unwrapped (with a leader where a port sits). */
export function codeContentWidth(code: CodeText, portLines: ReadonlySet<number>, measure: Measure): number {
  const ch = measure("0", CODE_FONT);
  const digits = String(code.lines.at(-1)?.number ?? 1).length;
  let widest = 0;
  code.lines.forEach((line, index) => {
    const skip = index === 0 ? 0 : Math.min(code.dedent, line.indent);
    const width = columns(code.text, line.start + skip, line.end) * ch + (portLines.has(line.number) ? LEADER : 0);
    widest = Math.max(widest, width);
  });
  return Math.ceil(GUTTER_PX + (digits + 1) * ch + widest + TEXT_END + BORDER + 1);
}

/** The width a block card's header needs: icon, name, location, switch, and fold toggle. */
export function headerWidth(name: string, location: string, measure: Measure): number {
  const controls = 150;
  return Math.ceil(16 + 22 + measure(name, TITLE_FONT) + 10 + measure(location, UI_FONT) * 0.92 + controls + BORDER);
}

export interface BlockWidthInput {
  readonly name: string;
  readonly location: string;
  /** The body's own width need (Code or Flow), or null while loading. */
  readonly body: number | null;
}

/** A block card's width: its header and body need, clamped to 360–880. */
export function blockCardWidth(input: BlockWidthInput, measure: Measure): number {
  return clampCardWidth(Math.max(headerWidth(input.name, input.location, measure), input.body ?? 0));
}

/** An entrance card's width: its title, evidence lines, and target rows. */
export function entranceCardWidth(title: string, lines: readonly string[], targets: readonly string[], measure: Measure): number {
  const ch = measure("0", CODE_FONT);
  const titleWidth = measure(title, '600 18px "JetBrains Mono Variable", ui-monospace, monospace') + 120;
  const evidence = lines.reduce((widest, line) => Math.max(widest, line.length * ch), 0) + 32 + BORDER;
  const rows = targets.reduce((widest, target) => Math.max(widest, measure(target, CODE_FONT)), 0) + 150;
  return clampCardWidth(Math.max(titleWidth, evidence, rows));
}
