/**
 * Geometry for the Flow view (architecture §10.4, §11.8: Rust computes structure, TypeScript lays
 * it out). A small structured layout of the reader's lane tree, top to bottom like a Mermaid
 * flowchart: conditions put `no` in their own lane and `yes` to the right, loops and try/catch get
 * regions, folded "▸ N checks" chains collapse into one summary. Routes come from the model and are
 * drawn as orthogonal paths by their kind. The result depends only on the model, the open chains,
 * the labels, and the text measurer, so the same input always lays out the same way.
 */

import type { FlowItem, FlowModel, FlowNode, RouteKind } from "../ipc/generated";

/** Measures the width of `text` in pixels for a CSS font shorthand. */
export type Measure = (text: string, font: string) => number;

export const FLOW_FONTS = {
  label: '500 13px "Inter Variable", system-ui, sans-serif',
  name: '600 12.5px "JetBrains Mono Variable", ui-monospace, monospace',
  pill: '600 12.5px "Inter Variable", system-ui, sans-serif',
} as const;

/** Room a measured line keeps so subpixel rounding never wraps it. */
const SLACK = 6;

/** Vertical rhythm of a node box (flow.css keeps the same numbers). */
export const FLOW_METRICS = {
  padY: 8,
  padX: 12,
  typeRow: 14,
  rowGap: 3,
  labelLine: 18,
  calleeRow: 16,
  callbackRow: 22,
  callbackGap: 4,
  noteRow: 16,
  pill: 30,
  pillPadX: 16,
} as const;

/** Spacing between items, lanes, and inside regions. */
export const FLOW_SPACING = {
  vgap: 30,
  laneGap: 28,
  regionPad: 18,
  regionTitle: 22,
  regionBottom: 14,
  diagramPad: 16,
  returnGutter: 26,
  join: 15,
} as const;

/** Lane widths tried, widest first, until the diagram fits the card's maximum width. */
const LANE_WIDTHS = [232, 216, 200, 184, 168, 152, 140] as const;

export interface PlacedNode {
  /** `"3"` for model node 3; `"chain:3"` for the summary of the folded chain headed by node 3. */
  readonly id: string;
  readonly node: FlowNode;
  /** The label shown (the board's label, else the fallback name). */
  readonly label: string;
  /** True when the label is a fallback name from the code (drawn with a dotted underline). */
  readonly fallback: boolean;
  readonly labelLines: number;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  /** For a chain head: whether its chain is folded into a summary. */
  readonly chainFolded?: boolean;
  /** For a chain summary: what its checks end in. */
  readonly summary?: boolean;
}

export interface FlowRegion {
  readonly kind: "loop" | "try" | "catch";
  readonly title: string;
  readonly subtitle?: string;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  /** For a loop: its header node's id. */
  readonly header?: string;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

export interface FlowEdge {
  readonly id: string;
  readonly from: string;
  readonly to: string;
  readonly kind: RouteKind;
  readonly points: readonly Point[];
  readonly label: string | null;
  readonly labelAt: Point | null;
  readonly dashed: boolean;
}

export interface FlowLayout {
  readonly width: number;
  readonly height: number;
  readonly laneWidth: number;
  readonly nodes: readonly PlacedNode[];
  readonly regions: readonly FlowRegion[];
  readonly edges: readonly FlowEdge[];
}

export interface FlowLayoutOptions {
  /** Authored labels by block key; they replace fallback names. */
  readonly labels?: Readonly<Record<string, string>>;
  /** Chain heads (by block key) the person opened; other chains of three or more checks fold. */
  readonly openChains?: ReadonlySet<string>;
  readonly measure: Measure;
  /** The widest the diagram may be (the card's maximum width less its border). */
  readonly maxWidth: number;
}

// ---------------------------------------------------------------------------------------------
// Text

/** Wraps `text` into lines no wider than `width` (long words break), returning the line count. */
export function wrapLines(text: string, width: number, font: string, measure: Measure): number {
  const words = text.split(/\s+/).filter((word) => word.length > 0);
  if (words.length === 0) {
    return 1;
  }
  let lines = 1;
  let current = 0;
  const space = measure(" ", font);
  for (const word of words) {
    let wordWidth = measure(word, font);
    if (current > 0 && current + space + wordWidth <= width) {
      current += space + wordWidth;
      continue;
    }
    if (current > 0) {
      lines += 1;
    }
    while (wordWidth > width) {
      lines += 1;
      wordWidth -= width;
    }
    current = wordWidth;
  }
  return lines;
}

const TYPE_NAMES: Readonly<Partial<Record<FlowNode["kind"], string>>> = {
  variable: "VARIABLE",
  call: "CALL",
  statement: "STATEMENT",
  condition: "CONDITION",
  loop: "LOOP",
  return: "RETURN",
  throw: "THROW",
  break: "BREAK",
  continue: "CONTINUE",
  definition: "DEFINITION",
  opaque: "NOT DRAWN",
};

/** The compiler-derived type a node shows above its label. */
export function nodeTypeName(node: FlowNode): string {
  if (node.kind === "call") {
    const main = node.call !== undefined ? node.chips?.[node.call] : undefined;
    if (main?.kind === "constructs") {
      return main.resolution === "external" ? "EXTERNAL NEW" : "NEW";
    }
    if (main?.resolution === "external") {
      return "EXTERNAL CALL";
    }
  }
  return TYPE_NAMES[node.kind] ?? node.kind.toUpperCase();
}

/** The main call chip, when it can be followed. */
export function followableCall(node: FlowNode) {
  const main = node.call !== undefined ? node.chips?.[node.call] : undefined;
  return main?.resolution === "resolved" && main.target !== undefined ? main : undefined;
}

// ---------------------------------------------------------------------------------------------
// Node sizes

interface Sized {
  readonly id: string;
  readonly node: FlowNode;
  readonly label: string;
  readonly fallback: boolean;
  readonly labelLines: number;
  readonly width: number;
  readonly height: number;
  readonly chainFolded?: boolean;
  readonly summary?: boolean;
}

interface Context {
  readonly model: FlowModel;
  readonly options: FlowLayoutOptions;
  readonly laneWidth: number;
}

function labelOf(node: FlowNode, options: FlowLayoutOptions): { text: string; fallback: boolean } {
  const authored = node.key !== undefined && node.kind !== "start" && node.kind !== "end" ? options.labels?.[node.key] : undefined;
  if (authored !== undefined && authored.trim().length > 0) {
    return { text: authored, fallback: false };
  }
  return { text: node.label, fallback: node.fallback === true };
}

function sizeNode(node: FlowNode, ctx: Context, id = String(node.id), overrides: Partial<Sized> = {}): Sized {
  const { measure } = ctx.options;
  const label = overrides.label !== undefined ? { text: overrides.label, fallback: false } : labelOf(node, ctx.options);
  const m = FLOW_METRICS;
  if (node.kind === "start" || node.kind === "end") {
    const font = node.kind === "start" ? FLOW_FONTS.name : FLOW_FONTS.pill;
    // The pill's padding plus its 1.5px border on each side.
    const chrome = m.pillPadX * 2 + 3;
    const width = Math.min(ctx.laneWidth, Math.ceil(measure(label.text, font) + chrome + SLACK));
    const lines = wrapLines(label.text, width - chrome - SLACK, font, measure);
    return { id, node, label: label.text, fallback: false, labelLines: lines, width, height: m.pill + (lines - 1) * m.labelLine, ...overrides };
  }
  if (node.kind === "condition") {
    // Text sits in the middle 55% of the diamond's width; the height leaves it room at that width.
    const textWidth = ctx.laneWidth * 0.55;
    const lines = wrapLines(label.text, textWidth - SLACK, FLOW_FONTS.label, measure);
    const content = m.typeRow + m.rowGap + lines * m.labelLine + (node.checks !== undefined ? 20 : 0);
    const height = Math.max(72, Math.ceil(content / 0.45) + 8);
    return { id, node, label: label.text, fallback: label.fallback, labelLines: lines, width: ctx.laneWidth, height, ...overrides };
  }
  const inset = node.kind === "loop" ? 22 : m.padX;
  const lines = wrapLines(label.text, ctx.laneWidth - inset * 2 - SLACK, FLOW_FONTS.label, measure);
  let height = m.padY * 2 + m.typeRow + m.rowGap + lines * m.labelLine;
  if (followableCall(node) && overrides.summary !== true) {
    height += m.rowGap + m.calleeRow;
  }
  if (node.callbacks && overrides.summary !== true) {
    height += node.callbacks.length * (m.callbackGap + m.callbackRow);
  }
  if (node.kind === "opaque") {
    height += m.rowGap + m.noteRow;
  }
  return { id, node, label: label.text, fallback: label.fallback, labelLines: lines, width: ctx.laneWidth, height, ...overrides };
}

// ---------------------------------------------------------------------------------------------
// Structured placement

interface Block {
  width: number;
  height: number;
  nodes: PlacedNode[];
  regions: FlowRegion[];
}

const emptyBlock = (): Block => ({ width: 0, height: 0, nodes: [], regions: [] });

function shift(block: Block, dx: number, dy: number, into: Block): void {
  for (const node of block.nodes) {
    into.nodes.push({ ...node, x: node.x + dx, y: node.y + dy });
  }
  for (const region of block.regions) {
    into.regions.push({ ...region, x: region.x + dx, y: region.y + dy });
  }
}

function single(sized: Sized): Block {
  return { width: sized.width, height: sized.height, nodes: [{ ...sized, x: 0, y: 0 }], regions: [] };
}

/** Nodes hidden by folded chains, mapped to the node that stands for them. */
interface Folding {
  readonly representative: Map<number, string>;
  /** Folded chain heads by node id, with their summary's outcome. */
  readonly folded: Map<number, NonNullable<FlowNode["outcome"]>>;
}

function nodeAt(ctx: Context, id: number): FlowNode | undefined {
  return ctx.model.nodes[id];
}

function collectNodes(items: readonly FlowItem[], into: number[]): void {
  for (const item of items) {
    if (item.type === "node") {
      into.push(item.node);
    } else if (item.type === "branch") {
      into.push(item.node);
      collectNodes(item.yes, into);
      collectNodes(item.no, into);
    } else if (item.type === "loop") {
      into.push(item.node);
      collectNodes(item.body, into);
    } else {
      collectNodes(item.body, into);
      collectNodes(item.handler, into);
    }
  }
}

/** The branches of a chain headed by `head`, ending with the last `else if`. */
function chainOf(head: Extract<FlowItem, { type: "branch" }>): Extract<FlowItem, { type: "branch" }>[] {
  const chain = [head];
  let current = head;
  while (current.elseIf && current.no.length === 1) {
    const next = current.no[0];
    if (next?.type !== "branch") {
      break;
    }
    chain.push(next);
    current = next;
  }
  return chain;
}

function planFolding(ctx: Context, items: readonly FlowItem[], folding: Folding): void {
  for (const item of items) {
    if (item.type === "branch") {
      const node = nodeAt(ctx, item.node);
      const open = node?.key !== undefined && ctx.options.openChains?.has(node.key) === true;
      if (node?.checks !== undefined && node.outcome !== undefined && !open) {
        const chain = chainOf(item);
        folding.folded.set(item.node, node.outcome);
        const summary = `chain:${item.node}`;
        for (const [index, branch] of chain.entries()) {
          if (index > 0) {
            folding.representative.set(branch.node, String(item.node));
          }
          const hidden: number[] = [];
          collectNodes(branch.yes, hidden);
          for (const id of hidden) {
            folding.representative.set(id, summary);
          }
        }
        const last = chain.at(-1) ?? item;
        planFolding(ctx, last.no, folding);
        continue;
      }
      planFolding(ctx, item.yes, folding);
      planFolding(ctx, item.no, folding);
    } else if (item.type === "loop") {
      planFolding(ctx, item.body, folding);
    } else if (item.type === "try") {
      planFolding(ctx, item.body, folding);
      planFolding(ctx, item.handler, folding);
    }
  }
}

function summaryLabel(outcome: NonNullable<FlowNode["outcome"]>, checks: number): string {
  if (outcome === "throw") {
    return "Throws (one per check)";
  }
  if (outcome === "return") {
    return "Returns (one per check)";
  }
  return `${checks} branches · open to read each`;
}

function layoutBranch(item: Extract<FlowItem, { type: "branch" }>, ctx: Context, folding: Folding): Block {
  const node = nodeAt(ctx, item.node);
  if (!node) {
    return emptyBlock();
  }
  const outcome = folding.folded.get(item.node);
  const block = emptyBlock();
  const head = sizeNode(node, ctx, String(item.node), outcome !== undefined ? { chainFolded: true } : node.checks !== undefined ? { chainFolded: false } : {});
  block.nodes.push({ ...head, x: 0, y: 0 });
  let yes: Block;
  let no: readonly FlowItem[];
  let yesEnds: boolean;
  if (outcome !== undefined) {
    const summaryNode: FlowNode = {
      id: node.id,
      kind: outcome === "throw" ? "throw" : outcome === "return" ? "return" : "statement",
      label: summaryLabel(outcome, node.checks ?? 0),
      lane: node.lane,
      ...(node.key !== undefined ? { key: node.key } : {}),
    };
    const summary = sizeNode(summaryNode, ctx, `chain:${item.node}`, { label: summaryLabel(outcome, node.checks ?? 0), summary: true });
    yes = single(summary);
    const last = chainOf(item).at(-1) ?? item;
    no = last.no;
    yesEnds = outcome !== "mixed";
  } else {
    yes = layoutSeq(item.yes, ctx, folding);
    no = item.no;
    yesEnds = item.yesEnds;
  }
  const noBlock = layoutSeq(no, ctx, folding);
  const top = head.height + FLOW_SPACING.vgap;
  const noBelow = no.length > 0 && yesEnds;
  if (noBelow) {
    const yesX = ctx.laneWidth + FLOW_SPACING.laneGap;
    shift(yes, yesX, top, block);
    const belowY = top + yes.height + FLOW_SPACING.vgap;
    shift(noBlock, 0, belowY, block);
    block.width = Math.max(yesX + yes.width, noBlock.width, ctx.laneWidth);
    block.height = belowY + noBlock.height;
  } else {
    const yesX = Math.max(ctx.laneWidth, noBlock.width) + FLOW_SPACING.laneGap;
    shift(noBlock, 0, top, block);
    shift(yes, yesX, top, block);
    block.width = Math.max(yesX + yes.width, ctx.laneWidth);
    block.height = top + Math.max(noBlock.height, yes.height);
    if (yes.nodes.length === 0 && noBlock.nodes.length === 0) {
      block.height = head.height;
    }
  }
  return block;
}

function layoutLoop(item: Extract<FlowItem, { type: "loop" }>, ctx: Context, folding: Folding): Block {
  const node = nodeAt(ctx, item.node);
  if (!node) {
    return emptyBlock();
  }
  const s = FLOW_SPACING;
  const header = sizeNode(node, ctx);
  const body = layoutSeq(item.body, ctx, folding);
  const block = emptyBlock();
  const inner = s.regionPad;
  let y = s.regionTitle;
  if (item.testAfter) {
    if (body.nodes.length > 0) {
      shift(body, inner, y, block);
      y += body.height + s.vgap;
    }
    block.nodes.push({ ...header, x: inner, y });
    y += header.height;
  } else {
    block.nodes.push({ ...header, x: inner, y });
    y += header.height;
    if (body.nodes.length > 0) {
      y += s.vgap;
      shift(body, inner, y, block);
      y += body.height;
    }
  }
  const width = Math.max(header.width, body.width) + inner * 2;
  const height = y + s.regionBottom;
  block.regions.unshift({ kind: "loop", title: "REPEATS", x: 0, y: 0, width, height, header: header.id });
  block.width = width;
  block.height = height;
  return block;
}

function layoutTry(item: Extract<FlowItem, { type: "try" }>, ctx: Context, folding: Folding): Block {
  const s = FLOW_SPACING;
  const body = layoutSeq(item.body, ctx, folding);
  const handler = layoutSeq(item.handler, ctx, folding);
  const block = emptyBlock();
  const tryWidth = Math.max(body.width, ctx.laneWidth) + s.regionPad * 2;
  const catchWidth = Math.max(handler.width, ctx.laneWidth) + s.regionPad * 2;
  const height = s.regionTitle + Math.max(body.height, handler.height) + s.regionBottom;
  const catchX = tryWidth + s.laneGap;
  block.regions.push({ kind: "try", title: "TRY", x: 0, y: 0, width: tryWidth, height });
  block.regions.push({
    kind: "catch",
    title: "CATCH",
    ...(item.binding !== undefined ? { subtitle: item.binding } : {}),
    x: catchX,
    y: 0,
    width: catchWidth,
    height,
  });
  shift(body, s.regionPad, s.regionTitle, block);
  shift(handler, catchX + s.regionPad, s.regionTitle, block);
  block.width = catchX + catchWidth;
  block.height = height;
  return block;
}

function layoutItem(item: FlowItem, ctx: Context, folding: Folding): Block {
  switch (item.type) {
    case "node": {
      const node = nodeAt(ctx, item.node);
      return node ? single(sizeNode(node, ctx)) : emptyBlock();
    }
    case "branch":
      return layoutBranch(item, ctx, folding);
    case "loop":
      return layoutLoop(item, ctx, folding);
    case "try":
      return layoutTry(item, ctx, folding);
  }
}

function layoutSeq(items: readonly FlowItem[], ctx: Context, folding: Folding, depth = 0): Block {
  const block = emptyBlock();
  // The reader caps nesting at 100 levels (deeper code is one opaque node), so this recursion is bounded.
  if (depth > 200) {
    return block;
  }
  let y = 0;
  for (const item of items) {
    const child = layoutItem(item, ctx, folding);
    if (child.nodes.length === 0 && child.regions.length === 0) {
      continue;
    }
    if (block.nodes.length > 0 || block.regions.length > 0) {
      y += FLOW_SPACING.vgap;
    }
    shift(child, 0, y, block);
    y += child.height;
    block.width = Math.max(block.width, child.width);
  }
  block.height = y;
  return block;
}

// ---------------------------------------------------------------------------------------------
// Routes

interface Box {
  readonly x: number;
  readonly y: number;
  readonly right: number;
  readonly bottom: number;
  readonly cx: number;
  readonly cy: number;
}

const boxOf = (node: PlacedNode): Box => ({
  x: node.x,
  y: node.y,
  right: node.x + node.width,
  bottom: node.y + node.height,
  cx: node.x + node.width / 2,
  cy: node.y + node.height / 2,
});

const ROUTE_LABELS: Readonly<Partial<Record<RouteKind, string>>> = {
  yes: "yes",
  no: "no",
  elseIf: "no",
  each: "each",
  next: "next",
  again: "again",
  test: "test",
  done: "done",
  break: "break",
  continue: "continue",
  error: "on error",
};

/** The visible routes: hidden chain nodes are replaced by their representative, duplicates dropped. */
function visibleRoutes(model: FlowModel, folding: Folding): { from: string; to: string; kind: RouteKind }[] {
  const seen = new Set<string>();
  const routes: { from: string; to: string; kind: RouteKind }[] = [];
  for (const route of model.routes) {
    const from = folding.representative.get(route.from) ?? String(route.from);
    const to = folding.representative.get(route.to) ?? String(route.to);
    if (from === to) {
      continue;
    }
    let kind = route.kind;
    if (from.startsWith("chain:")) {
      // A summary of throws ends; a summary of returns keeps one route to End.
      const outcome = folding.folded.get(Number(from.slice("chain:".length)));
      if (outcome === "throw" || (outcome === "return" && kind !== "return")) {
        continue;
      }
    }
    if (to.startsWith("chain:")) {
      kind = "yes";
    } else if (folding.folded.has(Number(from)) && route.from !== Number(from)) {
      // The last check's `no` now leaves from the folded head.
      kind = "no";
    }
    const id = `${from}>${to}>${kind}`;
    if (seen.has(id)) {
      continue;
    }
    seen.add(id);
    routes.push({ from, to, kind });
  }
  return routes;
}

function routeEdge(
  route: { from: string; to: string; kind: RouteKind },
  byId: ReadonlyMap<string, PlacedNode>,
  nodes: readonly PlacedNode[],
  regions: readonly FlowRegion[],
  gutterX: number,
): FlowEdge | null {
  const source = byId.get(route.from);
  const target = byId.get(route.to);
  if (!source || !target) {
    return null;
  }
  const s = boxOf(source);
  const t = boxOf(target);
  const join = FLOW_SPACING.join;
  const others = nodes.filter((node) => node.id !== source.id && node.id !== target.id).map(boxOf);
  const columnClear = (x: number, y1: number, y2: number) =>
    !others.some((box) => x > box.x - 1 && x < box.right + 1 && box.y < Math.max(y1, y2) && box.bottom > Math.min(y1, y2));
  const rowClear = (y: number, x1: number, x2: number) =>
    !others.some((box) => y > box.y - 1 && y < box.bottom + 1 && box.x < Math.max(x1, x2) && box.right > Math.min(x1, x2));
  const loopAround = (node: PlacedNode) =>
    regions
      .filter((region) => region.kind === "loop" && node.x >= region.x && node.x < region.x + region.width && node.y >= region.y && node.y < region.y + region.height)
      .sort((a, b) => a.width * a.height - b.width * b.height)[0];
  let points: Point[];
  const kind = route.kind;
  if (kind === "error") {
    const mid = (s.right + t.x) / 2;
    points = Math.abs(s.cy - t.cy) < 1 ? [{ x: s.right, y: s.cy }, { x: t.x, y: t.cy }] : [{ x: s.right, y: s.cy }, { x: mid, y: s.cy }, { x: mid, y: t.cy }, { x: t.x, y: t.cy }];
  } else if (kind === "return" && target.node.kind === "end") {
    if (Math.abs(s.cx - t.cx) < 1 && columnClear(s.cx, s.bottom, t.y)) {
      points = [{ x: s.cx, y: s.bottom }, { x: t.cx, y: t.y }];
    } else if (columnClear(s.cx, s.bottom, t.cy) && rowClear(t.cy, s.cx, s.cx > t.cx ? t.right : t.x)) {
      const side = s.cx > t.cx ? t.right : t.x;
      points = [{ x: s.cx, y: s.bottom }, { x: s.cx, y: t.cy }, { x: side, y: t.cy }];
    } else {
      points = [{ x: s.right, y: s.cy }, { x: gutterX, y: s.cy }, { x: gutterX, y: t.cy }, { x: t.right, y: t.cy }];
    }
  } else if (t.y < s.y + 1) {
    // Back to a loop header (next, continue, again): around the left side of the loop's region.
    const region = loopAround(target);
    const gx = region ? region.x + FLOW_SPACING.regionPad / 2 : Math.min(s.x, t.x) - FLOW_SPACING.regionPad / 2;
    if (rowClear(s.cy, gx, s.x)) {
      points = [{ x: s.x, y: s.cy }, { x: gx, y: s.cy }, { x: gx, y: t.cy }, { x: t.x, y: t.cy }];
    } else {
      const below = region ? region.y + region.height - FLOW_SPACING.regionBottom / 2 : s.bottom + join;
      points = [{ x: s.cx, y: s.bottom }, { x: s.cx, y: below }, { x: gx, y: below }, { x: gx, y: t.cy }, { x: t.x, y: t.cy }];
    }
  } else if (Math.abs(s.cx - t.cx) < 1 && columnClear(s.cx, s.bottom, t.y)) {
    points = [{ x: s.cx, y: s.bottom }, { x: t.cx, y: t.y }];
  } else if ((kind === "yes" || kind === "elseIf") && t.cx > s.cx && columnClear(t.cx, s.cy, t.y)) {
    points = [{ x: s.right, y: s.cy }, { x: t.cx, y: s.cy }, { x: t.cx, y: t.y }];
  } else if (columnClear(s.cx, s.bottom, t.y - join) && rowClear(t.y - join, s.cx, t.cx)) {
    points = [{ x: s.cx, y: s.bottom }, { x: s.cx, y: t.y - join }, { x: t.cx, y: t.y - join }, { x: t.cx, y: t.y }];
  } else {
    // Out of a loop (done, break) or past a blocked column: along the right side.
    const region = kind === "done" || kind === "break" ? loopAround(source) : undefined;
    const blocking = others.filter((box) => box.bottom > s.cy && box.y < t.y).reduce((right, box) => Math.max(right, box.right), s.right);
    const gx = region ? region.x + region.width - FLOW_SPACING.regionPad / 2 : blocking + FLOW_SPACING.regionPad / 2;
    points = [{ x: s.right, y: s.cy }, { x: gx, y: s.cy }, { x: gx, y: t.y - join }, { x: t.cx, y: t.y - join }, { x: t.cx, y: t.y }];
  }
  const label = ROUTE_LABELS[kind] ?? null;
  const first = points[0];
  const second = points[1];
  let labelAt: Point | null = null;
  if (label !== null && first && second) {
    labelAt = Math.abs(first.y - second.y) < 1 ? { x: (first.x + second.x) / 2, y: first.y - 8 } : { x: first.x + 6, y: Math.min(first.y + 12, (first.y + second.y) / 2) };
  }
  return {
    id: `${route.from}>${route.to}>${kind}`,
    from: route.from,
    to: route.to,
    kind,
    points,
    label,
    labelAt,
    dashed: kind === "error",
  };
}

// ---------------------------------------------------------------------------------------------
// The diagram

function layoutAt(model: FlowModel, options: FlowLayoutOptions, laneWidth: number): FlowLayout {
  const ctx: Context = { model, options, laneWidth };
  const folding: Folding = { representative: new Map(), folded: new Map() };
  planFolding(ctx, model.tree, folding);
  const s = FLOW_SPACING;
  const diagram = emptyBlock();
  let y = 0;
  const start = model.nodes[model.start];
  if (start) {
    diagram.nodes.push({ ...sizeNode(start, ctx), x: 0, y: 0 });
    y = diagram.nodes[0]?.height ?? 0;
  }
  const body = layoutSeq(model.tree, ctx, folding);
  if (body.nodes.length > 0 || body.regions.length > 0) {
    y += s.vgap;
    shift(body, 0, y, diagram);
    y += body.height;
  }
  const end = model.end !== undefined ? model.nodes[model.end] : undefined;
  if (end) {
    y += s.vgap;
    const sized = sizeNode(end, ctx);
    diagram.nodes.push({ ...sized, x: 0, y });
    y += sized.height;
  }
  // Start and End pills sit centered on the main lane.
  const nodes = diagram.nodes.map((node) =>
    node.node.kind === "start" || node.node.kind === "end" ? { ...node, x: node.x + (laneWidth - node.width) / 2 } : node,
  );
  const contentWidth = Math.max(laneWidth, body.width);
  const pad = s.diagramPad;
  const placed = nodes.map((node) => ({ ...node, x: Math.round(node.x + pad), y: Math.round(node.y + pad) }));
  const regions = diagram.regions.map((region) => ({ ...region, x: region.x + pad, y: region.y + pad }));
  const byId = new Map(placed.map((node) => [node.id, node]));
  const gutterX = pad + contentWidth + s.returnGutter / 2;
  const edges = visibleRoutes(model, folding)
    .map((route) => routeEdge(route, byId, placed, regions, gutterX))
    .filter((edge): edge is FlowEdge => edge !== null);
  const usesGutter = edges.some((edge) => edge.points.some((point) => Math.abs(point.x - gutterX) < 0.5));
  return {
    width: Math.ceil(pad * 2 + contentWidth + (usesGutter ? s.returnGutter : 0)),
    height: Math.ceil(pad * 2 + y),
    laneWidth,
    nodes: placed,
    regions,
    edges,
  };
}

/** Lays out a Flow model, narrowing lanes until the diagram fits `maxWidth` (or the narrowest lane). */
export function layoutFlow(model: FlowModel, options: FlowLayoutOptions): FlowLayout {
  let layout: FlowLayout | null = null;
  for (const laneWidth of LANE_WIDTHS) {
    layout = layoutAt(model, options, laneWidth);
    if (layout.width <= options.maxWidth) {
      return layout;
    }
  }
  return layout ?? layoutAt(model, options, LANE_WIDTHS[LANE_WIDTHS.length - 1] ?? 140);
}

/** An SVG path through `points` with rounded corners. */
export function roundedPath(points: readonly Point[], radius = 6): string {
  const first = points[0];
  if (!first) {
    return "";
  }
  let path = `M${first.x} ${first.y}`;
  for (let index = 1; index < points.length; index += 1) {
    const point = points[index];
    const previous = points[index - 1];
    const next = points[index + 1];
    if (!point || !previous) {
      continue;
    }
    if (!next) {
      path += ` L${point.x} ${point.y}`;
      continue;
    }
    const inLength = Math.hypot(point.x - previous.x, point.y - previous.y);
    const outLength = Math.hypot(next.x - point.x, next.y - point.y);
    const r = Math.min(radius, inLength / 2, outLength / 2);
    if (r < 0.5) {
      path += ` L${point.x} ${point.y}`;
      continue;
    }
    const before = { x: point.x - ((point.x - previous.x) / inLength) * r, y: point.y - ((point.y - previous.y) / inLength) * r };
    const after = { x: point.x + ((next.x - point.x) / outLength) * r, y: point.y + ((next.y - point.y) / outLength) * r };
    path += ` L${before.x} ${before.y} Q${point.x} ${point.y} ${after.x} ${after.y}`;
  }
  return path;
}
