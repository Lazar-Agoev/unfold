/**
 * A page's lines as the router takes them (`routeLines`), so the canvas, Arrange, and an agent route
 * the same lines the same way (docs/analysis/arranging-and-lines.md, 5.2):
 *
 * - An arrow leaves its port on the right (a call's row on an expanded card, a start's code row, a
 *   field's row, a compact card's header) and enters its target's header on the left, merging with the
 *   other arrows there. One that goes back (its target isn't past its card) may come in from the top or
 *   the bottom instead. In a frame that goes down, a compact card's arrows leave from its bottom, and
 *   arrows come into a card there from the top.
 * - A connection leaves its step's row on an expanded card that draws the step, a relationship its
 *   column's row on either side; otherwise the sides facing its other end (a side, where it can).
 *   It enters by the sides of its other end facing it, or a relationship by its column's row. The
 *   router picks among the sides, and spreads connections along a side; they never merge.
 */

import type { Connection, Key } from "../../ipc/generated";
import { frameChain } from "../../state/boards/frames";
import { NODE_PORT, stepPort, type Arrow } from "../arrows";
import { pageGeometry, type FramedPage } from "../frames/geometry";
import { routeArrows, routeLinks, type RoutedLink } from "../frames/routes";
import type { Rect, Size } from "../layout";
import { columnHandle } from "../resources/columns";
import { routeLines, type LineEnd, type LineRequest, type RoutedLine, type Side } from "./route";

/** The handle at a side's header (cards.css, resources.css): where a side's first line goes. */
const SIDE_HANDLE = "link-out-left";
/** Where that handle is, below an item's top, before it is measured: a card's or frame's header, a resource card's, an actor's. */
const HEADER = 22;
const RESOURCE_HEADER = 29;
const ACTOR_HEADER = 27;

export interface PageLines {
  readonly page: FramedPage & { readonly links: readonly Connection[] };
  /** The page as drawn (`pageGeometry`): each drawn item's rectangle, its frames, and what each expanded frame draws inside it. */
  readonly geometry: {
    readonly rects: ReadonlyMap<string, Rect>;
    readonly frames: readonly { readonly frame: { readonly id: string }; readonly collapsed: boolean }[];
    readonly inside: ReadonlyMap<string, readonly string[]>;
  };
  /** The arrows and connections between what is drawn (`routeArrows`, `routeLinks`). */
  readonly arrows: readonly Arrow[];
  readonly links: readonly RoutedLink[];
  /** A drawn item's port (or side handle) below its top: as drawn, else estimated; undefined where neither says. */
  readonly portOffset: (item: string, port: string) => number | undefined;
  /** Whether card `card` draws block `step` where a connection can leave from it (default: it has a port there). */
  readonly drawsStep?: (card: string, step: Key) => boolean;
  /** Route only the lines from or to these items (what a drag moves). */
  readonly only?: ReadonlySet<string>;
}

/** The sides of `a` that face `b`: those `b` lies wholly beyond; all four when they overlap. */
function facing(a: Rect, b: Rect): Side[] {
  const sides: Side[] = [];
  if (b.x >= a.x + a.width) {
    sides.push("right");
  }
  if (b.x + b.width <= a.x) {
    sides.push("left");
  }
  if (b.y >= a.y + a.height) {
    sides.push("bottom");
  }
  if (b.y + b.height <= a.y) {
    sides.push("top");
  }
  return sides.length > 0 ? sides : ["right", "left", "bottom", "top"];
}

/** The page's lines, routed (`routeLines`), by the arrow's or connection's id on the canvas. */
export function routePageLines(input: PageLines): Map<string, RoutedLine> {
  const { page, geometry, portOffset } = input;
  const { rects } = geometry;
  const resources = new Map((page.resources ?? []).map((resource) => [resource.id, resource]));
  const header = (item: string): number => {
    const kind = resources.get(item)?.kind;
    return portOffset(item, SIDE_HANDLE) ?? (kind === "actor" ? ACTOR_HEADER : kind !== undefined ? RESOURCE_HEADER : HEADER);
  };
  const down = (item: string) => frameChain(page, item)[0]?.direction === "down";
  const drawsStep = input.drawsStep ?? ((card: string, step: Key) => portOffset(card, stepPort(step)) !== undefined);
  const rect = (item: string) => rects.get(item) ?? { x: 0, y: 0, width: 0, height: 0 };

  const lines: LineRequest[] = [];
  for (const arrow of input.arrows) {
    const [from, to] = [rect(arrow.source), rect(arrow.target)];
    const source: LineEnd =
      arrow.sourceHandle === NODE_PORT && down(arrow.source)
        ? { item: arrow.source, sides: ["bottom"] }
        : { item: arrow.source, sides: ["right"], at: (arrow.sourceHandle === NODE_PORT ? undefined : portOffset(arrow.source, arrow.sourceHandle)) ?? header(arrow.source) };
    const forward = to.x >= from.x + from.width;
    const sides: Side[] = down(arrow.target) ? ["top"] : forward ? ["left"] : ["left", "top", "bottom"];
    lines.push({ id: arrow.id, from: source, to: { item: arrow.target, sides, at: header(arrow.target) }, merge: true });
  }
  for (const routed of input.links) {
    const { link } = routed;
    const [from, to] = [rect(routed.from), rect(routed.to)];
    const own = { from: link?.from === routed.from, to: link?.to === routed.to };
    const column = (item: string, name: string | undefined, way: "in" | "out"): LineEnd | undefined =>
      name === undefined ? undefined : { item, sides: ["left", "right"], at: portOffset(item, columnHandle(name, way, "right")) ?? header(item) };
    let source: LineEnd | undefined;
    let target: LineEnd | undefined;
    if (link?.kind === "relates") {
      source = own.from ? column(routed.from, link.fromColumn, "out") : undefined;
      target = own.to ? column(routed.to, link.toColumn, "in") : undefined;
    } else if (link?.step !== undefined && own.from && drawsStep(routed.from, link.step)) {
      const at = portOffset(routed.from, stepPort(link.step));
      source = at !== undefined ? { item: routed.from, sides: ["right"], at } : undefined;
    }
    if (!source) {
      const out = facing(from, to);
      source = { item: routed.from, sides: out.includes("right") ? ["right"] : out.includes("left") ? ["left"] : out };
    }
    lines.push({ id: routed.id, from: source, to: target ?? { item: routed.to, sides: facing(to, from) }, merge: false });
  }
  const frames = new Map(geometry.frames.flatMap(({ frame, collapsed }) => (collapsed ? [] : [[frame.id, geometry.inside.get(frame.id) ?? []] as const])));
  const headers = new Map([...rects.keys()].map((item) => [item, header(item)]));
  const { only } = input;
  return routeLines({ boxes: rects, frames, lines: only ? lines.filter((line) => only.has(line.from.item) || only.has(line.to.item)) : lines, headers });
}

/**
 * The page's lines routed from its document alone, as the canvas would draw them with these sizes
 * (`sizeOf`) and ports: its geometry, and its arrows and connections between what is drawn.
 */
export function routePage(page: FramedPage & { readonly links: readonly Connection[] }, sizeOf: (id: string) => Size, arrows: readonly Arrow[], portOffset: (item: string, port: string) => number | undefined): Map<string, RoutedLine> {
  const geometry = pageGeometry(page, sizeOf);
  return routePageLines({
    page,
    geometry,
    arrows: routeArrows(arrows, geometry.shownAs),
    links: routeLinks(page.links, geometry.shownAs),
    portOffset,
  });
}
