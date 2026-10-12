/**
 * Calls expanded in place in an expanded function card (docs/proposals/unified-code-diagrams.md §3):
 * which calls of a Flow node can expand (its main call when it resolved to a target, and each
 * callback), what an expanded one shows, and how big each inset is.
 *
 * - A card saves paths of keys, outermost first: at each level the main call's occurrence key or the
 *   callback's key, in the block shown at that level. Several calls of one function stay distinct.
 * - A call shows the callee's written Flow. A callback shows its own definition's Flow, unless it is a
 *   one-call callback (its one step makes one call, resolved to a target) whose callee isn't shown above:
 *   then the proven callee's, captioned "via <callback>", never as a direct call.
 * - At most `maxDepth` levels; a call to the card's own block or to an inset above it has no chevron
 *   (its follow link stays), so nothing repeats.
 *
 * Models come from `lookup` (the card loads them); this module only reads them and lays them out.
 */

import type { CardView, FlowCallback, FlowChip, FlowModel, FlowNode, Key } from "../ipc/generated";
import { DIAGRAM_PAD, INSET_GAP, followableCall, layoutFlow, type FlowLayout, type FlowLayoutOptions, type InsetSize } from "./layout";

/** A call or callback a Flow node can expand in place. */
export interface ExpandableCall {
  /** What the card saves in its path: the main call's occurrence key, or the callback's key. */
  readonly key: Key;
  /** The block it reveals first: the callee, or the callback's definition. */
  readonly target: Key;
  /** Its name for the chevron: the callee's qualified name, or the callback's label. */
  readonly name: string;
  /** For a callback: the callback. */
  readonly callback?: FlowCallback;
}

/**
 * The calls of a node that can expand in place: its main call when it resolved to a target, then each
 * callback. Start, End, and conditions show neither link, so none.
 */
export function expandableCalls(node: FlowNode): ExpandableCall[] {
  if (node.kind === "start" || node.kind === "end" || node.kind === "condition") {
    return [];
  }
  const calls: ExpandableCall[] = [];
  const main = followableCall(node);
  if (main?.target !== undefined) {
    calls.push({ key: main.source, target: main.target, name: main.name });
  }
  for (const callback of node.callbacks ?? []) {
    calls.push({ key: callback.key, target: callback.key, name: callback.label, callback });
  }
  return calls;
}

/** The kinds of the one step a one-call callback has: one that runs on to the callback's end or returns. */
const ONE_CALL_STEPS: ReadonlySet<FlowNode["kind"]> = new Set(["call", "statement", "variable", "return"]);

/**
 * The proven callee of a one-call callback: its Flow has one step, and the one call or construction
 * that step makes resolved to a target. An expression-bodied callback (`() => sendEmail(job.data)`)
 * has one statement step whose one call is that call.
 */
export function oneCallTarget(flow: Pick<FlowModel, "source" | "nodes">): FlowChip | undefined {
  const steps = flow.nodes.filter((node) => node.kind !== "start" && node.kind !== "end");
  const [only, ...others] = steps;
  if (flow.source !== "current" || !only || others.length > 0 || !ONE_CALL_STEPS.has(only.kind)) {
    return undefined;
  }
  const [call, ...more] = (only.chips ?? []).filter((chip) => chip.kind === "calls" || chip.kind === "constructs");
  return call && more.length === 0 && call.resolution === "resolved" && call.target !== undefined ? call : undefined;
}

/**
 * The callee an expanded callback shows instead of its own Flow (`flow`): a one-call callback's proven
 * callee, unless that callee already shows above (in `lineage`), so the repeated ancestor stays
 * unexpanded. Undefined when the callback shows its own Flow. The canvas and MCP's `set_view` share it.
 */
export function callbackCallee(flow: Pick<FlowModel, "source" | "nodes"> | null | undefined, lineage: readonly Key[]): FlowChip | undefined {
  const proven = flow ? oneCallTarget(flow) : undefined;
  return proven?.target !== undefined && !lineage.includes(proven.target) ? proven : undefined;
}

/** Whether `path` starts with `prefix`. */
function startsWith(path: readonly Key[], prefix: readonly Key[]): boolean {
  return prefix.every((key, index) => path[index] === key);
}

/**
 * Whether `call`, in the Flow shown at `prefix` (the path of the inset showing it; empty for the card's
 * own), shows a chevron, and whether it is expanded: null at the deepest level, and for a call to a
 * block in `lineage` (the card's own block and the blocks of the insets above).
 */
export function expansionState(
  call: ExpandableCall,
  prefix: readonly Key[],
  lineage: readonly Key[],
  paths: readonly (readonly Key[])[],
  maxDepth: number,
): "expanded" | "collapsed" | null {
  if (prefix.length >= maxDepth || lineage.includes(call.target)) {
    return null;
  }
  const expanded = paths.some((path) => path.length === prefix.length + 1 && path[prefix.length] === call.key && startsWith(path, prefix));
  return expanded ? "expanded" : "collapsed";
}

/** What a model lookup answers: still loading, failed (`missing`: the key is gone from the graph), or the model. */
export type InsetModel =
  | { readonly state: "loading" }
  | { readonly state: "failed"; readonly missing: boolean; readonly message: string }
  | { readonly state: "ready"; readonly view: CardView };

/** A call expanded in place, resolved against the models loaded so far. */
export interface InsetPlan {
  /** Its saved path, outermost first. */
  readonly path: readonly Key[];
  /** The key of the node it hangs below, in the Flow that shows it. */
  readonly node: Key;
  /** The block whose Flow it shows: a step selected in it belongs to this block. */
  readonly owner: Key;
  /** The card's own block, then the block of each inset down to this one (`expansionState`). */
  readonly lineage: readonly Key[];
  /** Its name until the owner's header loads: the callee's qualified name, or the callback's label. */
  readonly name: string;
  /** For a one-call callback shown as its proven callee: the callback's label (its "via" caption). */
  readonly via?: string;
  readonly model: InsetModel;
  /** The calls expanded inside it, in the order of their nodes. */
  readonly children: readonly InsetPlan[];
}

/** The model's Flow when it can be drawn: a block's, from current source. */
export function drawnFlow(model: InsetModel): FlowModel | null {
  const view = model.state === "ready" && model.view.type === "block" ? model.view : undefined;
  return view?.flow?.source === "current" ? view.flow : null;
}

function planLevel(
  flow: FlowModel,
  prefix: readonly Key[],
  lineage: readonly Key[],
  paths: readonly (readonly Key[])[],
  lookup: (key: Key) => InsetModel,
  maxDepth: number,
): InsetPlan[] {
  const plans: InsetPlan[] = [];
  for (const node of flow.nodes) {
    if (node.key === undefined) {
      continue;
    }
    for (const call of expandableCalls(node)) {
      if (expansionState(call, prefix, lineage, paths, maxDepth) !== "expanded") {
        continue;
      }
      const path = [...prefix, call.key];
      const first = lookup(call.target);
      let owner = call.target;
      let name = call.name;
      let via: string | undefined;
      let model = first;
      // A one-call callback shows its proven callee, unless that callee is already shown above.
      const proven = call.callback && first.state === "ready" && first.view.type === "block" ? callbackCallee(first.view.flow, lineage) : undefined;
      if (call.callback && proven?.target !== undefined) {
        owner = proven.target;
        name = proven.name;
        via = call.callback.label;
        model = lookup(proven.target);
      }
      const inner = drawnFlow(model);
      const ownLineage = [...lineage, owner];
      plans.push({
        path,
        node: node.key,
        owner,
        lineage: ownLineage,
        name,
        ...(via !== undefined ? { via } : {}),
        model,
        children: inner ? planLevel(inner, path, ownLineage, paths, lookup, maxDepth) : [],
      });
    }
  }
  return plans;
}

/**
 * The calls a card expands in place in its own Flow (`own` is the card's block), each with the ones
 * expanded inside it, resolved with `lookup`. A path whose call is gone, unresolved, or repeated is
 * left out (the card keeps it until someone collapses it).
 */
export function planInsets(
  flow: FlowModel,
  own: Key,
  paths: readonly (readonly Key[])[],
  lookup: (key: Key) => InsetModel,
  maxDepth: number,
): InsetPlan[] {
  return paths.length > 0 ? planLevel(flow, [], [own], paths, lookup, maxDepth) : [];
}

/** An inset's chrome (flow.css `.flow-inset`): its left rule, title row, "via" caption, the note it shows without a Flow, and its least width. */
export const INSET_CHROME = { rule: 2, title: 26, caption: 16, note: 36, minWidth: 240 } as const;

/** A planned inset laid out: its own Flow's layout (null while there is none to draw) and its size. */
export interface LaidInset {
  readonly plan: InsetPlan;
  readonly flow: FlowModel | null;
  readonly layout: FlowLayout | null;
  readonly children: readonly LaidInset[];
  readonly width: number;
  readonly height: number;
}

/** What an inset's Flow is laid out with, besides its insets, its width, and being bare. */
export type InsetLayoutOptions = Omit<FlowLayoutOptions, "insets" | "maxWidth" | "bare">;

/** The room each node keeps for its insets: several under one node stack, `INSET_GAP` apart. */
export function insetRoom(insets: readonly Pick<LaidInset, "plan" | "width" | "height">[]): Map<Key, InsetSize> {
  const room = new Map<Key, InsetSize>();
  for (const inset of insets) {
    const before = room.get(inset.plan.node);
    room.set(
      inset.plan.node,
      before ? { width: Math.max(before.width, inset.width), height: before.height + INSET_GAP + inset.height } : { width: inset.width, height: inset.height },
    );
  }
  return room;
}

/**
 * Lays out planned insets from the inside out, each no wider than `maxWidth`: its own Flow (bare, with
 * room for the insets inside it), under its title and caption, or a note while there is none.
 */
export function layoutInsets(plans: readonly InsetPlan[], options: (plan: InsetPlan) => InsetLayoutOptions, maxWidth: number): LaidInset[] {
  return plans.map((plan) => {
    const content = maxWidth - INSET_CHROME.rule;
    const children = layoutInsets(plan.children, options, content - DIAGRAM_PAD * 2);
    const flow = drawnFlow(plan.model);
    const layout = flow ? layoutFlow(flow, { ...options(plan), insets: insetRoom(children), bare: true, maxWidth: content }) : null;
    const width = Math.max(INSET_CHROME.minWidth, INSET_CHROME.rule + (layout?.width ?? 0));
    const height = INSET_CHROME.title + (plan.via !== undefined ? INSET_CHROME.caption : 0) + (layout ? layout.height : INSET_CHROME.note);
    return { plan, flow, layout, children, width, height };
  });
}
