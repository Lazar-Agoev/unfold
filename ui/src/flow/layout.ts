/**
 * Geometry for a drawn flow (core-rebuild-plan.md, "The drawn flow"): the reader decides what is
 * drawn, in which order, and where every route goes; this lays it out, top to bottom like
 * diagram-blocks-examples.html's DRAWN panels, and applies no rules of its own.
 *
 * ```text
 * layoutFlow(flow, { measure })
 *   ( start ) ─ wire ─ step ─ wire ─ step … ─ wire ─ ( End )      one column; wires on its spine
 *   a step:  call box (+ its callbacks, + an inset under it)  ·  thin row  ·  exit pill
 *            frame: decision rows (a head per part ─yes─▶ its lane)  ·  loop (title, body, ↺ next)
 *                   try (TRY ┄on error┄▶ EXCEPT lanes; ELSE, FINALLY below)  ·  with
 *            combined step (one box, or its steps under its title)  ·  PROPOSED step
 *   an inset: the callee's own drawn flow, laid out the same way inside its box (3 levels at most)
 * ```
 *
 * Everything is placed in one pass from the top-left: a step's position depends only on the steps
 * before it. A wire is drawn between two steps only where routes go from the first (or anything
 * inside it) to the second, and it carries those routes; an exit's other routes are its words
 * (`→ End`). Gap ticks sit on the wire into their step. Text is measured with the injected measurer
 * and flow.css keeps the same numbers, so the same flow always lays out the same way.
 */

import type { CallBox, Callback, CombinedStep, DrawnFlow, ExitStep, FlowFrame, GapTick, Key, Lane, LinkEnd, OtherCall, ProposedStep, RepoPath, Route, RouteKind, Sequence, Step, ThinRow } from "../ipc/generated";
import { callMeta, combinedExits, combinedSummary, exitDestination, exitWord, frameWord, gapWords, hasMeta, headWords, outcomeWords, sectionWords, type CallMeta, type CombinedExit } from "./words";

/** Measures the width of `text` in pixels for a CSS font shorthand. */
export type Measure = (text: string, font: string) => number;

export interface Point {
  readonly x: number;
  readonly y: number;
}

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface FlowLayoutOptions {
  readonly measure: Measure;
  /** Thin rows listed one call per row, by their first call's key (a click on a row lists or folds it). */
  readonly listed?: ReadonlySet<Key>;
}

/** A library or unknown call listed on a call box, `depth` calls deep in its arguments. */
export interface CallRow {
  readonly call: OtherCall;
  readonly depth: number;
}

interface PieceBase {
  readonly rect: Rect;
  /** No route reaches it: drawn dimmed. */
  readonly dim: boolean;
}

/** What a frame is drawn as. */
type FrameLook = "decision" | "loop" | "try" | "with";

/** One drawn thing, positioned in the card body; the list is in paint order (frames before what they hold). */
export type FlowPiece =
  /** The start of the flow: the definition's name (`key` is the definition). */
  | (PieceBase & { readonly kind: "start"; readonly key: Key; readonly label: string; readonly named: boolean })
  | (PieceBase & { readonly kind: "end" })
  /** A call box; `path` opens it in place (the open path to its flow, then its key). Its callbacks are pieces of their own, inside it. */
  | (PieceBase & { readonly kind: "call"; readonly box: CallBox; readonly path: readonly Key[]; readonly rows: readonly CallRow[] })
  | (PieceBase & { readonly kind: "callback"; readonly callback: Callback; readonly path: readonly Key[] })
  /** A thin row; `key` (its first call's) lists or folds it. Its lines are in `path`. */
  | (PieceBase & { readonly kind: "thin"; readonly row: ThinRow; readonly key: Key; readonly listed: boolean; readonly path: RepoPath })
  /** An exit pill; `destination` is where it goes in words, absent when a wire shows it. A project call as its value is a chip opened by `path`. */
  | (PieceBase & { readonly kind: "exit"; readonly exit: ExitStep; readonly destination: string | null; readonly path: readonly Key[] })
  /** A control block's frame. `path` is the open path for the chips in its title (its header calls). */
  | (PieceBase & { readonly kind: "frame"; readonly frame: FlowFrame; readonly look: FrameLook; readonly path: readonly Key[] })
  /**
   * A decision part's head: a test (`test`, amber, with the project calls written in it as chips),
   * `else`, a case, or a switch's `no match`. A switch's subject is a test head with no part.
   */
  | (PieceBase & {
      readonly kind: "head";
      readonly look: "test" | "else" | "case" | "none";
      readonly lane: Lane | null;
      readonly text: string;
      /** The width the text gets (it is cut short past it). */
      readonly textWidth: number;
      readonly chips: readonly CallBox[];
      readonly path: readonly Key[];
    })
  /** A part drawn as a section: a try's TRY, EXCEPT, ELSE, and FINALLY, or a loop's ELSE. */
  | (PieceBase & { readonly kind: "section"; readonly lane: Lane; readonly look: "try" | "handler" | "else" | "finally" | "loop-else"; readonly loop: boolean })
  /** A combined step: one box when collapsed, a frame around its steps when open. */
  | (PieceBase & { readonly kind: "combined"; readonly step: CombinedStep; readonly exits: readonly CombinedExit[] })
  | (PieceBase & { readonly kind: "proposed"; readonly step: ProposedStep })
  /** A call or callback opened in place: its callee's flow is laid out inside. */
  | (PieceBase & { readonly kind: "inset"; readonly flow: DrawnFlow })
  /** A gap tick; its lines are in `path`. */
  | (PieceBase & { readonly kind: "gap"; readonly gap: GapTick; readonly path: RepoPath })
  /** A few words: `continues after the if`, `↺ next item`, `never runs`. */
  | (PieceBase & { readonly kind: "note"; readonly text: string; readonly tone: "muted" | "loop" | "faint" });

/** A drawn line: a polyline with an arrowhead at its end, and the routes it stands for. */
export interface FlowWire {
  readonly points: readonly Point[];
  readonly routes: readonly Route[];
  /** A possibility or an uncertainty (on error, may continue). */
  readonly dashed: boolean;
  /** An error route (on error). */
  readonly error: boolean;
  /** Its words, with their baseline point and how they align to it. */
  readonly label: { readonly text: string; readonly at: Point; readonly anchor: "start" | "middle" } | null;
}

/** Where an arrow to the callee's card leaves a call. */
interface CallAnchor {
  /** The call. */
  readonly call: Key;
  /** The definition it reaches. */
  readonly to: Key;
  /** The box it leaves from: the call's box or chip, or the collapsed combined step holding it. */
  readonly rect: Rect;
  /** The box's right edge, level with its first row. */
  readonly at: Point;
}

/** A laid-out drawn flow: the card body's size and everything in it, in card-body coordinates. */
export interface FlowGeometry {
  readonly width: number;
  readonly height: number;
  readonly pieces: readonly FlowPiece[];
  readonly wires: readonly FlowWire[];
  /**
   * Every drawn block's box by key: the definition (the start pill), steps, frames, parts, calls (a
   * listed call's box is the box or row listing it), callbacks, the hidden blocks a gap tick names,
   * the blocks a collapsed combined step holds, and the same inside insets. A callee opened twice in
   * one card draws its blocks twice; the first is kept.
   */
  readonly blocks: ReadonlyMap<Key, Rect>;
  /** The combined and proposed steps' boxes, by their page ids. */
  readonly items: ReadonlyMap<string, Rect>;
  /** Where arrows to callee cards leave project calls, in the order they are drawn. */
  readonly anchors: readonly CallAnchor[];
}

// ---------------------------------------------------------------------------------------------
// Numbers (flow.css keeps the same)

const UI = '"Inter Variable", system-ui, sans-serif';
const CODE = '"JetBrains Mono Variable", ui-monospace, monospace';

/** The fonts the view sets, by role. */
const FONT = {
  pill: `600 12px ${UI}`,
  pillCode: `600 12px ${CODE}`,
  callName: `600 12.5px ${CODE}`,
  callNameUi: `600 12.5px ${UI}`,
  chipName: `600 11.5px ${CODE}`,
  chipNameUi: `600 11.5px ${UI}`,
  codeName: `11px ${CODE}`,
  binds: `500 11px ${CODE}`,
  why: `10.5px ${UI}`,
  tag: `700 9px ${UI}`,
  row: `11.5px ${CODE}`,
  rowName: `500 11.5px ${CODE}`,
  callbackWord: `700 9px ${UI}`,
  callback: `11px ${CODE}`,
  word: `700 9.5px ${UI}`,
  value: `500 12px ${CODE}`,
  destination: `500 11px ${UI}`,
  thin: `11.5px ${CODE}`,
  thinSmall: `10.5px ${CODE}`,
  thinCount: `600 10px ${UI}`,
  gap: `500 10.5px ${CODE}`,
  label: `600 10.5px ${UI}`,
  note: `500 10.5px ${UI}`,
  diamond: `9px ${UI}`,
  head: `500 12px ${CODE}`,
  name: `600 12px ${UI}`,
  sectionNote: `500 11px ${UI}`,
  combinedTitle: `600 13px ${UI}`,
  glyph: `600 14px ${CODE}`,
  toggle: `600 10.5px ${UI}`,
  summary: `11px ${CODE}`,
  summaryCall: `600 11px ${CODE}`,
  routeVerb: `700 9px ${UI}`,
  route: `500 10.5px ${UI}`,
  kicker: `700 9px ${UI}`,
  proposed: `600 12.5px ${UI}`,
  insetTitle: `600 11.5px ${CODE}`,
  insetPlace: `11.5px ${CODE}`,
} as const;

/** The card body's padding around the flow. */
const PAD = { top: 16, right: 20, bottom: 18, left: 20 } as const;
/** A column's wire runs this far in from its left edge. */
const SPINE = 22;
/** A wire's height between two steps, and with words or a gap tick on it. */
const WIRE = 20;
const WIRE_LABELED = 26;
const PILL_HEIGHT = 26;
/** A horizontal arrow from a head to its lane: 5px, a 30px line, 5px. */
const HARROW = 40;
const HEAD_HEIGHT = 28;
const CHIP_HEIGHT = 22;
const CHEVRON = 18;
const GAP_HEIGHT = 17;
/** A gap tick on its own line: 4px above and below. */
const GAP_LINE = GAP_HEIGHT + 8;
const NOTE_HEIGHT = 14;
/** Room a measured line keeps so subpixel rounding never cuts it short. */
const SLACK = 4;
/** The `?` beside an unknown call: 15px and the gap before it. */
const UNKNOWN_MARK = 21;
/** An inset sits this far in under what it opens, 6px below it and 2px above what follows. */
const INSET = { indent: 12, above: 6, below: 2, padTop: 8, padX: 12, padBottom: 10, rule: 2, title: 16, titleGap: 7 } as const;
const CALL = { min: 168, max: 380, padTop: 5, padBottom: 6, padLeft: 10, padRight: 8, head: 20, meta: 16, row: 20, rowGap: 2, rowsTop: 10, callback: 26, callbackTop: 5 } as const;
const FRAME = { padTop: 8, padX: 12, padBottom: 12, title: 20, titleGap: 8 } as const;
const SECTION = { padTop: 8, padX: 10, padBottom: 10, title: 14, titleGap: 8 } as const;
/** The `on error` arrow between a try's TRY section and its handlers: its width, and how far down it runs. */
const ON_ERROR = { width: 64, drop: 39 } as const;
const COMBINED = { min: 220, max: 430, padTop: 7, padBottom: 8, padLeft: 10, padRight: 8, head: 22, line: 16, route: 20 } as const;
const PROPOSED_MAX = 280;
const THIN_MAX = 460;
const LOOP_BODY_MIN = 220;
/** The kinds an exit's words show rather than a wire. */
const WORD_KINDS: ReadonlySet<RouteKind> = new Set(["return", "raise", "continue"]);

// ---------------------------------------------------------------------------------------------
// Routes

interface Scope {
  readonly flow: DrawnFlow;
  /** The routes leaving each id. */
  readonly from: ReadonlyMap<number, readonly Route[]>;
  /** Each step's and lane's id with the ids inside it. */
  readonly within: ReadonlyMap<number, ReadonlySet<number>>;
  /** The open path to this flow: empty for the card's own. */
  readonly path: readonly Key[];
  /** Laid out inside an inset (no start or End pill; a return goes back to the caller). */
  readonly inset: boolean;
  /** The routes the wire into End (or an inset's "returns to the caller") stands for. */
  readonly toEnd: ReadonlySet<Route>;
}

function stepOf(route: Route): number | null {
  return typeof route.to === "object" ? route.to.step : null;
}

function scopeOf(flow: DrawnFlow, path: readonly Key[], inset: boolean): Scope {
  const from = new Map<number, Route[]>();
  for (const route of flow.routes) {
    const list = from.get(route.from) ?? [];
    list.push(route);
    from.set(route.from, list);
  }
  const within = new Map<number, ReadonlySet<number>>([[0, new Set([0])]]);
  const inSequence = (sequence: Sequence): number[] => sequence.steps.flatMap(inStep);
  const inLane = (lane: Lane): number[] => {
    const ids = [lane.id, ...inSequence(lane.steps)];
    within.set(lane.id, new Set(ids));
    return ids;
  };
  function inStep(step: Step): number[] {
    let inner: number[] = [];
    switch (step.kind) {
      case "decision":
      case "loop":
      case "try":
      case "with":
        inner = step.lanes.flatMap(inLane);
        break;
      case "combined":
        inner = step.steps ? inSequence(step.steps) : [];
        break;
      default:
        break;
    }
    const ids = [step.id, ...inner];
    within.set(step.id, new Set(ids));
    return ids;
  }
  inSequence(flow.steps);
  const last = flow.steps.steps.at(-1);
  const ends = [...(within.get(last?.id ?? 0) ?? [])].flatMap((id) => from.get(id) ?? []);
  const toEnd = ends.filter((route) => {
    if (route.to !== "end" || route.kind === "raise") {
      return false;
    }
    // The last step's own return draws the wire into End, as its words would; one inside it keeps its words.
    return route.kind !== "return" || (!inset && last?.kind === "exit" && route.from === last.id && route.through.length === 0);
  });
  return { flow, from, within, path, inset, toEnd: new Set(toEnd) };
}

function routesFrom(scope: Scope, ids: Iterable<number>): Route[] {
  return [...ids].flatMap((id) => scope.from.get(id) ?? []);
}

/** The routes a wire from `ids` into step `to` stands for. */
function carried(scope: Scope, ids: Iterable<number>, to: number): Route[] {
  return routesFrom(scope, ids).filter((route) => stepOf(route) === to && !WORD_KINDS.has(route.kind));
}

/** Whether a lane's end leaves `frame` (rather than going into another of its parts or exiting). */
function laneLeaves(scope: Scope, lane: Lane, frame: number): boolean {
  const last = lane.steps.steps.at(-1);
  if (!last) {
    return false;
  }
  const inside = scope.within.get(frame) ?? new Set<number>();
  return routesFrom(scope, scope.within.get(last.id) ?? [last.id]).some((route) => {
    if (WORD_KINDS.has(route.kind) || route.kind === "break" || route.kind === "next" || route.kind === "onError") {
      return false;
    }
    const to = stepOf(route);
    return to === null ? route.to === "end" : !inside.has(to);
  });
}

// ---------------------------------------------------------------------------------------------
// Layout

interface Size {
  readonly width: number;
  readonly height: number;
}

/** A step's size, and the box of the step itself (without what hangs under it). */
interface Placed extends Size {
  readonly box: Rect;
}

interface MutableRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Lays out `flow` (with the insets it holds) into the geometry of a card body. */
export function layoutFlow(flow: DrawnFlow, options: FlowLayoutOptions): FlowGeometry {
  const builder = new Builder(options.measure, options.listed ?? new Set());
  const size = builder.flow(flow, PAD.left, PAD.top, [], false);
  return {
    width: Math.ceil(Math.max(PAD.left + size.width, builder.right()) + PAD.right),
    height: Math.ceil(PAD.top + size.height + PAD.bottom),
    pieces: builder.pieces,
    wires: builder.wires,
    blocks: builder.blocks,
    items: builder.items,
    anchors: builder.anchors,
  };
}

class Builder {
  readonly pieces: FlowPiece[] = [];
  readonly wires: FlowWire[] = [];
  readonly blocks = new Map<Key, Rect>();
  readonly items = new Map<string, Rect>();
  readonly anchors: CallAnchor[] = [];

  constructor(
    private readonly measure: Measure,
    private readonly listed: ReadonlySet<Key>,
  ) {}

  /** The right edge of everything drawn. */
  right(): number {
    const pieces = this.pieces.map((piece) => piece.rect.x + piece.rect.width);
    const wires = this.wires.flatMap((wire) => wire.points.map((point) => point.x));
    return Math.max(0, ...pieces, ...wires);
  }

  private text(text: string, font: string): number {
    return this.measure(text, font);
  }

  /** Uppercase words with letter spacing (`tracking` in em). */
  private caps(text: string, font: string, size: number, tracking: number): number {
    return this.measure(text.toUpperCase(), font) + text.length * size * tracking;
  }

  private block(key: Key, rect: Rect): void {
    if (!this.blocks.has(key)) {
      this.blocks.set(key, rect);
    }
  }

  /** Where an arrow leaves for the definition `to` reaches, when it reaches a project definition. */
  private anchor(call: Key, to: LinkEnd, rect: Rect, y: number): void {
    if (to.outcome === "project" && to.key !== undefined) {
      this.anchors.push({ call, to: to.key, rect, at: { x: rect.x + rect.width, y } });
    }
  }

  private note(text: string, x: number, y: number, tone: "muted" | "loop" | "faint", dim = false): Rect {
    const rect = { x, y, width: Math.ceil(this.text(text, FONT.note)) + 2, height: NOTE_HEIGHT };
    this.pieces.push({ kind: "note", rect, dim, text, tone });
    return rect;
  }

  private wire(points: readonly Point[], routes: readonly Route[], label: FlowWire["label"], error = false): void {
    const dashed = error || routes.some((route) => route.kind === "mayContinue");
    this.wires.push({ points, routes, dashed, error, label });
  }

  // -- Flows and sequences --------------------------------------------------------------------

  /** Lays out a flow at (x, y): with its start and End pills, or (an inset) bare. */
  flow(flow: DrawnFlow, x: number, y: number, path: readonly Key[], inset: boolean): Size {
    const scope = scopeOf(flow, path, inset);
    const file = flow.header.path;
    let cy = y;
    let width = 0;
    if (!inset) {
      const named = flow.header.name !== undefined;
      const label = flow.header.name ?? flow.header.label;
      const rect = { x, y, width: Math.ceil(this.text(label, named ? FONT.pill : FONT.pillCode)) + 31, height: PILL_HEIGHT };
      this.pieces.push({ kind: "start", rect, dim: false, key: flow.header.key, label, named });
      this.block(flow.header.key, rect);
      cy += PILL_HEIGHT;
      width = rect.width;
    }
    const steps = this.sequence(flow.steps, x, cy, scope, inset ? null : new Set([0]));
    cy += steps.height;
    width = Math.max(width, steps.width);
    const endGap = flow.steps.endGap;
    if (!inset) {
      if (scope.toEnd.size > 0) {
        const routes = [...scope.toEnd];
        const link = this.link(x, cy, routes, doneWords(routes), endGap, file);
        cy += link.height;
        width = Math.max(width, link.width);
      } else {
        if (endGap) {
          const line = this.gapLine(x, cy, endGap, file);
          cy += line.height;
          width = Math.max(width, line.width);
        }
        cy += 14;
      }
      const end = { x, y: cy, width: Math.ceil(this.text("End", FONT.pill)) + 31, height: PILL_HEIGHT };
      this.pieces.push({ kind: "end", rect: end, dim: false });
      cy += PILL_HEIGHT;
    } else {
      if (endGap) {
        const line = this.gapLine(x, cy, endGap, file);
        cy += line.height;
        width = Math.max(width, line.width);
      }
      if (flow.steps.steps.length === 0 && !endGap) {
        width = Math.max(width, this.note("nothing drawn", x, cy + 7, "faint").width);
        cy += HEAD_HEIGHT;
      } else if (flow.steps.steps.length > 0 && scope.toEnd.size > 0) {
        width = Math.max(width, this.note("↩ returns to the caller", x, cy + 6, "faint").width);
        cy += 6 + NOTE_HEIGHT;
      }
    }
    return { width, height: cy - y };
  }

  /**
   * Lays out steps in a column at (x, y). With `lead`, a wire comes into the first step from those
   * ids (the start); without, the column is entered from its side or top.
   */
  private sequence(sequence: Sequence, x: number, y: number, scope: Scope, lead: ReadonlySet<number> | null): Size {
    const file = scope.flow.header.path;
    let cy = y;
    let width = 0;
    let previous = lead;
    for (const step of sequence.steps) {
      if (previous !== null) {
        const routes = carried(scope, previous, step.id);
        const link = this.link(x, cy, routes, doneWords(routes), step.gap, file);
        cy += link.height;
        width = Math.max(width, link.width);
      } else if (step.gap) {
        const line = this.gapLine(x, cy, step.gap, file);
        cy += line.height;
        width = Math.max(width, line.width);
      }
      const placed = this.step(step, x, cy, scope);
      cy += placed.height;
      width = Math.max(width, placed.width);
      previous = scope.within.get(step.id) ?? new Set([step.id]);
    }
    return { width, height: cy - y };
  }

  /**
   * The wire down a column's spine from y, standing for `routes` (none drawn when there are none), with
   * its words and the gap tick on it.
   */
  private link(x: number, y: number, routes: readonly Route[], words: string | null, gap: GapTick | undefined, file: RepoPath): Size {
    const uncertain = routes.some((route) => route.kind === "mayContinue");
    const text = words ?? (uncertain ? "may continue" : null);
    const height = text !== null || gap ? WIRE_LABELED : WIRE;
    const spine = x + SPINE;
    let right = spine;
    if (routes.length > 0) {
      const label = text !== null ? { text, at: { x: spine + 8, y: y + 16 }, anchor: "start" as const } : null;
      this.wire(
        [
          { x: spine, y },
          { x: spine, y: y + height },
        ],
        routes,
        label,
      );
      right = spine + 8 + (text !== null ? this.text(text, FONT.label) : 0);
    }
    if (gap) {
      const at = text !== null && routes.length > 0 ? right + 8 : spine + 8;
      right = this.gap(gap, at, y + 4, file).x + this.gapWidth(gap);
    }
    return { width: right - x, height };
  }

  private gapWidth(gap: GapTick): number {
    return Math.ceil(this.text(gapWords(gap), FONT.gap)) + 14;
  }

  private gap(gap: GapTick, x: number, y: number, file: RepoPath): Rect {
    const rect = { x, y, width: this.gapWidth(gap), height: GAP_HEIGHT };
    this.pieces.push({ kind: "gap", rect, dim: false, gap, path: file });
    for (const hidden of gap.hidden) {
      this.block(hidden.key, rect);
    }
    return rect;
  }

  /** A gap tick on its own line (before a part's first step, or after its last). */
  private gapLine(x: number, y: number, gap: GapTick, file: RepoPath): Size {
    const rect = this.gap(gap, x, y + 4, file);
    return { width: rect.width, height: GAP_LINE };
  }

  private step(step: Step, x: number, y: number, scope: Scope): Placed {
    const dim = step.state === "unreachable";
    let placed: Placed;
    switch (step.kind) {
      case "call":
        placed = this.call(step, x, y, scope, dim);
        break;
      case "thin":
        placed = this.thin(step, x, y, dim, scope.flow.header.path);
        break;
      case "decision":
        placed = step.form === "switch" || step.form === "match" ? this.cases(step, step.id, x, y, scope, dim) : this.decision(step, step.id, x, y, scope, dim);
        break;
      case "loop":
        placed = this.loop(step, step.id, x, y, scope, dim);
        break;
      case "try":
        placed = this.try(step, step.id, x, y, scope, dim);
        break;
      case "with":
        placed = this.with(step, step.id, x, y, scope, dim);
        break;
      case "exit":
        placed = this.exit(step, step.id, x, y, scope, dim);
        break;
      case "combined":
        placed = this.combined(step, step.id, x, y, scope, dim);
        break;
      case "proposed":
        placed = this.proposed(step, x, y, dim);
        break;
    }
    if (!dim) {
      return placed;
    }
    const { box } = placed;
    const note = this.note("never runs", box.x + box.width + 8, box.y + Math.max(0, (Math.min(box.height, PILL_HEIGHT) - NOTE_HEIGHT) / 2), "faint");
    return { ...placed, width: Math.max(placed.width, note.x + note.width - x) };
  }

  // -- Calls ----------------------------------------------------------------------------------

  /** A name, with the code name beside an overlay name. */
  private nameWidth(name: string | undefined, callee: string, codeFont: string, uiFont: string): number {
    return name !== undefined ? this.text(name, uiFont) + 8 + this.text(callee, FONT.codeName) : this.text(callee, codeFont);
  }

  private metaWidth(meta: CallMeta): number {
    const widths = [
      ...(meta.binds !== null ? [this.text(meta.binds, FONT.binds)] : []),
      ...(meta.feeds !== null ? [this.text(meta.feeds, FONT.codeName)] : []),
      ...meta.tags.map((tag) => this.caps(tag, FONT.tag, 9, 0.06) + 10),
      ...(meta.why !== null ? [this.text(meta.why, FONT.why)] : []),
    ];
    return widths.reduce((sum, width) => sum + width, 0) + Math.max(0, widths.length - 1) * 8;
  }

  /** Lays out a call box at (x, y): its rows, its callbacks (and their insets), then the inset under it. */
  private call(box: CallBox, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const path = [...scope.path, box.key];
    const meta = callMeta(box);
    const rows = callRows(box.calls, 0);
    const head =
      this.nameWidth(box.name, box.callee, FONT.callName, FONT.callNameUi) + (box.to.outcome === "unknown" ? UNKNOWN_MARK : 0) + (box.actions.open ? 12 + CHEVRON : 0);
    const rowWidth = (row: CallRow) => {
      const why = row.call.to.outcome === "project" ? "" : outcomeWords(row.call.to);
      return (
        row.depth * 14 +
        this.text("↳", FONT.row) +
        6 +
        this.text(row.call.callee, FONT.rowName) +
        (row.call.to.outcome === "unknown" ? UNKNOWN_MARK : 0) +
        (why.length > 0 ? 6 + this.text(why, FONT.why) : 0)
      );
    };
    const callbackWidth = (callback: Callback) =>
      9 + this.caps("callback", FONT.callbackWord, 9, 0.07) + 7 + this.text(callbackText(callback), FONT.callback) + 7 + (callback.actions.open ? 20 : 0) + 5;
    const inner = Math.max(head, hasMeta(meta) ? this.metaWidth(meta) : 0, ...rows.map(rowWidth), ...box.callbacks.map(callbackWidth)) + SLACK;
    let width = Math.min(CALL.max, Math.max(CALL.min, Math.ceil(inner) + CALL.padLeft + CALL.padRight + 2));
    const rect: MutableRect = { x, y, width, height: 0 };
    this.pieces.push({ kind: "call", rect, dim, box, path, rows });
    this.block(box.key, rect);
    const inside: MutableRect[] = [];
    let cy = y + 1 + CALL.padTop + CALL.head;
    if (hasMeta(meta)) {
      cy += 1 + CALL.meta;
    }
    if (rows.length > 0) {
      cy += CALL.rowsTop;
      rows.forEach((row, index) => {
        cy += index > 0 ? CALL.rowGap : 0;
        const rowRect = { x: x + 1 + CALL.padLeft, y: cy, width: 0, height: CALL.row };
        inside.push(rowRect);
        this.block(row.call.key, rowRect);
        cy += CALL.row;
      });
    }
    for (const callback of box.callbacks) {
      cy += CALL.callbackTop;
      const callbackRect = { x: x + 1 + CALL.padLeft, y: cy, width: 0, height: CALL.callback };
      inside.push(callbackRect);
      this.pieces.push({ kind: "callback", rect: callbackRect, dim, callback, path: [...scope.path, callback.key] });
      this.block(callback.key, callbackRect);
      cy += CALL.callback;
      if (callback.inset) {
        cy += INSET.above;
        const size = this.inset(callback.inset, x + 1 + CALL.padLeft + INSET.indent, cy, [...scope.path, callback.key], dim);
        width = Math.max(width, 1 + CALL.padLeft + INSET.indent + size.width + CALL.padRight + 1);
        cy += size.height + INSET.below;
      }
    }
    cy += CALL.padBottom + 1;
    rect.width = width;
    rect.height = cy - y;
    for (const item of inside) {
      item.width = width - 2 - CALL.padLeft - CALL.padRight;
    }
    this.anchor(box.key, box.to, rect, y + 1 + CALL.padTop + CALL.head / 2);
    let height = rect.height;
    let extent = width;
    if (box.inset) {
      const size = this.inset(box.inset, x + INSET.indent, y + height + INSET.above, path, dim);
      height += INSET.above + size.height + INSET.below;
      extent = Math.max(extent, INSET.indent + size.width);
    }
    return { width: extent, height, box: rect };
  }

  private chipWidth(box: CallBox): number {
    return (
      1 +
      7 +
      this.nameWidth(box.name, box.callee, FONT.chipName, FONT.chipNameUi) +
      (box.to.outcome === "unknown" ? UNKNOWN_MARK : 0) +
      (box.actions.open ? 4 + CHEVRON + 2 : 7) +
      1
    );
  }

  /** Chips in a row from x, centered on `middle`; each one's box is reported. Returns where the row ends. */
  private chips(boxes: readonly CallBox[], x: number, middle: number, gap: number): number {
    let cx = x;
    boxes.forEach((box, index) => {
      cx += index > 0 ? gap : 0;
      const rect = { x: cx, y: middle - CHIP_HEIGHT / 2, width: this.chipWidth(box), height: CHIP_HEIGHT };
      this.block(box.key, rect);
      this.anchor(box.key, box.to, rect, middle);
      cx += rect.width;
    });
    return cx;
  }

  /** The insets of chips opened in place, stacked under (x, y). */
  private chipInsets(boxes: readonly CallBox[], x: number, y: number, scope: Scope, dim: boolean): Size {
    let cy = y;
    let width = 0;
    for (const box of boxes) {
      if (box.inset) {
        cy += INSET.above;
        const size = this.inset(box.inset, x, cy, [...scope.path, box.key], dim);
        cy += size.height + INSET.below;
        width = Math.max(width, size.width);
      }
    }
    return { width, height: cy - y };
  }

  /** A call or callback opened in place: a box titled by the callee, with its flow inside. */
  private inset(flow: DrawnFlow, x: number, y: number, path: readonly Key[], dim: boolean): Size {
    const rect: MutableRect = { x, y, width: 0, height: 0 };
    this.pieces.push({ kind: "inset", rect, dim, flow });
    this.block(flow.header.key, rect);
    const title = this.text(flow.header.name ?? flow.header.label, flow.header.name !== undefined ? FONT.chipNameUi : FONT.insetTitle) + 6 + this.text(insetPlace(flow), FONT.insetPlace);
    const top = y + INSET.padTop + INSET.title + INSET.titleGap;
    const size = this.flow(flow, x + INSET.rule + INSET.padX, top, path, true);
    rect.width = Math.ceil(INSET.rule + INSET.padX + Math.max(title, size.width) + INSET.padX);
    rect.height = top - y + size.height + INSET.padBottom;
    return { width: rect.width, height: rect.height };
  }

  // -- Thin rows and exits --------------------------------------------------------------------

  private thinItemWidth(call: OtherCall, listed: boolean): number {
    let width = this.text(`${call.role === "decorator" ? "@" : ""}${call.callee}`, FONT.thin);
    width += call.to.outcome === "unknown" ? UNKNOWN_MARK : 0;
    width += call.binds !== undefined ? 6 + this.text(`→ ${call.binds}`, FONT.binds) : 0;
    const inside = insideNames(call);
    width += inside.length > 0 ? 6 + this.text(`+ ${inside.join(", ")}`, FONT.thinSmall) : 0;
    if (listed) {
      const why = outcomeWords(call.to);
      width += why.length > 0 ? 6 + this.text(why, FONT.why) : 0;
      width += call.feeds !== undefined ? 6 + this.text(`into ${call.feeds.name ?? call.feeds.callee}(…)`, FONT.thinSmall) : 0;
    }
    return width;
  }

  /** A thin row: its calls in a line (wrapping), or listed one per row. */
  private thin(row: ThinRow, x: number, y: number, dim: boolean, file: RepoPath): Placed {
    const first = row.calls[0];
    const key = first?.key ?? "";
    const listed = this.listed.has(key);
    const rect: MutableRect = { x, y, width: 0, height: 0 };
    this.pieces.push({ kind: "thin", rect, dim, row, key, listed, path: file });
    if (listed) {
      const header = this.caps("Library and unknown calls", FONT.callbackWord, 9, 0.07);
      const widths = row.calls.map((call) => this.thinItemWidth(call, true));
      rect.width = Math.ceil(Math.max(header, ...widths)) + 22;
      let cy = y + 1 + 7 + 14 + 2;
      for (const call of row.calls) {
        cy += 3;
        const line = { x: x + 11, y: cy, width: rect.width - 22, height: 18 };
        this.blockCall(call, line);
        cy += 18;
      }
      rect.height = cy + 7 + 1 - y;
      return { width: rect.width, height: rect.height, box: rect };
    }
    const inner = THIN_MAX - 18;
    const dot = this.text("·", FONT.thin) + 6;
    const separator = this.text(",", FONT.thin) + 6;
    const countText = thinCount(row);
    const units = row.calls.map((call, index) => this.thinItemWidth(call, false) + (index < row.calls.length - 1 ? separator : 0));
    if (countText !== null) {
      units.push(8 + this.text(countText, FONT.thinCount));
    }
    let lines = 1;
    let line = dot;
    let widest = dot;
    for (const unit of units) {
      if (line + unit > inner && line > 0) {
        lines += 1;
        line = 0;
      }
      line += unit;
      widest = Math.max(widest, line);
    }
    rect.width = Math.ceil(Math.min(inner, widest)) + 18;
    rect.height = 2 + 6 + lines * 18;
    for (const call of row.calls) {
      this.blockCall(call, rect);
    }
    return { width: rect.width, height: rect.height, box: rect };
  }

  private blockCall(call: OtherCall, rect: Rect): void {
    this.block(call.key, rect);
    for (const inside of call.inside) {
      this.blockCall(inside, rect);
    }
  }

  /** An exit pill: its word, its value (text, or a project call as a chip), and where it goes in words. */
  private exit(exit: ExitStep, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const own = (scope.from.get(id) ?? []).filter((route) => !scope.toEnd.has(route));
    const destination = exitDestination(own, exit.form, scope.inset);
    const value = exit.value;
    const chip = value?.kind === "call" && value.to.outcome === "project" ? value : null;
    let cx = x + 1.5 + 9 + this.caps(exitWord(exit.form), FONT.word, 9.5, 0.09);
    if (exit.name !== undefined) {
      cx += 7 + this.text(exit.name, FONT.name);
    }
    const height = chip ? 2 * 1.5 + 2 * 2 + CHIP_HEIGHT : PILL_HEIGHT;
    let chipStart = 0;
    if (chip) {
      cx += 7;
      chipStart = cx;
      cx += this.chipWidth(chip);
    } else if (value) {
      cx += 7 + this.text(value.kind === "text" ? value.text : value.callee, FONT.value) + (value.kind === "call" && value.to.outcome === "unknown" ? UNKNOWN_MARK : 0);
    }
    if (destination !== null) {
      cx += 7 + this.text(`→ ${destination}`, FONT.destination);
    }
    const rect = { x, y, width: Math.ceil(cx + 11 + 1.5 - x), height };
    this.pieces.push({ kind: "exit", rect, dim, exit, destination, path: scope.path });
    this.block(exit.key, rect);
    if (chip) {
      this.chips([chip], chipStart, y + height / 2, 0);
    } else if (value?.kind === "call") {
      this.block(value.key, rect);
    }
    let extent = rect.width;
    let total = height;
    if (chip?.inset) {
      const size = this.chipInsets([chip], x + INSET.indent, y + height, scope, dim);
      total += size.height;
      extent = Math.max(extent, INSET.indent + size.width);
    }
    return { width: extent, height: total, box: rect };
  }

  // -- Frames ---------------------------------------------------------------------------------

  /** A frame's title row: its keyword, name, header, header calls as chips, and binds. Returns its size. */
  private title(frame: FlowFrame, word: string, x: number, y: number, withHeader: boolean): Size {
    const height = frame.headerCalls.length > 0 && withHeader ? CHIP_HEIGHT : FRAME.title;
    let cx = x + this.caps(word, FONT.word, 9.5, 0.09);
    if (frame.name !== undefined) {
      cx += 8 + this.text(frame.name, FONT.name);
    }
    if (withHeader && frame.header !== undefined) {
      cx += 8 + this.text(frame.header, FONT.head);
    }
    if (withHeader && frame.headerCalls.length > 0) {
      cx = this.chips(frame.headerCalls, cx + 8, y + height / 2, 8);
    }
    if (frame.binds !== undefined) {
      cx += 8 + this.text(`→ ${frame.binds}`, FONT.binds);
    }
    return { width: cx - x, height };
  }

  private framePiece(frame: FlowFrame, look: FrameLook, x: number, y: number, scope: Scope, dim: boolean): MutableRect {
    const rect: MutableRect = { x, y, width: 0, height: 0 };
    this.pieces.push({ kind: "frame", rect, dim, frame, look, path: scope.path });
    this.block(frame.key, rect);
    return rect;
  }

  /** A test head: ◆, the test (or the part's name and its test), and the project calls in it as chips. */
  private testHead(lane: Lane | null, text: string, chips: readonly CallBox[], x: number, y: number, scope: Scope, dim: boolean): Rect {
    const name = lane?.name;
    const words = name !== undefined ? this.text(name, FONT.name) + 8 + this.text(text, FONT.codeName) : this.text(text, FONT.head);
    const chipsWidth = chips.reduce((sum, box) => sum + 6 + this.chipWidth(box), 0);
    const fixed = 1.5 + 8 + this.text("◆", FONT.diamond) + 6 + chipsWidth + 10 + 1.5;
    const textWidth = Math.ceil(Math.max(Math.min(words, 320 - fixed), 40));
    const height = chips.length > 0 ? 2 * 1.5 + 2 * 3 + CHIP_HEIGHT : HEAD_HEIGHT;
    const rect = { x, y, width: Math.ceil(fixed + textWidth), height };
    this.pieces.push({ kind: "head", rect, dim, look: "test", lane, text, textWidth, chips, path: scope.path });
    if (lane) {
      this.block(lane.key, rect);
    }
    this.chips(chips, x + 1.5 + 8 + this.text("◆", FONT.diamond) + 6 + textWidth + 6, y + height / 2, 6);
    return rect;
  }

  /** A head with words only: `else`, `case "paid"`, `default`, `no match`. */
  private plainHead(lane: Lane | null, look: "else" | "case" | "none", text: string, x: number, y: number, scope: Scope, dim: boolean): Rect {
    const textWidth = Math.ceil(lane?.name !== undefined ? this.text(lane.name, FONT.name) + 8 + this.text(text, FONT.codeName) : this.text(text, FONT.head));
    const rect = { x, y, width: textWidth + 23, height: HEAD_HEIGHT };
    this.pieces.push({ kind: "head", rect, dim, look, lane, text, textWidth, chips: [], path: scope.path });
    if (lane) {
      this.block(lane.key, rect);
    }
    return rect;
  }

  /** The horizontal arrow from a head into its lane, standing for `routes`. */
  private harrow(x: number, y: number, routes: readonly Route[], words: string | null): void {
    const label = words !== null ? { text: words, at: { x: x + HARROW / 2, y: y - 4 }, anchor: "middle" as const } : null;
    this.wire(
      [
        { x, y },
        { x: x + HARROW, y },
      ],
      routes,
      label,
    );
  }

  /**
   * A part's steps as a column at (x, y): `no calls` when it has none, its end gap, a dashed stub
   * when its last step may continue, and `continues after …` when its end leaves the frame.
   */
  private lane(lane: Lane, x: number, y: number, scope: Scope, frame: number, cont: string | null, dim: boolean): Size {
    const file = scope.flow.header.path;
    const steps = this.sequence(lane.steps, x, y, scope, null);
    let cy = y + steps.height;
    let width = steps.width;
    const last = lane.steps.steps.at(-1);
    if (!last && !lane.steps.endGap) {
      width = Math.max(width, this.note("no calls", x, y + 7, "faint", dim).width);
      cy += HEAD_HEIGHT;
    }
    if (lane.steps.endGap) {
      const line = this.gapLine(x, cy, lane.steps.endGap, file);
      cy += line.height;
      width = Math.max(width, line.width);
    }
    if (last) {
      const uncertain = (scope.from.get(last.id) ?? []).filter((route) => route.kind === "mayContinue");
      if (uncertain.length > 0) {
        const link = this.link(x, cy, uncertain, null, undefined, file);
        cy += link.height;
        width = Math.max(width, link.width);
      }
    }
    if (last && lane.fallsThrough) {
      width = Math.max(width, this.note("↓ falls into the next case", x, cy + 5, "faint", dim).width);
      cy += 5 + NOTE_HEIGHT;
    } else if (last && cont !== null && laneLeaves(scope, lane, frame)) {
      width = Math.max(width, this.note(`↓ ${cont}`, x, cy + 5, "faint", dim).width);
      cy += 5 + NOTE_HEIGHT;
    }
    return { width, height: cy - y };
  }

  /** An `if` or ternary: a row per part (its head ─yes─▶ its lane), `no` down from each test. */
  private decision(frame: FlowFrame, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect = this.framePiece(frame, "decision", x, y, scope, dim);
    const cx = x + 1 + FRAME.padX;
    let cy = y + 1 + FRAME.padTop;
    const title = this.title(frame, frameWord(frame.form), cx, cy, true);
    let inner = title.width;
    cy += title.height + FRAME.titleGap;
    const cont = frame.form === "if" ? `continues after the ${frame.form}` : null;
    frame.lanes.forEach((lane, index) => {
      const routes = scope.from.get(lane.id) ?? [];
      const laneDim = dim || lane.state === "unreachable";
      const head = lane.test
        ? this.testHead(lane, lane.test.text ?? "", lane.test.calls, cx, cy, scope, laneDim)
        : this.plainHead(lane, "else", headWords(lane), cx, cy, scope, laneDim);
      const middle = cy + HEAD_HEIGHT / 2;
      const into = routes.filter((route) => route.kind === (lane.test ? "yes" : "on"));
      this.harrow(head.x + head.width, middle, into, lane.test ? "yes" : null);
      const laneX = head.x + head.width + HARROW;
      const column = this.lane(lane, laneX, cy, scope, id, cont, laneDim);
      inner = Math.max(inner, laneX - cx + column.width);
      cy += Math.max(head.height, column.height);
      const insets = this.chipInsets(lane.test?.calls ?? [], cx + SPINE + INSET.indent, cy, scope, laneDim);
      inner = Math.max(inner, SPINE + INSET.indent + insets.width);
      cy += insets.height;
      if (index < frame.lanes.length - 1 || lane.test) {
        const no = routes.filter((route) => route.kind === "no");
        const bottom = cy + WIRE_LABELED;
        this.wire(
          [
            { x: cx + SPINE, y: head.y + head.height },
            { x: cx + SPINE, y: bottom },
          ],
          no,
          { text: "no", at: { x: cx + SPINE + 8, y: bottom - WIRE_LABELED + 16 }, anchor: "start" },
        );
        cy = bottom;
      }
    });
    rect.width = Math.ceil(inner + 2 * FRAME.padX + 2);
    rect.height = cy + FRAME.padBottom + 1 - y;
    return { width: rect.width, height: rect.height, box: rect };
  }

  /** A `switch` or `match`: its subject as a test head, then a rail with a row per case (and `no match`). */
  private cases(frame: FlowFrame, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect = this.framePiece(frame, "decision", x, y, scope, dim);
    const cx = x + 1 + FRAME.padX;
    let cy = y + 1 + FRAME.padTop;
    const title = this.title(frame, frameWord(frame.form), cx, cy, false);
    let inner = title.width;
    cy += title.height + FRAME.titleGap;
    const subject = this.testHead(null, frame.header ?? "", frame.headerCalls, cx, cy, scope, dim);
    inner = Math.max(inner, subject.width);
    cy += subject.height;
    const insets = this.chipInsets(frame.headerCalls, cx + SPINE + INSET.indent, cy, scope, dim);
    inner = Math.max(inner, SPINE + INSET.indent + insets.width);
    cy += insets.height + 10;
    const rail = cx + SPINE;
    const railTop = subject.y + subject.height;
    const own = scope.from.get(id) ?? [];
    const lanes = new Set(frame.lanes.map((lane) => lane.id));
    const tick = (top: number, routes: readonly Route[]) => {
      this.wire(
        [
          { x: rail, y: railTop },
          { x: rail, y: top + HEAD_HEIGHT / 2 },
          { x: rail + 12, y: top + HEAD_HEIGHT / 2 },
        ],
        routes,
        null,
      );
    };
    frame.lanes.forEach((lane, index) => {
      cy += index > 0 ? 8 : 0;
      const laneDim = dim || lane.state === "unreachable";
      tick(
        cy,
        own.filter((route) => stepOf(route) === lane.id),
      );
      const head = this.plainHead(lane, "case", headWords(lane), rail + 12, cy, scope, laneDim);
      const into = (scope.from.get(lane.id) ?? []).filter((route) => route.kind === "on");
      this.harrow(head.x + head.width, cy + HEAD_HEIGHT / 2, into, null);
      const laneX = head.x + head.width + HARROW;
      const column = this.lane(lane, laneX, cy, scope, id, null, laneDim);
      inner = Math.max(inner, laneX - cx + column.width);
      cy += Math.max(HEAD_HEIGHT, column.height);
    });
    const unmatched = own.filter((route) => route.kind === "default" && !lanes.has(stepOf(route) ?? 0));
    if (unmatched.length > 0) {
      cy += frame.lanes.length > 0 ? 8 : 0;
      tick(cy, unmatched);
      const head = this.plainHead(null, "none", "no match", rail + 12, cy, scope, dim);
      this.harrow(head.x + head.width, cy + HEAD_HEIGHT / 2, [], null);
      const note = this.note(`↓ continues after the ${frame.form}`, head.x + head.width + HARROW, cy + 7, "faint", dim);
      inner = Math.max(inner, note.x + note.width - cx);
      cy += HEAD_HEIGHT;
    }
    rect.width = Math.ceil(inner + 2 * FRAME.padX + 2);
    rect.height = cy + FRAME.padBottom + 1 - y;
    return { width: rect.width, height: rect.height, box: rect };
  }

  /** A loop: titled by its header; its body (entered through its test when it has one), `↺ next item`, and its `else`. */
  private loop(frame: FlowFrame, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect = this.framePiece(frame, "loop", x, y, scope, dim);
    const cx = x + 1 + FRAME.padX;
    let cy = y + 1 + FRAME.padTop;
    const title = this.title(frame, frameWord(frame.form), cx, cy, true);
    let inner = title.width;
    cy += title.height + FRAME.titleGap;
    const insets = this.chipInsets(frame.headerCalls, cx + INSET.indent, cy, scope, dim);
    inner = Math.max(inner, INSET.indent + insets.width);
    cy += insets.height;
    const sections: MutableRect[] = [];
    frame.lanes.forEach((lane, index) => {
      const laneDim = dim || lane.state === "unreachable";
      if (lane.form === "else") {
        cy += 10;
        const section = this.section(lane, "loop-else", cx, cy, scope, id, laneDim, true);
        sections.push(section.box);
        inner = Math.max(inner, section.width);
        cy += section.height;
        return;
      }
      cy += index > 0 ? 10 : 0;
      const body: MutableRect = { x: cx, y: cy, width: 0, height: 0 };
      this.block(lane.key, body);
      let width: number;
      if (lane.test) {
        const routes = scope.from.get(lane.id) ?? [];
        const head = this.testHead(null, lane.test.text ?? "", lane.test.calls, cx, cy, scope, laneDim);
        this.harrow(
          head.x + head.width,
          cy + HEAD_HEIGHT / 2,
          routes.filter((route) => route.kind === "yes"),
          "yes",
        );
        const laneX = head.x + head.width + HARROW;
        const column = this.lane(lane, laneX, cy, scope, id, null, laneDim);
        width = laneX - cx + column.width;
        cy += Math.max(head.height, column.height);
        const testInsets = this.chipInsets(lane.test.calls, cx + INSET.indent, cy, scope, laneDim);
        width = Math.max(width, INSET.indent + testInsets.width);
        cy += testInsets.height;
        width = Math.max(width, this.note("no: next item", cx, cy + 4, "loop", laneDim).width);
        cy += 4 + NOTE_HEIGHT;
      } else {
        const column = this.lane(lane, cx, cy, scope, id, null, laneDim);
        width = column.width;
        cy += column.height;
      }
      width = Math.max(width, LOOP_BODY_MIN);
      const ids = [...(scope.within.get(lane.id) ?? [])].filter((inside) => inside !== lane.id);
      if (routesFrom(scope, ids).some((route) => route.kind !== "continue" && stepOf(route) === id)) {
        const text = "↺ next item";
        const noteWidth = Math.ceil(this.text(text, FONT.note)) + 2;
        this.note(text, cx + width - noteWidth, cy + 8, "loop", laneDim);
        cy += 8 + NOTE_HEIGHT;
      }
      body.width = width;
      body.height = cy - body.y;
      inner = Math.max(inner, width);
    });
    for (const section of sections) {
      section.width = inner;
    }
    rect.width = Math.ceil(inner + 2 * FRAME.padX + 2);
    rect.height = cy + FRAME.padBottom + 1 - y;
    return { width: rect.width, height: rect.height, box: rect };
  }

  /** A part as a section: its title (and name, clause, or note), then its steps. */
  private section(lane: Lane, look: "try" | "handler" | "else" | "finally" | "loop-else", x: number, y: number, scope: Scope, frame: number, dim: boolean, loop: boolean): Placed {
    const rect: MutableRect = { x, y, width: 0, height: 0 };
    this.pieces.push({ kind: "section", rect, dim, lane, look, loop });
    this.block(lane.key, rect);
    const words = sectionWords(lane, loop);
    let titleWidth = this.caps(words.title, FONT.word, 9.5, 0.09);
    titleWidth += lane.name !== undefined ? 8 + this.text(lane.name, FONT.name) : 0;
    titleWidth += words.clause !== null ? 8 + this.text(words.clause, FONT.head) : 0;
    titleWidth += words.note !== null ? 8 + this.text(words.note, FONT.sectionNote) : 0;
    const top = y + 1 + SECTION.padTop + SECTION.title + SECTION.titleGap;
    const column = this.lane(lane, x + 1 + SECTION.padX, top, scope, frame, look === "handler" ? "continues after the try" : null, dim);
    rect.width = Math.ceil(Math.max(titleWidth, column.width) + 2 * SECTION.padX + 2);
    rect.height = top - y + column.height + SECTION.padBottom + 1;
    return { width: rect.width, height: rect.height, box: rect };
  }

  /** A try: TRY with its handlers beside it (dashed `on error` into each), then ELSE and FINALLY below. */
  private try(frame: FlowFrame, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect = this.framePiece(frame, "try", x, y, scope, dim);
    const cx = x + 11;
    let cy = y + 11;
    const laneDim = (lane: Lane) => dim || lane.state === "unreachable";
    const body = frame.lanes.find((lane) => lane.form === "body");
    const handlers = frame.lanes.filter((lane) => lane.form === "except" || lane.form === "catch");
    const rest = frame.lanes.filter((lane) => lane.form === "else" || lane.form === "finally");
    let inner = 0;
    if (body) {
      const tried = this.section(body, "try", cx, cy, scope, id, laneDim(body), false);
      inner = tried.width;
      let rowHeight = tried.height;
      if (handlers.length > 0) {
        const from = tried.box.x + tried.box.width;
        const column = from + ON_ERROR.width;
        const drop = cy + ON_ERROR.drop;
        let hy = cy;
        const onError = scope.from.get(body.id) ?? [];
        handlers.forEach((handler, index) => {
          hy += index > 0 ? 8 : 0;
          const section = this.section(handler, "handler", column, hy, scope, id, laneDim(handler), false);
          const routes = onError.filter((route) => route.kind === "onError" && stepOf(route) === handler.id);
          const points =
            index === 0
              ? [
                  { x: from, y: drop },
                  { x: column, y: drop },
                ]
              : [
                  { x: from, y: drop },
                  { x: from + ON_ERROR.width / 2, y: drop },
                  { x: from + ON_ERROR.width / 2, y: hy + 20 },
                  { x: column, y: hy + 20 },
                ];
          const label = index === 0 ? { text: "on error", at: { x: from + ON_ERROR.width / 2, y: drop - 5 }, anchor: "middle" as const } : null;
          this.wire(points, routes, label, true);
          inner = Math.max(inner, column + section.width - cx);
          hy += section.height;
        });
        rowHeight = Math.max(rowHeight, hy - cy);
      }
      cy += rowHeight;
    }
    const stretched: MutableRect[] = [];
    for (const lane of rest) {
      cy += 10;
      const section = this.section(lane, lane.form === "else" ? "else" : "finally", cx, cy, scope, id, laneDim(lane), false);
      stretched.push(section.box);
      inner = Math.max(inner, section.width);
      cy += section.height;
    }
    for (const section of stretched) {
      section.width = inner;
    }
    rect.width = Math.ceil(inner + 22);
    rect.height = cy + 11 - y;
    return { width: rect.width, height: rect.height, box: rect };
  }

  /** A with block: titled by its header (and its header calls), then its body. */
  private with(frame: FlowFrame, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect = this.framePiece(frame, "with", x, y, scope, dim);
    const cx = x + 1 + FRAME.padX;
    let cy = y + 1 + FRAME.padTop;
    const title = this.title(frame, frameWord(frame.form), cx, cy, true);
    let inner = title.width;
    cy += title.height + FRAME.titleGap;
    const insets = this.chipInsets(frame.headerCalls, cx + INSET.indent, cy, scope, dim);
    inner = Math.max(inner, INSET.indent + insets.width);
    cy += insets.height;
    frame.lanes.forEach((lane, index) => {
      cy += index > 0 ? 10 : 0;
      const top = cy;
      const column = this.lane(lane, cx, cy, scope, id, null, dim || lane.state === "unreachable");
      this.block(lane.key, { x: cx, y: top, width: column.width, height: column.height });
      inner = Math.max(inner, column.width);
      cy += column.height;
    });
    rect.width = Math.ceil(inner + 2 * FRAME.padX + 2);
    rect.height = cy + FRAME.padBottom + 1 - y;
    return { width: rect.width, height: rect.height, box: rect };
  }

  // -- Combined and proposed steps ------------------------------------------------------------

  private toggleWidth(text: string): number {
    return this.text(text, FONT.toggle) + 18;
  }

  /** A combined step: collapsed, one box with its title, counts, and ways out; open, a frame around its steps. */
  private combined(step: CombinedStep, id: number, x: number, y: number, scope: Scope, dim: boolean): Placed {
    const rect: MutableRect = { x, y, width: 0, height: 0 };
    const routes = scope.from.get(id) ?? [];
    const exits = step.collapsed ? combinedExits(routes, scope.inset) : [];
    this.pieces.push({ kind: "combined", rect, dim, step, exits });
    this.items.set(step.item, rect);
    const glyph = this.text("⊞", FONT.glyph) + 7;
    if (!step.collapsed) {
      const header = glyph + this.text(step.title, FONT.combinedTitle) + 7 + this.toggleWidth("fold");
      const top = y + 1.5 + 6 + COMBINED.head + 8;
      const inner = step.steps ? this.sequence(step.steps, x + 1.5 + 12, top, scope, null) : { width: 0, height: 0 };
      rect.width = Math.ceil(Math.max(header, inner.width) + 24 + 3);
      rect.height = top - y + inner.height + 12 + 1.5;
      return { width: rect.width, height: rect.height, box: rect };
    }
    for (const held of step.holds ?? []) {
      this.block(held, rect);
    }
    const chrome = 1.5 + COMBINED.padLeft + COMBINED.padRight + 1.5;
    const toggle = this.toggleWidth("open");
    const summary = combinedSummary(step);
    const summaryWidths = summary.map((item, index) => this.text(item.text, item.tone === "call" ? FONT.summaryCall : FONT.summary) + (item.tone === "unknown" ? UNKNOWN_MARK : 0) + (index > 0 ? 7 + this.text("·", FONT.summary) + 7 : 0));
    const exitWidths = exits.map((exit) => 18 + this.caps(exit.verb, FONT.routeVerb, 9, 0.07) + 5 + this.text(`→ ${exit.destination}`, FONT.route));
    const titleWidth = this.text(step.title, FONT.combinedTitle);
    const natural = Math.max(glyph + titleWidth + 7 + toggle, sumRow(summaryWidths, 0), sumRow(exitWidths, 4));
    const width = Math.min(COMBINED.max, Math.max(COMBINED.min, Math.ceil(natural + chrome)));
    const inner = width - chrome;
    const titleLines = wrapLines(step.title, inner - glyph - 7 - toggle, FONT.combinedTitle, this.measure);
    let height = 1.5 + COMBINED.padTop + Math.max(COMBINED.head, titleLines * 18);
    if (summary.length > 0) {
      height += 4 + packLines(summaryWidths, inner) * COMBINED.line;
    }
    if (exits.length > 0) {
      height += 4 + 1 + packLines(exitWidths.map((item) => item + 4), inner + 4) * (COMBINED.route + 4) - 4;
    }
    height += COMBINED.padBottom + 1.5;
    rect.width = width;
    rect.height = height;
    for (const call of step.calls) {
      this.anchors.push({ call: call.key, to: call.to, rect, at: { x: rect.x + rect.width, y: y + 1.5 + COMBINED.padTop + COMBINED.head / 2 } });
    }
    // The stacked edge drawn 4px behind it.
    return { width: width + 4, height: height + 4, box: rect };
  }

  private proposed(step: ProposedStep, x: number, y: number, dim: boolean): Placed {
    const kicker = this.caps("Proposed", FONT.kicker, 9, 0.09);
    const title = this.text(step.title, FONT.proposed);
    const inner = Math.min(PROPOSED_MAX - 23, Math.max(kicker, title));
    const lines = wrapLines(step.title, inner, FONT.proposed, this.measure);
    const rect = { x, y, width: Math.ceil(inner) + 23, height: 1.5 + 6 + 14 + 1 + lines * 18 + 7 + 1.5 };
    this.pieces.push({ kind: "proposed", rect, dim, step });
    this.items.set(step.item, rect);
    return { width: rect.width, height: rect.height, box: rect };
  }
}

// ---------------------------------------------------------------------------------------------
// Helpers

/** A wire's words when a loop's `done` is among its routes. */
function doneWords(routes: readonly Route[]): "done" | null {
  return routes.some((route) => route.kind === "done") ? "done" : null;
}

/** The library and unknown calls listed on a box: each, then the calls in its arguments a level deeper. */
function callRows(calls: readonly OtherCall[], depth: number): CallRow[] {
  return calls.flatMap((call) => [{ call, depth }, ...callRows(call.inside, depth + 1)]);
}

/** The calls written in a thin row call's arguments, by name (`+ json.dumps`). */
function insideNames(call: OtherCall): string[] {
  return call.inside.flatMap((inside) => [inside.callee, ...insideNames(inside)]);
}

/** A thin row's count: `5 calls, 2 unknown` (only for two or more calls). */
export function thinCount(row: ThinRow): string | null {
  if (row.calls.length < 2) {
    return null;
  }
  return `${row.calls.length} calls${row.unknown > 0 ? `, ${row.unknown} unknown` : ""}`;
}

/** A callback row's words: its signature, else its label. */
export function callbackText(callback: Callback): string {
  return callback.signature ?? callback.label;
}

/** An inset's place: its file and first line. */
export function insetPlace(flow: DrawnFlow): string {
  return `${flow.header.path}:${flow.header.line}`;
}

function sumRow(widths: readonly number[], gap: number): number {
  return widths.reduce((sum, width) => sum + width, 0) + Math.max(0, widths.length - 1) * gap;
}

/** How many lines items of `widths` fill when packed into `width`. */
function packLines(widths: readonly number[], width: number): number {
  let lines = 1;
  let line = 0;
  for (const item of widths) {
    if (line > 0 && line + item > width) {
      lines += 1;
      line = 0;
    }
    line += item;
  }
  return lines;
}

/** Wraps `text` into lines no wider than `width` (long words break), returning the line count. */
function wrapLines(text: string, width: number, font: string, measure: Measure): number {
  const words = text.split(/\s+/).filter((word) => word.length > 0);
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
