/**
 * The page checks (design §7.7): the review checklist as data, so an agent that can't see images can
 * review a page too. They read the page's document and the page as the canvas laid it out, and find:
 *
 * - **Overlap**: cards, frames, resource cards, or notes on top of each other (a frame and what it
 *   holds don't count, nor a note and what it is attached to, nor a frame and a note on what it holds);
 * - **Crossings**: lines crossing each other more than a few times, or crossing a frame they neither
 *   start nor end in;
 * - **Density**: too many visible items on the page or in a frame, or steps in a Flow card;
 * - **Unlabeled steps**: Flow cards whose steps still show names from the code;
 * - **Missing words**: frames, step groups, and resource cards without a description;
 * - **Evidence**: resource cards and connections that cite no code (an agent's are refused at save, a
 *   person's may lack them);
 * - **Strays**: cards and resource cards with no line, connection, or frame;
 * - **Views**: many Flow or Code cards on an overview page.
 *
 * Pure and deterministic: findings come in that order, items in page order. The limits are in
 * `thresholds.ts`.
 */

import type { PageCheck } from "@unfold/desktop/render-api";
import type { Link, Page } from "../../ipc/generated";
import { cardBody } from "../../code/typeKinds";
import { cardKey, keyDisplayName } from "../../state/boards";
import { frameBox, frameTree, type FrameTree } from "./frames";
import { crossingOf, distance, overlapOf, segmentMeetsBox, type Box, type DrawnLine, type PageGeometry, type Point } from "./geometry";
import { THRESHOLDS } from "./thresholds";

/** Connection kinds that must cite code even when they have a step (the engine's rule for agents). */
const ANCHORED_KINDS: ReadonlySet<string> = new Set(["http", "rpc", "queue", "event", "calls"]);

/** What every check reads. */
interface Context {
  readonly page: Page;
  readonly geometry: PageGeometry;
  readonly tree: FrameTree;
  /** Each item's position in the page's lists (cards, frames, resource cards, notes, links, step groups). */
  readonly order: ReadonlyMap<string, number>;
  /** The visible cards, frames, resource cards, and notes (none hidden in a collapsed frame), in page order. */
  readonly visible: readonly string[];
}

/** Runs every check on a laid-out page. */
export function pageChecks(page: Page, geometry: PageGeometry): PageCheck[] {
  const tree = frameTree(page);
  const items = [...page.cards, ...(page.frames ?? []), ...(page.resources ?? []), ...(page.notes ?? [])].map((item) => item.id);
  const order = new Map([...items, ...page.links.map((link) => link.id), ...page.groups.map((group) => group.id)].map((id, index) => [id, index]));
  const context: Context = { page, geometry, tree, order, visible: items.filter((id) => tree.hiddenBy(id) === undefined) };
  return [overlap, crossings, density, unlabeled, missingWords, evidence, strays, views].flatMap((check) => check(context));
}

// ---------------------------------------------------------------------------------------------
// Names

function quoted(text: string): string {
  return `"${text}"`;
}

/** An item's name for a message: a card's name, `the frame "API"`, `the table "orders"`, … */
function nameOf(page: Page, id: string): string {
  const card = page.cards.find((item) => item.id === id);
  if (card) {
    return keyDisplayName(cardKey(card));
  }
  const frame = page.frames?.find((item) => item.id === id);
  if (frame) {
    return `the frame ${quoted(frame.title)}`;
  }
  const resource = page.resources?.find((item) => item.id === id);
  if (resource) {
    return `the ${resource.kind} ${quoted(resource.name)}`;
  }
  const group = page.groups.find((item) => item.id === id);
  if (group) {
    return `the step group ${quoted(group.title)}`;
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    return linkName(page, link);
  }
  return page.notes?.some((item) => item.id === id) === true ? "a note" : id;
}

function linkName(page: Page, link: Link): string {
  const what = link.kind !== undefined && link.kind !== "other" ? `${link.kind} connection` : "line";
  return `the ${what} from ${nameOf(page, link.from)} to ${nameOf(page, link.to)}`;
}

/** A drawn line's name: its link's, or the arrow's between its two items. */
function lineName(page: Page, line: DrawnLine): string {
  const link = page.links.find((item) => item.id === line.id);
  return link ? linkName(page, link) : `the line from ${nameOf(page, line.from)} to ${nameOf(page, line.to)}`;
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
 * Whether one item holds the other (a frame and an item in it), or a note is attached to the other or
 * to something a frame holds (the frame's box takes the note in).
 */
function belongTogether(context: Context, a: string, b: string): boolean {
  const { tree, page } = context;
  const holds = (frame: string, item: string) => {
    const held = tree.inside(frame);
    const attach = page.notes?.find((entry) => entry.id === item)?.attach;
    return held.has(item) || (attach !== undefined && held.has(attach));
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
  const names = pairs.map(([a, b]) => `${nameOf(context.page, a)} and ${nameOf(context.page, b)}`);
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
      message: `Lines cross ${plural(total, "time")}: ${listed(crossed.map(({ a, b }) => `${lineName(page, a)} crosses ${lineName(page, b)}`))}.`,
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
        message: `${plural(through.length, "line")} cross${through.length === 1 ? "es" : ""} ${nameOf(page, frame)} without starting or ending in it: ${listed(through.map((line) => lineName(page, line)))}.`,
        suggestion: "Call arrange_page, or move the frame or the cards so the lines go around it.",
      });
    }
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

function density(context: Context): PageCheck[] {
  const { page, tree, visible } = context;
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
        message: `${opening(nameOf(page, frame))} shows ${plural(held.length, "item")}; about ${THRESHOLDS.frameItems} is the most a frame reads well with.`,
        suggestion: "Nest frames inside it and collapse some (frame_cards, set_view), or move part of it to another page.",
      });
    }
  }
  for (const card of visible) {
    const flow = context.geometry.flows.get(card);
    if (flow && flow.steps > THRESHOLDS.flowSteps) {
      findings.push({
        kind: "density",
        items: [card],
        message: `${opening(nameOf(page, card))} shows ${plural(flow.steps, "step")} in Flow; about ${THRESHOLDS.flowSteps} is the most a Flow card reads well with.`,
        suggestion: "group_steps into 3 to 7 step groups and collapse the ones the page isn't about (set_view).",
      });
    }
  }
  return findings;
}

function unlabeled(context: Context): PageCheck[] {
  return context.visible.flatMap((card): PageCheck[] => {
    const flow = context.geometry.flows.get(card);
    if (!flow || flow.unlabeled === 0) {
      return [];
    }
    const name = nameOf(context.page, card);
    return [
      {
        kind: "unlabeled",
        items: [card],
        message: `${opening(name)} has ${plural(flow.unlabeled, "Flow step")} that still ${flow.unlabeled === 1 ? "shows a name" : "show names"} from the code.`,
        suggestion: `label_steps on ${name}: 2 to 8 words each, never code (read_page lists the unlabeled steps with their keys).`,
      },
    ];
  });
}

function missingWords(context: Context): PageCheck[] {
  const { page } = context;
  const bare = [
    ...(page.frames ?? []).filter((frame) => frame.description === undefined).map((frame) => frame.id),
    ...page.groups.filter((group) => group.description === undefined).map((group) => group.id),
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
      message: `No description on ${listed(items.map((id) => nameOf(page, id)))}.`,
      suggestion: "describe each in one sentence: what it is and why, not how.",
    },
  ];
}

/** Whether a connection cites code: anchors, or a step for the kinds a step grounds. */
function connectionGrounded(link: Link): boolean {
  return (link.anchors ?? []).length > 0 || (link.step !== undefined && !ANCHORED_KINDS.has(link.kind ?? "other"));
}

function evidence(context: Context): PageCheck[] {
  const { page } = context;
  const bare = [
    ...(page.resources ?? []).filter((resource) => resource.kind !== "actor" && resource.anchors.length === 0).map((resource) => resource.id),
    ...page.links.filter((link) => link.kind !== undefined && !connectionGrounded(link)).map((link) => link.id),
  ];
  if (bare.length === 0) {
    return [];
  }
  const items = inOrder(context, bare);
  return [
    {
      kind: "evidence",
      items,
      message: `${opening(listed(items.map((id) => nameOf(page, id))))} cite${items.length === 1 ? "s" : ""} no code: nothing shows ${items.length === 1 ? "it is" : "they are"} true, and nothing says when ${items.length === 1 ? "it goes" : "they go"} out of date.`,
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
      message: `${opening(listed(alone.map((id) => nameOf(page, id))))} ${alone.length === 1 ? "has" : "have"} no line, connection, or frame to the rest of the page.`,
      suggestion: "connect them, put them in a frame (frame_cards), or take them off the page (remove_items).",
    },
  ];
}

function views(context: Context): PageCheck[] {
  const { page } = context;
  const cards = page.cards.filter((card) => context.visible.includes(card.id));
  const overview = (page.frames ?? []).length > 0 || cards.length >= THRESHOLDS.overviewCards;
  const detailed = cards.filter((card) => {
    const body = cardBody(cardKey(card), card.view ?? page.defaultView);
    return body === "code" || body === "flow";
  });
  if (!overview || detailed.length <= THRESHOLDS.overviewDetailed) {
    return [];
  }
  return [
    {
      kind: "views",
      items: detailed.map((card) => card.id),
      message: `${plural(detailed.length, "card")} show Flow or Code on an overview page: ${listed(detailed.map((card) => nameOf(page, card.id)))}.`,
      suggestion: "set_view Diagram for the cards that give context; keep Flow for the one or two the page is about.",
    },
  ];
}
