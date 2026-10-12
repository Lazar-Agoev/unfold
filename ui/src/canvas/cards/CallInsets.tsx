/**
 * Calls expanded in place on an expanded function card in Diagram (docs/proposals/unified-code-diagrams.md
 * §3). The card saves which call occurrences are expanded (`Card.expanded`); this module loads the
 * models they reveal through the card query (the same cache as the callee's own card), lays them out
 * from the inside out (`flow/insets`), and draws each inset: a title row (the chevron, `ƒ`, the callee's
 * name, which opens its definition in the peek sheet (View code), its `file:line`, and `→`, which opens
 * it as its own card), a "via …" caption for a one-call callback, then the callee's written Flow, where
 * calls can expand again, three levels at most.
 *
 * Labels are the page's for the card's project; step groups come only from the callee's own card on the
 * page. A step selected in an inset carries its owner (the block whose Flow shows it), and its `</>`
 * and menu open the callee's own source; it is never part of a run to combine. Insets create no ports,
 * and nothing in them changes the caller.
 */

import { useQueries, useQueryClient, type QueryState } from "@tanstack/react-query";
import { useMemo, type MouseEvent, type ReactNode } from "react";
import { definitionSource, showSource } from "../../board/viewCode";
import { shortPath } from "../../code/text";
import { stepGroups } from "../../flow/groups";
import { FlowView, type FlowExpansion } from "../../flow/FlowView";
import { DIAGRAM_PAD, type Measure, type PlacedInset } from "../../flow/layout";
import { expansionState, layoutInsets, planInsets, type ExpandableCall, type InsetModel, type InsetPlan, type LaidInset } from "../../flow/insets";
import { Icon } from "../../icons";
import type { Card, CardView, FlowModel, Group, Key, ProjectId } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { MAX_EXPANSION_DEPTH, editPage, findCardByRef, findPage, toggleCallExpansion, useOpenBoard, type PageTarget } from "../../state/boards";
import { cardQuery, isMissing } from "../cardData";
import { CardNote } from "./parts";
import { clickCard } from "./select";

const NO_PATHS: readonly (readonly Key[])[] = [];
const LOADING: InsetModel = { state: "loading" };

/** Opens an inset's block (a callee's or a callback's definition) in the peek sheet, from the page `target`. */
function openInsetSource(target: PageTarget, project: ProjectId, header: { readonly path: string; readonly startLine: number }): void {
  showSource(target, definitionSource(project, header));
}

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
    return owners.map((owner) => (page ? (findCardByRef(page, owner, card.ref.project)?.id ?? "") : "")).join("\u0000");
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
}

/** The chevrons of one Flow (the card's own at `prefix` [], or an inset's) and the insets under its nodes. */
export function flowExpansion(context: InsetContext, prefix: readonly Key[], lineage: readonly Key[], insets: readonly LaidInset[]): FlowExpansion {
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
        .map((inset) => <CallInset key={inset.plan.path.join("\u0000")} inset={inset} context={context} />),
  };
}

/** One call expanded in place: its title row, its caption, and the callee's own Flow. */
function CallInset({ inset, context }: { readonly inset: LaidInset; readonly context: InsetContext }) {
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
        expansion={flowExpansion(context, plan.path, plan.lineage, inset.children)}
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
              openInsetSource(context.target, context.project, header);
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

