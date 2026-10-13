/**
 * Calls expanded in place on an expanded function card in Diagram. The card saves which call
 * occurrences are expanded (`Card.expanded`); this module loads the models they reveal through the
 * card query (the same cache as the callee's own card), lays them out from the inside out
 * (`flow/insets`), and draws each inset: a title row (the chevron, `ƒ`, the callee's name, which opens
 * its definition in the peek sheet (View code), its `file:line`, and `→`, which opens it as its own
 * card), a "via …" caption for a one-call callback, then the callee's written Flow, where calls can
 * expand again, three levels at most.
 *
 * Labels are the page's for the card's project; step groups come only from the callee's own card on the
 * page. A step selected in an inset carries its owner (the block whose Flow shows it), and its `</>`
 * and menu open the callee's own source; it is never part of a run to combine. Nothing in them changes
 * the caller.
 *
 * An inset's steps carry the ports of the calls they show (`insetPort`, one set per inset, so several
 * calls of one function stay apart) and, in a callback shown as itself, of the connections leaving
 * its steps (`placeFlowPorts`); their leaders reach the card's edge.
 */

import { useQueries, useQueryClient, type QueryState } from "@tanstack/react-query";
import { useMemo, type MouseEvent, type ReactNode } from "react";
import { viewCode } from "../../board/viewCode";
import { shortPath } from "../../code/text";
import { isWithin, portStep } from "../../code/outline";
import { stepGroups } from "../../flow/groups";
import { FlowView, flowPortNodes, type FlowExpansion } from "../../flow/FlowView";
import { DIAGRAM_PAD, type FlowLayout, type Measure, type PlacedInset } from "../../flow/layout";
import { INSET_CHROME, expansionState, layoutInsets, planInsets, type ExpandableCall, type InsetModel, type InsetPlan, type LaidInset } from "../../flow/insets";
import { Icon } from "../../icons";
import type { Card, CardView, FlowModel, Group, Key, ProjectId } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { MAX_EXPANSION_DEPTH, editPage, findCardByKey, findPage, toggleCallExpansion, useOpenBoard, type PageTarget } from "../../state/boards";
import { insetPortOf } from "../arrows";
import { cardQuery, isMissing } from "../cardData";
import { CardNote, renderPorts } from "./parts";
import { clickCard } from "./select";

const NO_PATHS: readonly (readonly Key[])[] = [];
const LOADING: InsetModel = { state: "loading" };
const NO_PORTS: ReadonlyMap<string, readonly string[]> = new Map();


/** A card query's state as an inset reads it. */
function modelOf(data: CardView | undefined, error: unknown): InsetModel {
  if (data !== undefined) {
    return { state: "ready", view: data };
  }
  if (error !== null && error !== undefined) {
    return { state: "failed", missing: isMissing(error), message: toUnfoldError(error).message };
  }
  return LOADING;
}

/** The blocks planned insets show, outermost first. */
function ownersOf(plans: readonly InsetPlan[], into: Key[] = []): Key[] {
  for (const plan of plans) {
    if (!into.includes(plan.owner)) {
      into.push(plan.owner);
    }
    ownersOf(plan.children, into);
  }
  return into;
}

export interface CallInsetsInput {
  readonly target: PageTarget;
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  /** The card's own Flow while it shows it, else null (no insets). */
  readonly flow: FlowModel | null;
  /** The page's labels for the card's project. */
  readonly labels: Readonly<Record<string, string>>;
  readonly openChains: ReadonlySet<string>;
  /** The page's groups, and the ids of its broken ones. */
  readonly groups: readonly Group[] | undefined;
  readonly broken: ReadonlySet<string>;
  readonly measure: Measure;
  /** The widest the card's own diagram may be. */
  readonly maxWidth: number;
}

/**
 * The card's calls expanded in place, laid out (none until its Flow shows). Each level's models load as
 * the level above them is known: subscribing to what the cache lacks redraws the card when it arrives.
 */
export function useCallInsets(input: CallInsetsInput): readonly LaidInset[] {
  const { target, project, card, flow, labels, openChains, groups, broken, measure, maxWidth } = input;
  const own = card.ref.block;
  const paths = card.expanded ?? NO_PATHS;
  const client = useQueryClient();
  // What the expanded calls need, from what is cached now.
  const wanted: Key[] = [];
  const cached = (key: Key): InsetModel => {
    if (!wanted.includes(key)) {
      wanted.push(key);
    }
    const state: QueryState<CardView> | undefined = client.getQueryState(cardQuery(project, key).queryKey);
    return modelOf(state?.data, state?.error);
  };
  const owners = flow ? ownersOf(planInsets(flow, own, paths, cached, MAX_EXPANSION_DEPTH)) : [];
  const loaded = useQueries({
    queries: wanted.map((key) => cardQuery(project, key)),
    // Structurally shared: it keeps its identity while no model changed.
    combine: (results) => ({ data: results.map((result) => result.data), errors: results.map((result) => result.error) }),
  });
  // The callee's own card on the page (same project) lends its step groups.
  const ownerCards = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return owners.map((owner) => (page ? (findCardByKey(page, owner)?.id ?? "") : "")).join("\u0000");
  });
  const wantedKey = wanted.join("\u0000");
  const ownersKey = owners.join("\u0000");
  return useMemo(() => {
    if (!flow || paths.length === 0) {
      return [];
    }
    const keys = wantedKey.split("\u0000");
    const byKey = new Map(keys.map((key, index) => [key, modelOf(loaded.data[index], loaded.errors[index])]));
    const cardOf = new Map(ownersKey.split("\u0000").map((owner, index) => [owner, (ownerCards ?? "").split("\u0000")[index] ?? ""]));
    const plans = planInsets(flow, own, paths, (key) => byKey.get(key) ?? LOADING, MAX_EXPANSION_DEPTH);
    return layoutInsets(
      plans,
      (plan) => {
        const view = plan.model.state === "ready" && plan.model.view.type === "block" ? plan.model.view : undefined;
        const outline = view?.code?.source.state === "current" ? view.code.source.outline : null;
        const ownerCard = cardOf.get(plan.owner) ?? "";
        const lent = outline && ownerCard !== "" ? (groups ?? []).filter((group) => group.card === ownerCard && !broken.has(group.id)) : [];
        return { labels, openChains, groups: outline ? stepGroups(lent, outline) : [], measure };
      },
      maxWidth - DIAGRAM_PAD * 2,
    );
  }, [flow, own, paths, wantedKey, loaded, ownersKey, ownerCards, groups, broken, labels, openChains, measure, maxWidth]);
}

/** The insets drawn with a Flow, outermost first. */
function drawnInsets(insets: readonly LaidInset[], into: LaidInset[] = []): LaidInset[] {
  for (const inset of insets) {
    if (inset.layout) {
      into.push(inset);
      drawnInsets(inset.children, into);
    }
  }
  return into;
}

/** The key of an inset's ports: its path. */
function insetPortsKey(path: readonly Key[]): string {
  return path.join("\u0000");
}

/** Where a card's Flow places its ports: by node of its own Flow and of each inset (by path), and the rest at its header. */
export interface FlowPorts {
  readonly byNode: ReadonlyMap<string, readonly string[]>;
  readonly insets: ReadonlyMap<string, ReadonlyMap<string, readonly string[]>>;
  readonly unplaced: readonly string[];
}

/**
 * Places a card's ports (`card` is its block): a call shown in an inset at its step there; a
 * connection's step inside a callback shown as itself, in that inset; the rest in the card's own
 * Flow (`ownerOf` names the callback that owns an occurrence). What no Flow shows goes to the header.
 */
export function placeFlowPorts(card: Key, layout: FlowLayout, insets: readonly LaidInset[], ports: readonly string[], ownerOf: (occurrence: string) => string): FlowPorts {
  const drawn = drawnInsets(insets);
  const own = new Map<string, string>();
  // By inset: each port's id in that inset's Flow, its owner there, and the card's port id.
  const inner = new Map<LaidInset, { owners: Map<string, string>; ids: Map<string, string> }>();
  const unplaced: string[] = [];
  const put = (inset: LaidInset, id: string, port: string, owner: string) => {
    const entry = inner.get(inset) ?? { owners: new Map<string, string>(), ids: new Map<string, string>() };
    entry.owners.set(id, owner);
    entry.ids.set(id, port);
    inner.set(inset, entry);
  };
  for (const port of ports) {
    const at = insetPortOf(port);
    if (at) {
      const inset = drawn.find((item) => item.plan.path.length === at.path.length && item.plan.path.every((key, index) => at.path[index] === key));
      const view = inset?.plan.model.state === "ready" && inset.plan.model.view.type === "block" ? inset.plan.model.view : undefined;
      const links = view?.code?.source.state === "current" ? view.code.source.links : [];
      if (inset) {
        put(inset, at.port, port, links.find((link) => link.source === at.port)?.owner ?? "");
      } else {
        unplaced.push(port);
      }
      continue;
    }
    const step = portStep(port);
    let holder: LaidInset | undefined;
    for (const inset of drawn) {
      const { owner, path } = inset.plan;
      if (step !== undefined && isWithin(owner, card) && isWithin(step, owner) && (!holder || path.length > holder.plan.path.length)) {
        holder = inset;
      }
    }
    if (holder) {
      put(holder, port, port, "");
    } else {
      own.set(port, ownerOf(port));
    }
  }
  const placed = flowPortNodes(layout, own);
  const byInset = new Map<string, Map<string, string[]>>();
  for (const [{ plan, layout: shown }, { owners, ids }] of inner) {
    if (!shown) {
      continue;
    }
    const there = flowPortNodes(shown, owners);
    byInset.set(insetPortsKey(plan.path), new Map([...there.byNode].map(([node, list]) => [node, list.map((id) => ids.get(id) ?? id)])));
    unplaced.push(...there.unplaced.map((id) => ids.get(id) ?? id));
  }
  return { byNode: placed.byNode, insets: byInset, unplaced: [...placed.unplaced, ...unplaced] };
}

/** Where a Flow's ports reach: its diagram's width, and how far past it to the card's edge. */
export interface PortFrame {
  readonly width: number;
  readonly reach: number;
}

/** What every inset of one card shares: the card, its expansions, the selection, and the actions. */
export interface InsetContext {
  readonly target: PageTarget;
  readonly project: ProjectId;
  readonly cardId: string;
  readonly paths: readonly (readonly Key[])[];
  /** The step selected in an inset of this card (its key and owner), if any. */
  readonly selected: { readonly key: Key; readonly owner: Key } | null;
  readonly selectedGroup: string | null;
  /** Opens a block as its own card, from this card (`followCall`). */
  readonly follow: (targetKey: Key, anchor: HTMLElement) => void;
  readonly onToggleChain: (key: string) => void;
  readonly onSelectGroup: (groupId: string) => void;
  readonly onToggleGroup: (groupId: string) => void;
  /** Selects step `key` of `owner`'s Flow and opens its source (its `</>`). */
  readonly onViewStep: (key: Key, owner: Key) => void;
  /** Selects a combined block and opens its source. */
  readonly onViewGroup: (groupId: string) => void;
  /** Opens the menu of the selection a step's right-click left. */
  readonly onStepMenu: (at: MouseEvent | Element) => void;
  /** Each inset's ports by node (`placeFlowPorts`), by `insetPortsKey`. */
  readonly ports: ReadonlyMap<string, ReadonlyMap<string, readonly string[]>>;
}

/**
 * The chevrons of one Flow (the card's own at `prefix` [], or an inset's) and the insets under its nodes;
 * `frame` is where that Flow's ports reach.
 */
export function flowExpansion(context: InsetContext, prefix: readonly Key[], lineage: readonly Key[], insets: readonly LaidInset[], frame: PortFrame): FlowExpansion {
  return {
    stateOf: (call: ExpandableCall) => expansionState(call, prefix, lineage, context.paths, MAX_EXPANSION_DEPTH),
    onToggle: (call: ExpandableCall) => {
      editPage(context.target, (page) => {
        toggleCallExpansion(page, context.cardId, [...prefix, call.key]);
      });
    },
    renderInset: (placed: PlacedInset) =>
      insets
        .filter((inset) => inset.plan.node === placed.key)
        .map((inset) => <CallInset key={insetPortsKey(inset.plan.path)} inset={inset} placed={placed} frame={frame} context={context} />),
  };
}

/** One call expanded in place: its title row, its caption, and the callee's own Flow. */
function CallInset({ inset, placed, frame, context }: { readonly inset: LaidInset; readonly placed: PlacedInset; readonly frame: PortFrame; readonly context: InsetContext }) {
  const { plan, layout, flow } = inset;
  const view = plan.model.state === "ready" && plan.model.view.type === "block" ? plan.model.view : undefined;
  const header = view?.header;
  const name = header?.qualifiedName ?? plan.name;
  const location = header ? `${shortPath(header.path)}:${header.startLine}` : "";
  const collapse = () => {
    editPage(context.target, (page) => {
      toggleCallExpansion(page, context.cardId, plan.path);
    });
  };
  const stop = (event: MouseEvent) => {
    event.stopPropagation();
  };
  let body: ReactNode;
  if (flow && layout) {
    // From this Flow's right edge to the card's: past the room the parent kept, and the parent's own reach.
    const reach = frame.reach + Math.max(0, frame.width - placed.x - INSET_CHROME.rule - layout.width);
    body = (
      <FlowView
        inset
        model={flow}
        layout={layout}
        width={layout.width}
        selectedKey={context.selected?.owner === plan.owner ? context.selected.key : null}
        onSelect={(key) => {
          clickCard(context.target, context.cardId, key, plan.owner);
        }}
        onFollow={(_node, targetKey, anchor) => {
          context.follow(targetKey, anchor);
        }}
        onOpenCallback={(callback, anchor) => {
          context.follow(callback.key, anchor);
        }}
        onToggleChain={context.onToggleChain}
        selectedGroup={context.selectedGroup}
        onSelectGroup={context.onSelectGroup}
        onToggleGroup={context.onToggleGroup}
        onViewCode={(key) => {
          context.onViewStep(key, plan.owner);
        }}
        onViewGroupCode={context.onViewGroup}
        onStepMenu={(_key, at) => {
          context.onStepMenu(at);
        }}
        ports={context.ports.get(insetPortsKey(plan.path)) ?? NO_PORTS}
        renderPorts={renderPorts}
        portReach={reach}
        expansion={flowExpansion(context, plan.path, plan.lineage, inset.children, { width: layout.width, reach })}
      />
    );
  } else {
    body = <div className="flow-inset-note">{insetNote(plan.model)}</div>;
  }
  return (
    <section className="flow-inset nodrag" style={{ width: inset.width, height: inset.height }} aria-label={`${name}, expanded in place`} data-inset={plan.owner}>
      <div className="flow-inset-title" onClick={stop}>
        <button
          type="button"
          className="flow-call-toggle"
          aria-label={`Collapse ${plan.via ?? plan.name} here`}
          aria-expanded
          onClick={(event) => {
            event.stopPropagation();
            collapse();
          }}
        >
          <Icon name="chevron-down-small" size={10} />
        </button>
        <span className="flow-inset-glyph" aria-hidden="true">
          ƒ
        </span>
        <button
          type="button"
          className="flow-inset-name"
          title={header ? `Open ${name}'s definition` : name}
          disabled={!header}
          onClick={(event) => {
            event.stopPropagation();
            if (header) {
              // The inset's block (a callee's or a callback's definition), as View code finds it.
              viewCode(context.target, { kind: "card", cardId: context.cardId, key: plan.owner, owner: plan.owner });
            }
          }}
        >
          {name}
        </button>
        <span className="flow-inset-location" title={header ? `${header.path}:${header.startLine}` : undefined}>
          {location}
        </span>
        <button
          type="button"
          className="flow-inset-follow"
          aria-label={`Open ${name} as its own card`}
          title={`Open ${name} as its own card`}
          onClick={(event) => {
            event.stopPropagation();
            context.follow(plan.owner, event.currentTarget);
          }}
        >
          →
        </button>
      </div>
      {plan.via !== undefined ? <div className="flow-inset-via">via {plan.via}</div> : null}
      {body}
    </section>
  );
}

/** Why an inset shows no Flow: loading, gone from the graph, or its source unavailable or changed. */
function insetNote(model: InsetModel): ReactNode {
  if (model.state === "loading") {
    return <CardNote>Loading…</CardNote>;
  }
  if (model.state === "failed") {
    return <CardNote warn>{model.missing ? "This reference is missing from the graph." : model.message}</CardNote>;
  }
  const view = model.view.type === "block" ? model.view : undefined;
  if (view?.flow?.source === "changed" || view?.code?.source.state === "changed") {
    return <CardNote warn>{view.header.fileState === "last_good" ? "This file doesn't parse right now." : "The source changed · updating."}</CardNote>;
  }
  return <CardNote warn>Source unavailable.</CardNote>;
}

