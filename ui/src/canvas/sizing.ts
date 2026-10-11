/**
 * Card widths: block cards size to their content between 280 and 880 px, entrance cards between 200
 * and 320 px, Diagram nodes and collapsed frames' nodes between 240 and 400 px, type cards between
 * 280 and 560 px, resource cards between 200 and 360 px (tables 240 to 520), and notes between 140
 * and 260 px, measured with the bundled fonts; longer lines soft-wrap and nothing scrolls inside a card. Heights follow from
 * the content and are measured by the canvas; before a card is drawn, its height and an entrance card's
 * handler rows are estimated from its model, so cards a step opens are placed in that step. The
 * constants follow cards.css, code.css, and types.css.
 */

import type { CardView, CodeText, Description, Frame, Key, TypeDefinition, View } from "../ipc/generated";
import type { Measure } from "../flow/layout";
import { shortPath } from "../code/text";
import { cardBody, keyTypeWord, memberName, memberValue, typeKindWord } from "../code/typeKinds";
import { fieldRows } from "./arrows";
import { entranceCardText, entranceTitle } from "./entranceText";
import { ENTRANCE_MAX_WIDTH, ENTRANCE_MIN_WIDTH, clampCardWidth, fallbackSize, type Size } from "./layout";
import { CODE_FONT, DESCRIPTION_FONT, FRAME_TITLE_FONT, LOCATION_FONT, MARK_FONT, METHOD_FONT, TITLE_FONT } from "./measure";
import { columnTypeText, listedColumns, markText, type TableColumn } from "./resources/columns";

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
/** The card's Diagram | Flow | Code mini lens. */
const LENS = 146;
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

/**
 * Where line `line` of a block card's Code is before the card is drawn: its row's center below the
 * card's top (long lines wrap, so this is a floor); undefined when the model has no current Code or
 * doesn't show the line.
 */
export function estimatedLineOffset(view: CardView | undefined, line: number): number | undefined {
  const code = view?.type === "block" && view.code?.source.state === "current" ? view.code.source : null;
  const first = code?.lines[0]?.number;
  if (!code || first === undefined || line < first || line > first + code.lines.length - 1) {
    return undefined;
  }
  return BLOCK_HEADER + CODE_PADDING / 2 + (line - first + 0.5) * CODE_ROW;
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

// ---------------------------------------------------------------------------------------------
// Diagram nodes and type cards

/** Diagram nodes size to their header between these widths; the description wraps inside. */
export const NODE_MIN_WIDTH = 240;
export const NODE_MAX_WIDTH = 400;
/** Type cards size to their header and their widest field between these widths. */
const TYPE_MIN_WIDTH = 280;
export const TYPE_MAX_WIDTH = 560;
/** A description's line, and the strip's block padding and rule. */
const DESCRIPTION_LINE = 18;
const DESCRIPTION_PADDING = 16;
/** A type card's fields: the grid's block padding (8 above), a row, a base's "from" heading, and "N more". */
const FIELDS_PADDING = 18;
const FIELDS_TOP = 8;
const FIELD_ROW = 20;
const FROM_HEADING = 22;
const MORE_ROW = 24;
/** The name column's least width, and the gap after the widest name (types.css). */
const NAME_COLUMN = 84;
const NAME_GAP = 8;
/** A type card with this many fields or more lists the first `FIELDS_FOLDED` and "N more". */
const FOLD_FIELDS_AT = 12;
export const FIELDS_FOLDED = 8;

/** Where a Diagram node or a type card is: `routes/orders.ts:25`. */
export function nodeLocation(path: string, line: number | null): string {
  return line !== null ? `${shortPath(path)}:${line}` : shortPath(path);
}

/** What a Diagram node's header shows: its mark (`ƒ`, or an entrance's kind), its name and location, and the switch. */
export interface NodeWidthInput {
  readonly mark: string;
  readonly name: string;
  readonly location: string;
  /** Whether the header starts with the project monogram (a card from another project). */
  readonly monogram: boolean;
  /** Whether it has its own Diagram | Flow | Code switch (an entrance has none). */
  readonly lens: boolean;
}

/** A Diagram node's width: its header, clamped to 240–400. */
export function nodeWidth(input: NodeWidthInput, measure: Measure): number {
  const monogram = input.monogram ? MONOGRAM + GAP : 0;
  const text = measure(input.mark, METHOD_FONT) + GAP + measure(input.name, TITLE_FONT) + GAP + measure(input.location, LOCATION_FONT);
  return clampCardWidth(PADDING * 2 + BORDER + monogram + text + (input.lens ? GAP + LENS : 0), NODE_MIN_WIDTH, NODE_MAX_WIDTH);
}

/**
 * The height a description strip takes: one line (a type card's, as a block card's), or, in a Diagram
 * node `wrapWidth` wide, all of its text wrapped.
 */
export function descriptionHeight(description: Description | undefined, wrapWidth: number | null, measure: Measure): number {
  if (!description) {
    return 0;
  }
  const room = Math.max(1, (wrapWidth ?? 0) - BORDER - PADDING * 2);
  const lines =
    wrapWidth === null
      ? 1
      : description.text.split("\n").reduce((count, paragraph) => count + Math.max(1, Math.ceil(measure(paragraph, DESCRIPTION_FONT) / room)), 0);
  return DESCRIPTION_PADDING + RULE + lines * DESCRIPTION_LINE;
}

/** One row a type card lists, as text: the name as written and its written type (or value). */
export interface FieldText {
  readonly name: string;
  readonly type: string;
}

/** The rows a type card lists, as text (`fieldRows`). */
export function fieldTexts(definition: TypeDefinition): FieldText[] {
  return fieldRows(definition).map(({ member, type }) => ({
    name: member ? `${member.readonly ? "readonly " : ""}${memberName(member)}` : "=",
    type: `${type?.text ?? ""}${member ? (memberValue(member) ?? "") : ""}`,
  }));
}

/** A type card's width: its header (kind, name, location, switch) and its widest field, clamped to 280–560. */
export function typeCardWidth(header: Omit<NodeWidthInput, "lens">, fields: readonly FieldText[], measure: Measure): number {
  const head = nodeWidth({ ...header, lens: true }, measure);
  const name = Math.max(NAME_COLUMN, ...fields.map((field) => measure(field.name, CODE_FONT) + NAME_GAP));
  const type = Math.max(0, ...fields.map((field) => measure(field.type, CODE_FONT)));
  return clampCardWidth(Math.max(head, PADDING * 2 + BORDER + name + type), TYPE_MIN_WIDTH, TYPE_MAX_WIDTH);
}

/** How many of a type card's `count` rows it lists before "N more" (all when it has fewer than 12). */
export function shownFieldCount(count: number, all: boolean): number {
  return all || count < FOLD_FIELDS_AT ? count : FIELDS_FOLDED;
}

/** One entry a type card lists: a row (its index in `fieldRows`), or the "from <Type>" heading of base `base`. */
export type FieldEntry = { readonly kind: "row"; readonly index: number } | { readonly kind: "heading"; readonly base: number };

/**
 * What a type card lists, in order, and how many rows "N more" hides: its own rows, then each base's
 * heading and its members. A heading shows when a row after it does, and a base without members (no
 * declaration) when nothing is hidden.
 */
export function listedFields(definition: TypeDefinition, all: boolean): { readonly entries: FieldEntry[]; readonly hidden: number } {
  const total = fieldRows(definition).length;
  const shown = shownFieldCount(total, all);
  const entries: FieldEntry[] = [];
  let index = 0;
  const row = () => {
    if (index < shown) {
      entries.push({ kind: "row", index });
    }
    index += 1;
  };
  const own = (definition.aliased ? 1 : 0) + definition.members.length;
  for (let count = 0; count < own; count += 1) {
    row();
  }
  definition.inherited.forEach((group, base) => {
    if (index < shown || shown === total) {
      entries.push({ kind: "heading", base });
    }
    group.members.forEach(row);
  });
  return { entries, hidden: total - shown };
}

/** A type card's height before it is drawn: its header and description, then what it lists. */
export function typeCardHeight(definition: TypeDefinition, description: number): number {
  const { entries, hidden } = listedFields(definition, false);
  const rows = entries.filter((entry) => entry.kind === "row").length;
  return fieldsCardHeight(rows, entries.length - rows, hidden) + description;
}

/** A type card's height without a description: its header, `rows` rows, `headings` bases' headings, and "N more" when it hides some. */
export function fieldsCardHeight(rows: number, headings: number, hidden: number): number {
  return BLOCK_HEADER + FIELDS_PADDING + Math.max(1, rows) * FIELD_ROW + headings * FROM_HEADING + (hidden > 0 ? MORE_ROW : 0);
}

/**
 * Where the row of field `row` (a `fieldRows` id) is in a type card before it is drawn: its center below
 * the card's top, under a description `description` tall ("N more" for a field it hides); undefined for
 * a row the type doesn't have.
 */
export function estimatedFieldOffset(definition: TypeDefinition, row: string, description: number): number | undefined {
  const index = fieldRows(definition).findIndex((field) => field.row === row);
  if (index < 0) {
    return undefined;
  }
  const { entries } = listedFields(definition, false);
  let y = BLOCK_HEADER + description + FIELDS_TOP;
  for (const entry of entries) {
    if (entry.kind === "row" && entry.index === index) {
      return y + FIELD_ROW / 2;
    }
    y += entry.kind === "row" ? FIELD_ROW : FROM_HEADING;
  }
  return y + MORE_ROW / 2;
}

/** What estimating a card's size before it is drawn reads: its key, model, view, and what the page wrote about it. */
export interface EstimateInput {
  readonly key: Key;
  readonly model: CardView | undefined;
  readonly view: View;
  /** Whether the card is from another project (its header has the monogram). */
  readonly monogram: boolean;
  readonly description?: Description;
  /** A type card's definition, once loaded. */
  readonly definition?: TypeDefinition;
}

/** A card's size before it is drawn, from its model in the view it shows. */
export function estimatedCardSize(input: EstimateInput, measure: Measure): Size {
  const { key, view, monogram, description, definition } = input;
  const entrance = key.startsWith("entrance:");
  const body = cardBody(key, view);
  // A model of the other kind (an entrance's for a block key) is no model for this card.
  const model = input.model && (input.model.type === "entrance") === entrance ? input.model : undefined;
  if (!model) {
    return fallbackSize(entrance, body);
  }
  if (body === "node") {
    const header =
      model.type === "block"
        ? { mark: "ƒ", name: model.header.qualifiedName, location: nodeLocation(model.header.path, model.header.startLine), monogram, lens: true }
        : { mark: model.header.kind.toUpperCase(), name: entranceTitle(model), location: nodeLocation(model.evidence.path, model.evidence.startLine), monogram, lens: false };
    const width = nodeWidth(header, measure);
    return { width, height: BLOCK_HEADER + descriptionHeight(description, width, measure) };
  }
  if (model.type === "entrance") {
    return { width: entranceCardWidth(entranceCardText(model), measure), height: fallbackSize(true).height };
  }
  if (body === "fields") {
    const header = { mark: definition ? typeKindWord(definition.kind) : keyTypeWord(key), name: model.header.qualifiedName, location: nodeLocation(model.header.path, model.header.startLine), monogram };
    const width = typeCardWidth(header, definition ? fieldTexts(definition) : [], measure);
    return { width, height: definition ? typeCardHeight(definition, descriptionHeight(description, null, measure)) : fallbackSize(false, body).height };
  }
  return estimatedBlockSize(model, monogram, measure);
}

// ---------------------------------------------------------------------------------------------
// Collapsed frames

/** A collapsed frame's node sizes to its header between these widths; its description wraps inside. */
const FRAME_NODE_MIN_WIDTH = 240;
const FRAME_NODE_MAX_WIDTH = 400;
/** Its header's chevron, icon, and the agent mark's glyph with its gap (frames.css). */
const TOGGLE = 12;
const ICON = 16;
const MARK_GLYPH = 12;
/** Its body's block padding, the gap before what it holds, and that line. */
const FRAME_BODY_PADDING = 18;
const HOLDS_GAP = 4;
const HOLDS_LINE = 16;

/** A collapsed frame's node width: its chevron, icon, title, and agent mark, clamped to 240–400. */
export function frameNodeWidth(title: string, agent: boolean, measure: Measure): number {
  const mark = agent ? GAP + MARK_GLYPH + measure("agent", MARK_FONT) : 0;
  const header = PADDING * 2 + BORDER + TOGGLE + GAP + ICON + GAP + measure(title, FRAME_TITLE_FONT) + mark;
  return clampCardWidth(header, FRAME_NODE_MIN_WIDTH, FRAME_NODE_MAX_WIDTH);
}

/** A collapsed frame's node: its width, and its header, wrapped description, and what it holds. */
export function frameNodeSize(frame: Pick<Frame, "title" | "author" | "description">, measure: Measure): Size {
  const width = frameNodeWidth(frame.title, frame.author.startsWith("agent:"), measure);
  const room = Math.max(1, width - BORDER - PADDING * 2);
  const lines = (frame.description?.text.split("\n") ?? []).reduce((count, paragraph) => count + Math.max(1, Math.ceil(measure(paragraph, DESCRIPTION_FONT) / room)), 0);
  const header = HEADER_PADDING * 2 + TITLE_LINE + RULE;
  return { width, height: BORDER + header + FRAME_BODY_PADDING + lines * DESCRIPTION_LINE + (lines > 0 ? HOLDS_GAP : 0) + HOLDS_LINE };
}

// ---------------------------------------------------------------------------------------------
// Resource cards and notes

/** A resource card sizes to its header between these widths; a table, to its columns too. */
const RESOURCE_MIN_WIDTH = 200;
const RESOURCE_MAX_WIDTH = 360;
const TABLE_MIN_WIDTH = 240;
const TABLE_MAX_WIDTH = 520;
/** A resource card's header (no switch), and its columns' grid: block padding, a row, the gaps between columns, "N more". */
const RESOURCE_HEADER = HEADER_PADDING * 2 + TITLE_LINE + RULE;
const COLUMNS_PADDING = 18;
const COLUMN_ROW = 20;
const COLUMN_GAP = 8;
const COLUMN_NAME_MIN = 64;

/** What sizing a resource card reads: its kind's word, its name, who drew it, its description, and a table's columns. */
export interface ResourceSizeInput {
  readonly word: string;
  readonly name: string;
  readonly agent: boolean;
  readonly description?: Description;
  /** A table's columns (`tableColumns`); none for any other kind. */
  readonly columns: readonly TableColumn[];
}

/** A resource card's width: its header (glyph, kind's word, name, agent mark) and a table's widest column. */
export function resourceCardWidth(input: ResourceSizeInput, measure: Measure): number {
  const mark = input.agent ? GAP + MARK_GLYPH + measure("agent", MARK_FONT) : 0;
  const header = PADDING * 2 + BORDER + ICON + GAP + measure(input.word, MARK_FONT) + GAP + measure(input.name, TITLE_FONT) + mark;
  if (input.columns.length === 0) {
    return clampCardWidth(header, RESOURCE_MIN_WIDTH, RESOURCE_MAX_WIDTH);
  }
  const widest = (texts: readonly string[], min = 0) => Math.max(min, ...texts.map((text) => measure(text, CODE_FONT)));
  const name = widest(input.columns.map((column) => `${column.name}${column.optional ? "?" : ""}`), COLUMN_NAME_MIN) + COLUMN_GAP;
  const type = widest(input.columns.map(columnTypeText));
  const marks = Math.max(0, ...input.columns.map((column) => measure(markText(column), LOCATION_FONT)));
  const grid = PADDING * 2 + BORDER + name + type + (marks > 0 ? COLUMN_GAP + marks : 0);
  return clampCardWidth(Math.max(header, grid), TABLE_MIN_WIDTH, TABLE_MAX_WIDTH);
}

/** A resource card's size before it is drawn: its header, its whole description wrapped, and a table's listed columns. */
export function resourceCardSize(input: ResourceSizeInput, measure: Measure): Size {
  const width = resourceCardWidth(input, measure);
  const { shown, hidden } = listedColumns(input.columns, false);
  const columns = shown.length > 0 ? COLUMNS_PADDING + shown.length * COLUMN_ROW + (hidden > 0 ? MORE_ROW : 0) : 0;
  return { width, height: BORDER + RESOURCE_HEADER + descriptionHeight(input.description, width, measure) + columns };
}

/** A note sizes to its text between these widths. */
const NOTE_MIN_WIDTH = 140;
const NOTE_MAX_WIDTH = 260;
/** A note's padding (notes.css), the gap above its byline, and the byline's line. */
const NOTE_PADDING_X = 10;
const NOTE_PADDING_Y = 8;
const NOTE_BYLINE = 4 + 16;

/** A note's size: its text wrapped between 140 and 260 px, and its byline. */
export function noteSize(text: string, measure: Measure): Size {
  const longest = Math.max(0, ...text.split("\n").map((paragraph) => measure(paragraph, DESCRIPTION_FONT)));
  const width = clampCardWidth(BORDER + NOTE_PADDING_X * 2 + longest, NOTE_MIN_WIDTH, NOTE_MAX_WIDTH);
  const room = Math.max(1, width - BORDER - NOTE_PADDING_X * 2);
  const lines = text.split("\n").reduce((count, paragraph) => count + Math.max(1, Math.ceil(measure(paragraph, DESCRIPTION_FONT) / room)), 0);
  return { width, height: BORDER + NOTE_PADDING_Y * 2 + lines * DESCRIPTION_LINE + NOTE_BYLINE };
}
