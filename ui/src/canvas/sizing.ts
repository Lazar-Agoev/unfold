/**
 * Card widths: a compact card sizes to its header between 240 and 560 px; an expanded card to its
 * header, its signature, and its drawn flow (at least 280 px, and wider than 880 only for a flow that
 * needs it, so nothing is clipped); type cards (and compact classes) between 280 and 560 px; app and
 * entrance cards between 200 and 320 px; collapsed frames' nodes between 240 and 400 px; resource
 * cards between 200 and 360 px (tables 240 to 520); and notes between 140 and 260 px, measured with
 * the bundled fonts. Longer lines soft-wrap and nothing scrolls inside a card. Heights follow from the
 * content and are measured by the canvas; before a card is drawn, its size is estimated from what the
 * reader drew (an expanded card's flow laid out by `layoutFlow`), so the cards a step opens are placed
 * in that step. The constants follow cards.css and flow.css.
 */

import type { CardView, Description, Frame, Header, StartCard, TypeDefinition, View } from "../ipc/generated";
import { layoutFlow, type FlowGeometry, type Measure } from "../flow/layout";
import { memberName, memberValue } from "../code/typeKinds";
import { fieldRows } from "./arrows";
import { cardShape, headerLocation, headerMark, startRowName, startRowPlace, startRows, type CardShape, type HeaderMark } from "./cardText";
import { CARD_MAX_WIDTH, clampCardWidth, fallbackSize, type Size } from "./layout";
import {
  CODE_FONT,
  DESCRIPTION_FONT,
  DOC_FONT,
  FRAME_TITLE_FONT,
  LINKS_FONT,
  LOCATION_FONT,
  MARK_FONT,
  SIGNATURE_FONT,
  START_APP_FONT,
  START_NAME_FONT,
  START_ON_FONT,
  TITLE_FONT,
  TYPE_KIND_FONT,
} from "./measure";
import { columnTypeText, listedColumns, markText, type TableColumn } from "./resources/columns";

/** The card's border, left and right. */
const BORDER = 2;
/** The header's and body's inline padding, each side. */
const PADDING = 12;
/** Gaps between the header's parts. */
const GAP = 8;
/** The `ƒ` glyph. */
const GLYPH = 8;
/** The chevron at the start of a header: 16px less its -2px margin (cards.css `.card-expand`). */
const CHEVRON = 14;
/** A header (cards.css keeps it 43px with its rule, 44 with the card's border). */
const BLOCK_HEADER = 44;
/** A header's block padding (each side) and the rule under it. */
const HEADER_PADDING = 10;
const RULE = 1;
/** A header's line. */
const TITLE_LINE = 16;

/** The text of a definition card's header: its mark, name, location, and whether it has the chevron. */
export interface HeaderText {
  readonly mark: HeaderMark;
  readonly name: string;
  readonly location: string;
  readonly chevron: boolean;
}

/** A definition card's header text from its drawn flow's header. */
export function headerText(header: Header, chevron: boolean): HeaderText {
  return { mark: headerMark(header), name: header.name ?? header.label, location: headerLocation(header), chevron };
}

/** The width a header needs: padding, border, chevron, mark, name, and location. */
function headerWidth(header: HeaderText, measure: Measure): number {
  const mark = header.mark.word ? measure(header.mark.text, TYPE_KIND_FONT) : GLYPH;
  const text = mark + GAP + measure(header.name, TITLE_FONT) + GAP + measure(header.location, LOCATION_FONT);
  return Math.ceil(PADDING * 2 + BORDER + (header.chevron ? CHEVRON + GAP : 0) + text);
}

// ---------------------------------------------------------------------------------------------
// Definition cards

/** Compact cards size to their header between these widths; the description wraps inside. */
export const COMPACT_MIN_WIDTH = 240;
export const COMPACT_MAX_WIDTH = 560;
/** A description's line, and the strip's block padding. */
const DESCRIPTION_LINE = 18;
const DESCRIPTION_PADDING = 16;
/** What an expanded card says from its code: the section's padding (8 above, 9 below), its gap, and its lines. */
const CODE_SECTION_PADDING = 17;
const CODE_SECTION_GAP = 3;
const SIGNATURE_LINE = 17;
const DOC_LINE = 18;
const LINKS_LINE = 17;
/** The `FROM CODE` tag before the doc, and the label before a row of links, with their gaps. */
const SOURCE_TAG = 66;
const LINKS_LABEL = 120;

/** A compact card's width: its header, with its chevron, clamped to 240–560. */
export function compactCardWidth(header: HeaderText, measure: Measure): number {
  return clampCardWidth(headerWidth(header, measure), COMPACT_MIN_WIDTH, COMPACT_MAX_WIDTH);
}

/**
 * The height a description strip takes: one line (an expanded card's, a type card's), or, in a card
 * `wrapWidth` wide, all of its text wrapped (a compact card's, a resource card's).
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

/** The lines of `text` in `font` wrapped into `room`. */
function wrappedLines(text: string, font: string, room: number, measure: Measure): number {
  return Math.max(1, Math.ceil(measure(text, font) / Math.max(1, room)));
}

/** The words an expanded card's header section says from its code (each line its text), empty when it says nothing. */
function codeSectionLines(header: Header): { readonly font: string; readonly line: number; readonly text: string; readonly lead: number }[] {
  const imports = header.imports.map((item) => item.text).join(" · ");
  const definedWith = header.definedWith.map((call) => `${call.callee} ${call.role ?? ""}`).join(" · ");
  return [
    ...(header.signature !== undefined ? [{ font: SIGNATURE_FONT, line: SIGNATURE_LINE, text: header.signature, lead: 0 }] : []),
    ...(header.description !== undefined ? [{ font: DOC_FONT, line: DOC_LINE, text: header.description, lead: SOURCE_TAG }] : []),
    ...(imports.length > 0 ? [{ font: LINKS_FONT, line: LINKS_LINE, text: imports, lead: LINKS_LABEL }] : []),
    ...(definedWith.length > 0 ? [{ font: LINKS_FONT, line: LINKS_LINE, text: definedWith, lead: LINKS_LABEL }] : []),
  ];
}

/** The height of an expanded card's header section in a card `width` wide (0 when it says nothing). */
function codeSectionHeight(header: Header, width: number, measure: Measure): number {
  const lines = codeSectionLines(header);
  if (lines.length === 0) {
    return 0;
  }
  const room = width - BORDER - PADDING * 2;
  const text = lines.reduce((sum, item) => sum + wrappedLines(item.text, item.font, room - item.lead, measure) * item.line, 0);
  return CODE_SECTION_PADDING + RULE + (lines.length - 1) * CODE_SECTION_GAP + text;
}

/**
 * An expanded card's width: its header with the chevron, its signature (up to a card's widest), and its
 * drawn flow, at least 280; a flow wider than a card's widest widens it rather than being clipped.
 */
export function expandedCardWidth(header: Header, geometry: FlowGeometry, measure: Measure): number {
  const head = clampCardWidth(headerWidth(headerText(header, true), measure));
  const signature = header.signature !== undefined ? Math.min(CARD_MAX_WIDTH, Math.ceil(measure(header.signature, SIGNATURE_FONT) + PADDING * 2 + BORDER)) : 0;
  return Math.max(head, signature, Math.ceil(geometry.width + BORDER));
}

/** An expanded card's size: its header, one line of description, what it says from its code, and its drawn flow. */
export function expandedCardSize(header: Header, geometry: FlowGeometry, description: Description | undefined, measure: Measure): Size {
  const width = expandedCardWidth(header, geometry, measure);
  return {
    width,
    height: BLOCK_HEADER + descriptionHeight(description, null, measure) + codeSectionHeight(header, width, measure) + Math.ceil(geometry.height) + RULE,
  };
}

// ---------------------------------------------------------------------------------------------
// App and entrance cards

/** App and entrance cards size to their words and code rows between these widths. */
const START_MIN_WIDTH = 200;
const START_MAX_WIDTH = 320;
/** Their border (1.5px each side), padding, lines, and gaps (cards.css `.start-card`). */
const START_BORDER = 3;
const START_PADDING_X = 15;
const START_PADDING_Y = 13 + 14;
const START_GAP = 3;
const START_APP_LINE = 14;
const START_NAME_LINE = 20;
const START_ON_LINE = 16;
/** The app's small caps: 0.08em of letter spacing per character. */
const CAPS_TRACKING = 0.08 * 10;
/** The code rows under the words: the gap above them, a row, the gap between rows, and a dashed row's frame. */
const ROWS_TOP = 8;
const ROW = 20;
const ROW_GAP = 4;
const ROW_FRAME = 2 * 6 + 2;
const ABOUT_LINE = 18;

/** What an app's or entrance's card shows in `shape`: its words, its sentence (expanded), and its code rows. */
function startTexts(start: StartCard, shape: CardShape) {
  const rows = startRows(start, shape).map(({ code }) => [startRowName(code), startRowPlace(code) ?? ""].join("  "));
  return { about: shape === "expanded" ? (start.about ?? null) : null, rows };
}

/** An app's or entrance's card width: its small caps, name, what starts it, and its code rows, clamped to 200–320. */
export function startCardWidth(start: StartCard, shape: CardShape, measure: Measure): number {
  const { rows } = startTexts(start, shape);
  const words = Math.max(
    CHEVRON + GAP + measure(start.app.toUpperCase(), START_APP_FONT) + start.app.length * CAPS_TRACKING,
    measure(start.name, START_NAME_FONT),
    measure(start.on ?? "", START_ON_FONT),
    ...rows.map((row) => measure(row, CODE_FONT) + ROW_FRAME),
  );
  return clampCardWidth(words + START_PADDING_X * 2 + START_BORDER, START_MIN_WIDTH, START_MAX_WIDTH);
}

/** An app's or entrance's card size in `shape`. */
export function startCardSize(start: StartCard, shape: CardShape, measure: Measure): Size {
  const width = startCardWidth(start, shape, measure);
  const { about, rows } = startTexts(start, shape);
  const room = width - START_PADDING_X * 2 - START_BORDER;
  const words = START_APP_LINE + START_GAP + START_NAME_LINE + (start.on !== undefined ? START_GAP + START_ON_LINE : 0);
  const sentence = about !== null ? START_GAP + wrappedLines(about, DESCRIPTION_FONT, room, measure) * ABOUT_LINE : 0;
  const list = rows.length > 0 ? ROWS_TOP + rows.length * ROW + (rows.length - 1) * ROW_GAP : 0;
  return { width, height: START_BORDER + START_PADDING_Y + words + sentence + list };
}

// ---------------------------------------------------------------------------------------------
// Type cards

/** Type cards size to their header and their widest field between these widths. */
const TYPE_MIN_WIDTH = 280;
export const TYPE_MAX_WIDTH = 560;
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

/** A type card's width: its header (a class's chevron, kind, name, location) and its widest field, clamped to 280–560. */
export function typeCardWidth(header: HeaderText, fields: readonly FieldText[], measure: Measure): number {
  const head = headerWidth(header, measure);
  const name = Math.max(NAME_COLUMN, ...fields.map((field) => measure(field.name, CODE_FONT) + NAME_GAP));
  const type = Math.max(0, ...fields.map((field) => measure(field.type, CODE_FONT)));
  return clampCardWidth(Math.max(head, PADDING * 2 + BORDER + name + type), TYPE_MIN_WIDTH, TYPE_MAX_WIDTH);
}

/** How many of a type card's `count` rows it lists before "N more" (all when it has fewer than 12). */
function shownFieldCount(count: number, all: boolean): number {
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
  const headings = entries.length - rows;
  return BLOCK_HEADER + FIELDS_PADDING + Math.max(1, rows) * FIELD_ROW + headings * FROM_HEADING + (hidden > 0 ? MORE_ROW : 0) + description;
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

// ---------------------------------------------------------------------------------------------
// Any card

/** What estimating a card's size before it is drawn reads: what the reader drew, its view, and what the page wrote about it. */
export interface EstimateInput {
  readonly model: CardView | undefined;
  readonly view: View;
  readonly description?: Description;
  /** A type's (or a compact class's) definition, once loaded. */
  readonly definition?: TypeDefinition;
}

/** A card's size before it is drawn, from what the reader drew, in the view it shows. */
export function estimatedCardSize(input: EstimateInput, measure: Measure): Size {
  const { model, view, description, definition } = input;
  if (!model) {
    return fallbackSize(view);
  }
  const shape = cardShape(model, view);
  if (model.type === "start") {
    return startCardSize(model, shape, measure);
  }
  const { header } = model.flow;
  if (shape === "fields") {
    const text = headerText(header, header.kind === "class");
    const width = typeCardWidth(text, definition ? fieldTexts(definition) : [], measure);
    const listed = definition ? typeCardHeight(definition, descriptionHeight(description, null, measure)) : BLOCK_HEADER + FIELDS_PADDING + FIELD_ROW;
    return { width, height: listed };
  }
  if (shape === "compact") {
    const width = compactCardWidth(headerText(header, true), measure);
    return { width, height: BLOCK_HEADER + descriptionHeight(description, width, measure) };
  }
  return expandedCardSize(header, layoutFlow(model.flow, { measure }), description, measure);
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
