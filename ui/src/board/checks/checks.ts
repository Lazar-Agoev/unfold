/**
 * The page checks (design §7.7): the review checklist as data, so an agent that can't see images can
 * review a page too. They read the page's document and the page as the canvas laid it out, and find:
 *
 * - **Overlap**: cards, frames, resource cards, or notes on top of each other (a frame and what it
 *   holds don't count, nor a note and what it is attached to, nor a frame and a note on what it holds
 *   or on a line from or to what it holds);
 * - **Crossings**: lines crossing each other more than a few times, crossing a frame they neither
 *   start nor end in, or passing under a card they neither start nor end at;
 * - **Density**: too many visible items on the page or in a frame, or steps in an expanded card;
 * - **Missing words**: frames, combined steps, and resource cards without a description;
 * - **Evidence**: resource cards and connections that cite no code (an agent's are refused at save, a
 *   person's may lack them);
 * - **Strays**: cards and resource cards with no line, connection, or frame.
 *
 * A card's steps are counted from its drawn flow (`cards`, as the canvas drew them) when the card is
 * expanded; a compact card shows no steps to count. Cards are named by what they draw.
 *
 * Pure and deterministic: findings come in that order, items in page order. The limits are in
 * `thresholds.ts`.
 */

import type { PageCheck } from "@unfold/desktop/render-api";
import type { CardView, Connection, ConnectionKind, Page, Sequence } from "../../ipc/generated";
import { cardView } from "../../state/boards";
import { cardName } from "../../state/cards";
import { frameBox, frameTree, type FrameTree } from "./frames";
import { crossingOf, distance, overlapOf, segmentMeetsBox, type Box, type DrawnLine, type PageGeometry, type Point } from "./geometry";
import { THRESHOLDS } from "./thresholds";

/** Connection kinds that must cite code even when they have a step (the engine's rule for agents). */
const ANCHORED_KINDS: ReadonlySet<ConnectionKind> = new Set(["http", "queue", "calls"]);

/** What every check reads. */
interface Context {
  readonly page: Page;
  readonly geometry: PageGeometry;
  /** The cards as drawn, by card id. */
  readonly cards: ReadonlyMap<string, CardView>;
  readonly tree: FrameTree;
  /** Each item's position in the page's lists (cards, frames, resource cards, notes, connections, combined steps). */
  readonly order: ReadonlyMap<string, number>;
  /** The visible cards, frames, resource cards, and notes (none hidden in a collapsed frame), in page order. */
  readonly visible: readonly string[];
}

/** Runs every check on a laid-out page whose cards drew `cards` (by card id). */
export function pageChecks(page: Page, geometry: PageGeometry, cards: ReadonlyMap<string, CardView>): PageCheck[] {
  const tree = frameTree(page);
  const items = [...page.cards, ...(page.frames ?? []), ...(page.resources ?? []), ...(page.notes ?? [])].map((item) => item.id);
  const order = new Map([...items, ...page.links.map((link) => link.id), ...page.combined.map((combined) => combined.id)].map((id, index) => [id, index]));
  const context: Context = { page, geometry, cards, tree, order, visible: items.filter((id) => tree.hiddenBy(id) === undefined) };
  return [overlap, crossings, density, missingWords, evidence, strays].flatMap((check) => check(context));
}

// ---------------------------------------------------------------------------------------------
// Names

function quoted(text: string): string {
  return `"${text}"`;
}

/** An item's name for a message: a card's name (its id until it is drawn), `the frame "API"`, `the table "orders"`, … */
function nameOf(context: Pick<Context, "page" | "cards">, id: string): string {
  const { page } = context;
  if (page.cards.some((item) => item.id === id)) {
    const view = context.cards.get(id);
    return view ? cardName(view) : id;
  }
  const frame = page.frames?.find((item) => item.id === id);
  if (frame) {
    return `the frame ${quoted(frame.title)}`;
  }
  const resource = page.resources?.find((item) => item.id === id);
  if (resource) {
    return `the ${resource.kind} ${quoted(resource.name)}`;
  }
  const combined = page.combined.find((item) => item.id === id);
  if (combined) {
    return `the combined step ${quoted(combined.title)}`;
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    return linkName(context, link);
  }
  return page.notes?.some((item) => item.id === id) === true ? "a note" : id;
}

function linkName(context: Pick<Context, "page" | "cards">, link: Connection): string {
  const what = link.kind !== "other" ? `${link.kind} connection` : "line";
  return `the ${what} from ${nameOf(context, link.from)} to ${nameOf(context, link.to)}`;
}

/** A drawn line's name: its connection's, or the arrow's between its two items. */
function lineName(context: Pick<Context, "page" | "cards">, line: DrawnLine): string {
  const link = context.page.links.find((item) => item.id === line.id);
  return link ? linkName(context, link) : `the line from ${nameOf(context, line.from)} to ${nameOf(context, line.to)}`;
}

/** The page items a line stands for: its link, or the two items an arrow joins. */
function lineItems(page: Page, line: DrawnLine): string[] {
  return page.links.some((link) => link.id === line.id) ? [line.id] : [line.from, line.to];
}

/** `a, b, and c`, listing at most `THRESHOLDS.listedNames` and then "N more". */
function listed(names: readonly string[]): string {
  const shown = names.slice(0, THRESHOLDS.listedNames);
  const more = names.length - shown.length;
  const parts = more > 0 ? [...shown, `${more} more`] : shown;
  return parts.length <= 2 ? parts.join(" and ") : `${parts.slice(0, -1).join(", ")}, and ${parts.at(-1) ?? ""}`;
}

/** Ids once each, in page order. */
function inOrder(context: Context, ids: Iterable<string>): string[] {
  const rank = (id: string) => context.order.get(id) ?? Number.MAX_SAFE_INTEGER;
  return [...new Set(ids)].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

/** A name at the start of a sentence: "The frame …", "A note"; code names stay as written. */
function opening(name: string): string {
  return /^(the|a) /.test(name) ? `${name.charAt(0).toUpperCase()}${name.slice(1)}` : name;
}

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

// ---------------------------------------------------------------------------------------------
// Layout

/** Where a visible item is drawn: its box, or a frame's (as drawn, else around its members). */
function boxOf(context: Context, id: string): Box | undefined {
  return context.tree.frames.has(id) ? frameBox(context.tree, context.geometry.boxes, id) : context.geometry.boxes.get(id);
}

/**
 * Whether one item holds the other (a frame and an item in it), or a note is attached to the other, to
 * something a frame holds (the frame's box takes the note in), or to a line from or to something a
 * frame holds (the note sits by its line, which runs in the frame).
 */
function belongTogether(context: Context, a: string, b: string): boolean {
  const { tree, page } = context;
  const holds = (frame: string, item: string) => {
    const held = tree.inside(frame);
    const attach = page.notes?.find((entry) => entry.id === item)?.attach;
    const line = attach !== undefined ? page.links.find((link) => link.id === attach) : undefined;
    return held.has(item) || (attach !== undefined && held.has(attach)) || (line !== undefined && (held.has(line.from) || held.has(line.to)));
  };
  const attached = (note: string, item: string) => page.notes?.some((entry) => entry.id === note && entry.attach === item) === true;
  return holds(a, b) || holds(b, a) || attached(a, b) || attached(b, a);
}

function overlap(context: Context): PageCheck[] {
  const placed = context.visible.flatMap((id) => {
    const box = boxOf(context, id);
    return box ? [{ id, box }] : [];
  });
  const pairs: [string, string][] = [];
  placed.forEach((a, index) => {
    for (const b of placed.slice(index + 1)) {
      const { width, height } = overlapOf(a.box, b.box);
      if (width > THRESHOLDS.overlap && height > THRESHOLDS.overlap && !belongTogether(context, a.id, b.id)) {
        pairs.push([a.id, b.id]);
      }
    }
  });
  if (pairs.length === 0) {
    return [];
  }
  const names = pairs.map(([a, b]) => `${nameOf(context, a)} and ${nameOf(context, b)}`);
  return [
    {
      kind: "overlap",
      items: inOrder(context, pairs.flat()),
      message: `${pairs.length === 1 ? "Two items overlap" : `${pairs.length} pairs of items overlap`}: ${listed(names)}.`,
      suggestion: "Call arrange_page, or move them apart with its moves.",
    },
  ];
}

function nearEnd(point: Point, line: DrawnLine): boolean {
  const first = line.points[0];
  const last = line.points.at(-1);
  return (first !== undefined && distance(point, first) < THRESHOLDS.endSlack) || (last !== undefined && distance(point, last) < THRESHOLDS.endSlack);
}

/** How many times two lines cross, away from their ends. */
function crossingsBetween(a: DrawnLine, b: DrawnLine): number {
  let count = 0;
  for (let i = 1; i < a.points.length; i += 1) {
    for (let j = 1; j < b.points.length; j += 1) {
      const [p1, p2, q1, q2] = [a.points[i - 1], a.points[i], b.points[j - 1], b.points[j]];
      const at = p1 && p2 && q1 && q2 ? crossingOf(p1, p2, q1, q2) : null;
      if (at && !nearEnd(at, a) && !nearEnd(at, b)) {
        count += 1;
      }
    }
  }
  return count;
}

/** Whether a line passes through a box. */
function meets(line: DrawnLine, box: Box): boolean {
  return line.points.some((point, index) => {
    const next = line.points[index + 1];
    return next !== undefined && segmentMeetsBox(point, next, box);
  });
}

function crossings(context: Context): PageCheck[] {
  const { page, tree } = context;
  const lines = context.geometry.lines.filter((line) => line.points.length > 1);
  const findings: PageCheck[] = [];

  // Lines crossing lines: pairs that share an item fan out from it, so only other pairs count.
  const crossed: { a: DrawnLine; b: DrawnLine; times: number }[] = [];
  lines.forEach((a, index) => {
    for (const b of lines.slice(index + 1)) {
      const shared = a.from === b.from || a.from === b.to || a.to === b.from || a.to === b.to;
      const times = shared ? 0 : crossingsBetween(a, b);
      if (times > 0) {
        crossed.push({ a, b, times });
      }
    }
  });
  const total = crossed.reduce((sum, pair) => sum + pair.times, 0);
  if (total > THRESHOLDS.lineCrossings) {
    findings.push({
      kind: "crossings",
      items: inOrder(context, crossed.flatMap(({ a, b }) => [...lineItems(page, a), ...lineItems(page, b)])),
      message: `Lines cross ${plural(total, "time")}: ${listed(crossed.map(({ a, b }) => `${lineName(context, a)} crosses ${lineName(context, b)}`))}.`,
      suggestion: "Call arrange_page, or change a frame's direction with frame_cards, so most lines run one way without crossing.",
    });
  }

  // Lines through a frame they neither start nor end in.
  for (const frame of context.visible.filter((id) => tree.frames.has(id))) {
    const box = boxOf(context, frame);
    const within = new Set([frame, ...tree.inside(frame)]);
    const through = box ? lines.filter((line) => !within.has(line.from) && !within.has(line.to) && meets(line, box)) : [];
    if (through.length > 0) {
      findings.push({
        kind: "crossings",
        items: inOrder(context, [frame, ...through.flatMap((line) => lineItems(page, line))]),
        message: `${plural(through.length, "line")} cross${through.length === 1 ? "es" : ""} ${nameOf(context, frame)} without starting or ending in it: ${listed(through.map((line) => lineName(context, line)))}.`,
        suggestion: "Call arrange_page, or move the frame or the cards so the lines go around it.",
      });
    }
  }

  // Lines under a card, resource card, or collapsed frame they neither start nor end at (grazing its edge doesn't count).
  const solid = context.visible.flatMap((id) => {
    const frame = tree.frames.get(id);
    const box = (frame ? frame.collapsed === true : page.notes?.some((note) => note.id === id) !== true) ? context.geometry.boxes.get(id) : undefined;
    const inset = THRESHOLDS.overlap;
    return box ? [{ id, box: { x: box.x + inset, y: box.y + inset, width: box.width - 2 * inset, height: box.height - 2 * inset } }] : [];
  });
  const under = lines.flatMap((line) => {
    const covering = solid.filter(({ id, box }) => id !== line.from && id !== line.to && meets(line, box)).map(({ id }) => id);
    return covering.length > 0 ? [{ line, covering }] : [];
  });
  if (under.length > 0) {
    findings.push({
      kind: "crossings",
      items: inOrder(context, under.flatMap(({ line, covering }) => [...lineItems(page, line), ...covering])),
      message: `${plural(under.length, "line")} ${under.length === 1 ? "passes under a card it doesn't" : "pass under cards they don't"} start or end at: ${listed(under.map(({ line, covering }) => `${lineName(context, line)} passes under ${listed(covering.map((id) => nameOf(context, id)))}`))}.`,
      suggestion: "Call arrange_page, or move the cards off the line with its moves (rightOf, below), so the line can be seen end to end.",
    });
  }
  return findings;
}

// ---------------------------------------------------------------------------------------------
// Density and words

/** Whether an item counts as one visible item: a card, a resource card, or a collapsed frame. */
function countable(context: Context, id: string): boolean {
  const frame = context.tree.frames.get(id);
  return frame ? frame.collapsed === true : context.page.cards.some((card) => card.id === id) || context.page.resources?.some((item) => item.id === id) === true;
}

/** The steps a sequence draws, its frames' parts and open combined steps included; the calls opened in place are their own flows. */
function drawnSteps(sequence: Sequence): number {
  return sequence.steps.reduce((count, step) => {
    switch (step.kind) {
      case "decision":
      case "loop":
      case "try":
      case "with":
        return count + 1 + step.lanes.reduce((inner, lane) => inner + drawnSteps(lane.steps), 0);
      case "combined":
        return count + 1 + (step.steps ? drawnSteps(step.steps) : 0);
      default:
        return count + 1;
    }
  }, 0);
}

/** The steps an expanded card draws, by id; none for a compact card or a start card. */
function expandedSteps(context: Context, id: string): number | undefined {
  const card = context.page.cards.find((item) => item.id === id);
  const view = context.cards.get(id);
  return card && view?.type === "definition" && cardView(card, context.page.defaultView) === "expanded" ? drawnSteps(view.flow.steps) : undefined;
}

function density(context: Context): PageCheck[] {
  const { tree, visible } = context;
  const findings: PageCheck[] = [];
  const shown = visible.filter((id) => countable(context, id));
  if (shown.length > THRESHOLDS.pageItems) {
    findings.push({
      kind: "density",
      items: [],
      message: `The page shows ${plural(shown.length, "item")} (cards, resource cards, and collapsed frames); about ${THRESHOLDS.pageItems} is the most a page reads well with.`,
      suggestion: "Collapse frames the page isn't about (set_view), or split it: edit_pages adds a page.",
    });
  }
  for (const frame of visible.filter((id) => tree.frames.has(id) && tree.frames.get(id)?.collapsed !== true)) {
    const held = shown.filter((id) => tree.inside(frame).has(id));
    if (held.length > THRESHOLDS.frameItems) {
      findings.push({
        kind: "density",
        items: [frame],
        message: `${opening(nameOf(context, frame))} shows ${plural(held.length, "item")}; about ${THRESHOLDS.frameItems} is the most a frame reads well with.`,
        suggestion: "Nest frames inside it and collapse some (frame_cards, set_view), or move part of it to another page.",
      });
    }
  }
  for (const card of visible) {
    const steps = expandedSteps(context, card);
    if (steps !== undefined && steps > THRESHOLDS.flowSteps) {
      findings.push({
        kind: "density",
        items: [card],
        message: `${opening(nameOf(context, card))} shows ${plural(steps, "step")} expanded; about ${THRESHOLDS.flowSteps} is the most an expanded card reads well with.`,
        suggestion: "Combine its steps: 3 to 7 combined steps, collapsed for the ones the page isn't about; or set_view to compact.",
      });
    }
  }
  return findings;
}

function missingWords(context: Context): PageCheck[] {
  const { page } = context;
  const bare = [
    ...(page.frames ?? []).filter((frame) => frame.description === undefined).map((frame) => frame.id),
    ...page.combined.filter((combined) => combined.description === undefined).map((combined) => combined.id),
    ...(page.resources ?? []).filter((resource) => resource.description === undefined).map((resource) => resource.id),
  ];
  if (bare.length === 0) {
    return [];
  }
  const items = inOrder(context, bare);
  return [
    {
      kind: "missingWords",
      items,
      message: `No description on ${listed(items.map((id) => nameOf(context, id)))}.`,
      suggestion: "describe each in one sentence: what it is and why, not how.",
    },
  ];
}

/** Whether a connection cites code: anchors, or a step for the kinds a step grounds. */
function connectionGrounded(link: Connection): boolean {
  return (link.anchors ?? []).length > 0 || (link.step !== undefined && !ANCHORED_KINDS.has(link.kind));
}

function evidence(context: Context): PageCheck[] {
  const { page } = context;
  const bare = [
    ...(page.resources ?? []).filter((resource) => resource.kind !== "actor" && resource.anchors.length === 0).map((resource) => resource.id),
    ...page.links.filter((link) => !connectionGrounded(link)).map((link) => link.id),
  ];
  if (bare.length === 0) {
    return [];
  }
  const items = inOrder(context, bare);
  return [
    {
      kind: "evidence",
      items,
      message: `${opening(listed(items.map((id) => nameOf(context, id))))} cite${items.length === 1 ? "s" : ""} no code: nothing shows ${items.length === 1 ? "it is" : "they are"} true, and nothing says when ${items.length === 1 ? "it goes" : "they go"} out of date.`,
      suggestion: "Give each anchors (add_resources or connect with the same id, citing a block or an evidence file's lines); a person's are theirs to change, so ask first.",
    },
  ];
}

function strays(context: Context): PageCheck[] {
  const { page, tree } = context;
  const placed = context.visible.filter((id) => page.cards.some((card) => card.id === id) || page.resources?.some((item) => item.id === id) === true);
  if (placed.length < 2) {
    return [];
  }
  const joined = new Set([...context.geometry.lines, ...page.links].flatMap((line) => [line.from, line.to]));
  const alone = placed.filter((id) => !joined.has(id) && tree.parentOf(id) === undefined);
  if (alone.length === 0) {
    return [];
  }
  return [
    {
      kind: "strays",
      items: alone,
      message: `${opening(listed(alone.map((id) => nameOf(context, id))))} ${alone.length === 1 ? "has" : "have"} no line, connection, or frame to the rest of the page.`,
      suggestion: "connect them, put them in a frame (frame_cards), or take them off the page (remove_items).",
    },
  ];
}
