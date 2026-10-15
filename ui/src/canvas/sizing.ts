/**
 * Card widths: a compact card sizes to its header between 240 and 560 px; an expanded card to its
 * header, its signature, and its drawn flow (at least 280 px, and wider than 880 only for a flow that
 * needs it, so nothing is clipped); type cards (and compact classes) between 280 and 560 px; app and
 * entrance cards between 200 and 320 px (with a 250 to 320 px dashed card beside them for each code that
 * isn't found); collapsed frames' nodes between 240 and 400 px; resource
 * cards between 240 and 360 px (tables 240 to 520, actors from 140); and notes between 140 and 260 px, measured with
 * the bundled fonts. Longer lines soft-wrap and nothing scrolls inside a card. Heights follow from the
 * content and are measured by the canvas; before a card is drawn, its size is estimated from what the
 * reader drew (an expanded card's flow laid out by `layoutFlow`), so the cards a step opens are placed
 * in that step. The constants follow cards.css and flow.css.
 */

import { cardShape, fieldRows, type CardShape } from "@unfold/desktop/drawing";
import type { CardView, Description, Frame, Header, ResourceKind, StartCard, TypeDefinition, View } from "../ipc/generated";
import { layoutFlow, type FlowGeometry, type Measure, type Point as FlowPoint, type Rect as FlowRect } from "../flow/layout";
import { memberName, memberValue } from "../code/typeKinds";
import { callPort, stepPort } from "./arrows";
import { headerLocation, headerMark, startRowName, startRowPlace, startRows, unfoundCode, type HeaderMark } from "./cardText";
import { CARD_MAX_WIDTH, clampCardWidth, fallbackSize, type Size } from "./layout";
import {
  AUTHORED_TITLE_FONT,
  CODE_FONT,
  ACTOR_NAME_FONT,
  CODE_NAME_FONT,
  COLUMN_NAME_FONT,
  COLUMN_TYPE_FONT,
  DESCRIPTION_FONT,
  DOC_FONT,
  FRAME_COUNT_FONT,
  FRAME_TITLE_FONT,
  LANE_TYPE_FONT,
  LINKS_FONT,
  LOCATION_FONT,
  RESOURCE_KIND_FONT,
  RESOURCE_NAME_FONT,
  RESOURCE_PROSE_FONT,
  SMALL_PROSE_FONT,
  SIGNATURE_FONT,
  START_APP_FONT,
  START_NAME_FONT,
  START_ON_FONT,
  TABLE_NAME_FONT,
  TABLE_PROSE_FONT,
  TAG_FONT,
  TITLE_FONT,
  TYPE_KIND_FONT,
} from "./measure";
import { columnTypeText, listedColumns, type TableColumn } from "./resources/columns";

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
/** The header's `</>` at its end (codeButton.css): three 6px characters, its padding, and its border. */
const CODE_BUTTON = 30;
/** A header (cards.css keeps it 43px with its rule, 44 with the card's border). */
const BLOCK_HEADER = 44;
/** The rule under a header. */
const RULE = 1;

/**
 * The text of a definition card's header: its mark, its title, the code name beside a title the page
 * gave it, its `ASYNC` tag, location, and whether it has the chevron.
 */
export interface HeaderText {
  readonly mark: HeaderMark;
  /** What titles the card: the page's name for its definition, else its code name. */
  readonly name: string;
  /** The code name, shown after `name` when the page's name for the definition differs from it; absent otherwise. */
  readonly code?: string;
  readonly async: boolean;
  readonly location: string;
  readonly chevron: boolean;
}

/** A definition card's header text from its drawn flow's header. */
export function headerText(header: Header, chevron: boolean): HeaderText {
  const named = header.name !== undefined && header.name !== header.label;
  return { mark: headerMark(header), name: header.name ?? header.label, ...(named ? { code: header.label } : {}), async: header.async, location: headerLocation(header), chevron };
}

/** A header's tag (cards.css `.card-tag`): its small caps with 0.06em of letter spacing, its padding, and its border. */
function tagWidth(word: string, measure: Measure): number {
  return measure(word.toUpperCase(), TAG_FONT) + word.length * 0.06 * 9 + 4 * 2 + 2;
}

/** The width a header's title needs: the code name in the code font, or a title the page gave it in the UI font with the code name after it. */
function titleWidth(header: HeaderText, measure: Measure): number {
  return header.code !== undefined ? measure(header.name, AUTHORED_TITLE_FONT) + GAP + measure(header.code, CODE_NAME_FONT) : measure(header.name, TITLE_FONT);
}

/** The width a header needs: padding, border, chevron, mark, title (and the code name beside one the page gave it), `ASYNC`, location, and `</>`. */
function headerWidth(header: HeaderText, measure: Measure): number {
  const mark = header.mark.word ? measure(header.mark.text, TYPE_KIND_FONT) : GLYPH;
  const tag = header.async ? GAP + tagWidth("async", measure) : 0;
  const text = mark + GAP + titleWidth(header, measure) + tag + GAP + measure(header.location, LOCATION_FONT) + GAP + CODE_BUTTON;
  return Math.ceil(PADDING * 2 + BORDER + (header.chevron ? CHEVRON + GAP : 0) + text);
}

/** How much wider a header's `ƒ` gets when its name is at its biggest (cards.css `.card-glyph`). */
const GLYPH_GROWTH = 1.8;

/**
 * How much a header's title may grow as the page zooms out (zoom.ts `--name-grow`) and still fit the
 * header: the room left once its location has given way, over the title's width at its own size.
 */
export function nameFit(header: HeaderText, width: number, measure: Measure): number {
  const mark = header.mark.word ? measure(header.mark.text, TYPE_KIND_FONT) : GLYPH * GLYPH_GROWTH;
  const tag = header.async ? GAP + tagWidth("async", measure) : 0;
  const code = header.code !== undefined ? GAP + measure(header.code, CODE_NAME_FONT) : 0;
  const room = width - (PADDING * 2 + BORDER + (header.chevron ? CHEVRON + GAP : 0) + mark + GAP + tag + code + GAP + GAP + CODE_BUTTON);
  const name = measure(header.name, header.code !== undefined ? AUTHORED_TITLE_FONT : TITLE_FONT);
  return Math.max(1, room / Math.max(1, name));
}

// ---------------------------------------------------------------------------------------------
// Definition cards

/** Compact cards size to their header between these widths; the description wraps inside. */
export const COMPACT_MIN_WIDTH = 240;
export const COMPACT_MAX_WIDTH = 560;
/** A description's line, and the strip's block padding (10 above, 12 below). */
const DESCRIPTION_LINE = 20;
const DESCRIPTION_PADDING = 22;
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
 * The height a description strip takes: one line (a type card's), or, in a card `wrapWidth` wide, all
 * of its text wrapped (a definition's or a resource card's).
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

/** Where an expanded card `width` wide draws its flow, below its top: under its header, its description, and what it says from its code. */
function expandedBodyTop(header: Header, width: number, description: Description | undefined, measure: Measure): number {
  return BLOCK_HEADER + descriptionHeight(description, width, measure) + codeSectionHeight(header, width, measure);
}

/** An expanded card's size: its header, its description, what it says from its code, and its drawn flow. */
export function expandedCardSize(header: Header, geometry: FlowGeometry, description: Description | undefined, measure: Measure): Size {
  const width = expandedCardWidth(header, geometry, measure);
  return { width, height: expandedBodyTop(header, width, description, measure) + Math.ceil(geometry.height) + RULE };
}

/** The right edge of an expanded card `width` wide's body, from its left: its lines leave the card there. */
export function bodyRight(width: number): number {
  return width - BORDER;
}

/** Where a line leaves an expanded card's drawn flow, and its part inside the card. */
export interface FlowPortSpot {
  /** Its height below the flow's top where it leaves the card. */
  readonly y: number;
  /** Its path inside the card, in the flow's coordinates: from the right side of its box to the body's right edge. */
  readonly path: readonly FlowPoint[];
}

/** How far left of a box beside a row a line turns down, and how far it keeps from the boxes it passes. */
const PORT_TURN = 12;
const PORT_CLEAR = 4;

function holds(outer: FlowRect, inner: FlowRect): boolean {
  return outer.x <= inner.x && outer.y <= inner.y && outer.x + outer.width >= inner.x + inner.width && outer.y + outer.height >= inner.y + inner.height;
}

/**
 * A line's path inside a card from `start` (the right side of `box`, its row's height) to `right`:
 * straight along its row; under a box beside the row (a try's handlers beside the try), it turns down
 * just before that box, runs in the gap under it, and leaves the card there.
 */
function portPath(start: FlowPoint, box: FlowRect, geometry: FlowGeometry, right: number): FlowPoint[] {
  const boxes = geometry.pieces.filter((piece) => piece.kind !== "note" && !holds(piece.rect, box) && !holds(box, piece.rect)).map((piece) => piece.rect);
  const across = (y: number, from: number) => boxes.filter((rect) => rect.x + rect.width > from && rect.y - PORT_CLEAR < y && y < rect.y + rect.height + PORT_CLEAR);
  let hits = across(start.y, start.x);
  if (hits.length === 0) {
    return [start, { x: right, y: start.y }];
  }
  const beside = Math.min(...hits.map((rect) => rect.x));
  const turn = beside - Math.min(PORT_TURN, Math.max(0, beside - start.x) / 2);
  let y = start.y;
  for (let tries = 0; hits.length > 0 && tries < geometry.pieces.length; tries += 1) {
    const bottom = Math.max(...hits.map((rect) => rect.y + rect.height));
    const next = Math.min(...boxes.filter((rect) => rect.x + rect.width > turn && rect.y >= bottom).map((rect) => rect.y), bottom + 4 * PORT_CLEAR);
    y = (bottom + next) / 2;
    hits = across(y, turn);
  }
  return [start, { x: turn, y: start.y }, { x: turn, y }, { x: right, y }];
}

/**
 * Where an expanded card's drawn flow places its ports, and the part of each line inside the card,
 * up to `right` (`bodyRight`): a call's (`callPort`) and a connection's from that call (`stepPort`)
 * leave the right side of the call's box (or of the collapsed combined step holding it) at its row;
 * a connection's from a step that isn't a call leaves the block's right side at its first line. A line
 * leaves the card at its row, or under a box beside the row (`portPath`).
 */
export function flowPortSpots(geometry: FlowGeometry, right: number): Map<string, FlowPortSpot> {
  const spots = new Map<string, FlowPortSpot>();
  const spot = (box: FlowRect, start: FlowPoint): FlowPortSpot => {
    const path = portPath(start, box, geometry, right);
    return { y: path[path.length - 1]?.y ?? start.y, path };
  };
  for (const anchor of geometry.anchors) {
    for (const port of [callPort(anchor.call), stepPort(anchor.call)]) {
      if (!spots.has(port)) {
        spots.set(port, spot(anchor.rect, anchor.at));
      }
    }
  }
  for (const [key, rect] of geometry.blocks) {
    if (!spots.has(stepPort(key))) {
      spots.set(stepPort(key), spot(rect, { x: rect.x + rect.width, y: rect.y + Math.min(rect.height / 2, 13) }));
    }
  }
  return spots;
}

/** Each port of an expanded card below its top, before it is drawn: where it leaves the card (`flowPortSpots`), under the flow's top. */
export function estimatedPortOffsets(header: Header, geometry: FlowGeometry, description: Description | undefined, measure: Measure): Map<string, number> {
  const width = expandedCardWidth(header, geometry, measure);
  const top = expandedBodyTop(header, width, description, measure);
  return new Map([...flowPortSpots(geometry, bodyRight(width))].map(([port, { y }]) => [port, top + y]));
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
/** The code rows under the words (expanded): the gap above them, a row, and the gap between rows. */
const ROWS_TOP = 8;
const ROW = 20;
const ROW_GAP = 4;
const ABOUT_LINE = 18;
/**
 * Code that isn't found, beside the card (cards.css `.start-unfound`): the arrow's length, the gap
 * between cards, their widths, and their parts: the header ("?" and "Code not found") with its rule,
 * the padding around the key, a key's line, and the lines under it (what happened, and its fix).
 */
const UNFOUND_LINK = 56;
const UNFOUND_GAP = 16;
const UNFOUND_MIN_WIDTH = 250;
const UNFOUND_MAX_WIDTH = 320;
const UNFOUND_BORDER = 2;
const UNFOUND_HEADER = 11 + 20 + 11 + 1;
const UNFOUND_HEADER_TEXT = 20 + 8 + 14 * 2;
const UNFOUND_PADDING_X = 18;
const UNFOUND_PADDING_Y = 16 + 18;
const UNFOUND_KEY_LINE = 16;
const UNFOUND_LINES = 2 * (4 + 20);

/** What an app's or entrance's card shows in `shape`: its words, its sentence (expanded), and its code rows. */
function startTexts(start: StartCard, shape: CardShape) {
  const rows = startRows(start, shape).map(({ code }) => [startRowName(code), startRowPlace(code) ?? ""].join("  "));
  return { about: shape === "expanded" ? (start.about ?? null) : null, rows };
}

/** An app's or entrance's card width (the card itself): its small caps (an entrance's), name, what starts it, and its code rows, clamped to 200–320. */
export function startCardWidth(start: StartCard, shape: CardShape, measure: Measure): number {
  const { rows } = startTexts(start, shape);
  const entrance = start.kind === "entrance";
  const words = Math.max(
    entrance ? CHEVRON + GAP + measure(start.app.toUpperCase(), START_APP_FONT) + start.app.length * CAPS_TRACKING : 0,
    (entrance ? 0 : CHEVRON + GAP) + measure(start.name, START_NAME_FONT),
    measure(start.on ?? "", START_ON_FONT),
    ...rows.map((row) => measure(row, CODE_FONT)),
  );
  return clampCardWidth(words + START_PADDING_X * 2 + START_BORDER, START_MIN_WIDTH, START_MAX_WIDTH);
}

/** The size of the dashed card for code that isn't found: its header, and its key as written, which wraps. */
function unfoundCardSize(key: string, measure: Measure): Size {
  const width = clampCardWidth(Math.max(UNFOUND_HEADER_TEXT + measure("Code not found", TITLE_FONT), measure(key, LOCATION_FONT) + UNFOUND_PADDING_X * 2) + UNFOUND_BORDER, UNFOUND_MIN_WIDTH, UNFOUND_MAX_WIDTH);
  const keyLines = wrappedLines(key, LOCATION_FONT, width - UNFOUND_PADDING_X * 2 - UNFOUND_BORDER, measure);
  return { width, height: UNFOUND_BORDER + UNFOUND_HEADER + UNFOUND_PADDING_Y + keyLines * UNFOUND_KEY_LINE + UNFOUND_LINES };
}

/** An app's or entrance's card size in `shape`, with the dashed card of each code that isn't found beside it. */
export function startCardSize(start: StartCard, shape: CardShape, measure: Measure): Size {
  const width = startCardWidth(start, shape, measure);
  const { about, rows } = startTexts(start, shape);
  const room = width - START_PADDING_X * 2 - START_BORDER;
  const words = (start.kind === "entrance" ? START_APP_LINE + START_GAP : 0) + START_NAME_LINE + (start.on !== undefined ? START_GAP + START_ON_LINE : 0);
  const sentence = about !== null ? START_GAP + wrappedLines(about, SMALL_PROSE_FONT, room, measure) * ABOUT_LINE : 0;
  const list = rows.length > 0 ? ROWS_TOP + rows.length * ROW + (rows.length - 1) * ROW_GAP : 0;
  const height = START_BORDER + START_PADDING_Y + words + sentence + list;
  const unfound = unfoundCode(start).map(({ code }) => unfoundCardSize(code.key, measure));
  if (unfound.length === 0) {
    return { width, height };
  }
  return {
    width: width + UNFOUND_LINK + Math.max(...unfound.map((card) => card.width)),
    height: Math.max(height, unfound.reduce((sum, card) => sum + card.height, 0) + (unfound.length - 1) * UNFOUND_GAP),
  };
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

/** A collapsed frame's node sizes to its title between these widths; its description wraps inside. */
const FRAME_NODE_MIN_WIDTH = 240;
const FRAME_NODE_MAX_WIDTH = 400;
/**
 * A collapsed frame's node (frames.css `.frame-node`): 12px by 14px of padding, its title's 18px line
 * with the chevron (shown while hovered) and the count of the cards it holds after it, then its
 * description's 18px lines 2px under it.
 */
const FRAME_NODE_PADDING_X = 14;
const FRAME_NODE_PADDING_Y = 12 * 2;
const FRAME_NODE_LINE = 18;
const FRAME_NODE_GAP = 2;
const TOGGLE = 12;
/** The count's widest likely text, so the node needn't know it to be sized. */
const COUNT_SAMPLE = "00 cards";

/** A collapsed frame's node width: its title, chevron, and count, clamped to 240–400. */
export function frameNodeWidth(title: string, measure: Measure): number {
  const header = FRAME_NODE_PADDING_X * 2 + BORDER + measure(title, FRAME_TITLE_FONT) + GAP + TOGGLE + GAP + measure(COUNT_SAMPLE, FRAME_COUNT_FONT);
  return clampCardWidth(header, FRAME_NODE_MIN_WIDTH, FRAME_NODE_MAX_WIDTH);
}

/** A collapsed frame's node: its width, and its title line and wrapped description. */
export function frameNodeSize(frame: Pick<Frame, "title" | "description">, measure: Measure): Size {
  const width = frameNodeWidth(frame.title, measure);
  const room = Math.max(1, width - BORDER - FRAME_NODE_PADDING_X * 2);
  const text = frame.description?.text;
  const about = text !== undefined ? FRAME_NODE_GAP + text.split("\n").reduce((count, paragraph) => count + wrappedLines(paragraph, SMALL_PROSE_FONT, room, measure), 0) * FRAME_NODE_LINE : 0;
  return { width, height: BORDER + FRAME_NODE_PADDING_Y + FRAME_NODE_LINE + about };
}

// ---------------------------------------------------------------------------------------------
// Resource cards and notes

/** A resource card sizes to its words between these widths; a table, to its columns too. */
const RESOURCE_MIN_WIDTH = 240;
const RESOURCE_MAX_WIDTH = 360;
const TABLE_MIN_WIDTH = 240;
const TABLE_MAX_WIDTH = 520;
/**
 * A queue's, cache's, storage's, or outside service's card (resources.css): 14px of padding each
 * side, the 32px glyph tile and the 12px gap after it, then the kind's small caps (a 14px line) over
 * the name (a 20px line), 1px apart. The header has 12px above and 8 below (12 with nothing under it);
 * the description's 18px lines have 12px under them (10 before a queue's lane).
 */
const TILE_PADDING = 14;
const TILE = 32 + 12;
const TILE_HEAD = 12 + 14 + 1 + 20;
const TILE_LINE = 18;
/** An outside service's dashed border is 1.5px each side. */
const DASHED_BORDER = 3;
/** A queue's lane: its rule, its padding (9 above, 10 below), the 16px row of slots, and the slots, arrow, and gaps before the type. */
const LANE = 1 + 9 + 16 + 10;
const LANE_LEAD = 3 * 14 + 4 * 6 + 16 + 4;
/** A table: its 44px name band (with its rule), the glyph and gap before the name, its description's 16px lines with 8px above and below, its 28px rows, and the 22px key lane and the gap before a row's type. */
const BAND = 44;
const BAND_LEAD = 16 + 10;
const TABLE_LINE = 16;
const TABLE_ABOUT_PADDING = 16;
const COLUMN_ROW = 28;
const KEY_LANE = 22;
const TYPE_GAP = 16;
/** An actor's pill: 8px of padding around its 36px avatar (20 after its words), the 12px gap, its border; the description 10px under it, inset 8. */
const PILL = 8 + 36 + 8 + BORDER;
const PILL_LEAD = 8 + 36 + 12;
const PILL_TAIL = 20;
const ACTOR_GAP = 10;
const ACTOR_INSET = 8;
/** The least width an actor's description wraps in, and the least an actor takes. */
const ACTOR_PROSE_WIDTH = 228;
const ACTOR_MIN_WIDTH = 140;
/** The small caps' letter spacing: 0.08em of 10px per character. */
const KIND_TRACKING = 0.08 * 10;

/** What sizing a resource card reads: its kind, its name, its description, a table's columns, and the type a queue carries. */
export interface ResourceSizeInput {
  readonly kind: ResourceKind;
  readonly name: string;
  readonly description?: Description;
  /** A table's columns (`tableColumns`); none for any other kind. */
  readonly columns: readonly TableColumn[];
  /** The name of the type a queue carries, when the page says. */
  readonly carries?: string;
}

/** The kind's word in small caps (resources.css `.rcard-kind`). */
function kindWidth(kind: ResourceKind, measure: Measure): number {
  return measure(kind.toUpperCase(), RESOURCE_KIND_FONT) + kind.length * KIND_TRACKING;
}

/** How many lines `text` wraps to in `room` in `font`, paragraph by paragraph. */
function proseLines(text: string, font: string, room: number, measure: Measure): number {
  return text.split("\n").reduce((count, paragraph) => count + wrappedLines(paragraph, font, room, measure), 0);
}

/** A resource card's width: its words (and a table's columns, a queue's lane), clamped to its kind's widths. */
export function resourceCardWidth(input: ResourceSizeInput, measure: Measure): number {
  if (input.kind === "table") {
    const band = PADDING * 2 + BORDER + BAND_LEAD + measure(input.name, TABLE_NAME_FONT);
    const names = Math.max(0, ...input.columns.map((column) => measure(`${column.name}${column.optional ? "?" : ""}`, COLUMN_NAME_FONT)));
    const types = Math.max(0, ...input.columns.map((column) => measure(columnTypeText(column), COLUMN_TYPE_FONT)));
    const rows = input.columns.length > 0 ? PADDING * 2 + BORDER + KEY_LANE + names + TYPE_GAP + types : 0;
    return clampCardWidth(Math.max(band, rows), TABLE_MIN_WIDTH, TABLE_MAX_WIDTH);
  }
  const words = Math.max(kindWidth(input.kind, measure), measure(input.name, input.kind === "actor" ? ACTOR_NAME_FONT : RESOURCE_NAME_FONT));
  if (input.kind === "actor") {
    const pill = PILL_LEAD + words + PILL_TAIL + BORDER;
    return clampCardWidth(Math.max(pill, input.description ? ACTOR_PROSE_WIDTH : 0), ACTOR_MIN_WIDTH, RESOURCE_MAX_WIDTH);
  }
  const border = input.kind === "external" ? DASHED_BORDER : BORDER;
  const lane = input.kind === "queue" && input.carries !== undefined ? LANE_LEAD + measure(input.carries, LANE_TYPE_FONT) : 0;
  return clampCardWidth(TILE_PADDING * 2 + border + Math.max(TILE + words, lane), RESOURCE_MIN_WIDTH, RESOURCE_MAX_WIDTH);
}

/** How much a resource card's name may grow as the page zooms out and still fit its card `width` wide (as `nameFit`). */
export function resourceNameFit(input: Pick<ResourceSizeInput, "kind" | "name">, width: number, measure: Measure): number {
  if (input.kind === "table") {
    return Math.max(1, (width - PADDING * 2 - BORDER - BAND_LEAD) / Math.max(1, measure(input.name, TABLE_NAME_FONT)));
  }
  const room = input.kind === "actor" ? width - PILL_LEAD - PILL_TAIL - BORDER : width - TILE_PADDING * 2 - BORDER - TILE;
  return Math.max(1, room / Math.max(1, measure(input.name, input.kind === "actor" ? ACTOR_NAME_FONT : RESOURCE_NAME_FONT)));
}

/** A resource card's size before it is drawn: its header, its whole description wrapped, a table's listed columns, a queue's lane. */
export function resourceCardSize(input: ResourceSizeInput, measure: Measure): Size {
  const width = resourceCardWidth(input, measure);
  const text = input.description?.text;
  if (input.kind === "table") {
    const about = text !== undefined ? TABLE_ABOUT_PADDING + proseLines(text, TABLE_PROSE_FONT, width - BORDER - PADDING * 2, measure) * TABLE_LINE : 0;
    const { shown, hidden } = listedColumns(input.columns, false);
    return { width, height: BORDER + BAND + about + (shown.length + (hidden > 0 ? 1 : 0)) * COLUMN_ROW };
  }
  if (input.kind === "actor") {
    const about = text !== undefined ? ACTOR_GAP + proseLines(text, RESOURCE_PROSE_FONT, width - ACTOR_INSET, measure) * TILE_LINE : 0;
    return { width, height: PILL + about };
  }
  const border = input.kind === "external" ? DASHED_BORDER : BORDER;
  const queue = input.kind === "queue";
  const about = text !== undefined ? proseLines(text, RESOURCE_PROSE_FONT, width - border - TILE_PADDING * 2, measure) * TILE_LINE + (queue ? 10 : 12) : 0;
  const head = TILE_HEAD + (text !== undefined || queue ? 8 : 12);
  return { width, height: border + head + about + (queue ? LANE : 0) };
}

/** A note sizes to its text between these widths. */
const NOTE_MIN_WIDTH = 140;
const NOTE_MAX_WIDTH = 260;
/** A note's padding (notes.css: 12 above, 10 below, 14 each side; no border), the gap above its byline, and the byline's line. */
const NOTE_PADDING_X = 14;
const NOTE_PADDING_Y = 12 + 10;
const NOTE_LINE = 20;
const NOTE_BYLINE = 8 + 16;

/** A note's size: its text wrapped between 140 and 260 px, and its byline. */
export function noteSize(text: string, measure: Measure): Size {
  const longest = Math.max(0, ...text.split("\n").map((paragraph) => measure(paragraph, DESCRIPTION_FONT)));
  const width = clampCardWidth(NOTE_PADDING_X * 2 + longest, NOTE_MIN_WIDTH, NOTE_MAX_WIDTH);
  const room = Math.max(1, width - NOTE_PADDING_X * 2);
  const lines = text.split("\n").reduce((count, paragraph) => count + Math.max(1, Math.ceil(measure(paragraph, DESCRIPTION_FONT) / room)), 0);
  return { width, height: NOTE_PADDING_Y + lines * NOTE_LINE + NOTE_BYLINE };
}
