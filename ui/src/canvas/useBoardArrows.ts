/**
 * The card models of a page (from the query cache), the definitions of its type cards that show their
 * fields, and the arrows derived from them, including those of the calls expanded cards show in place.
 */

import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { cardBody, type CardBody } from "../code/typeKinds";
import { drawnFlow, planInsets, type InsetModel, type InsetPlan } from "../flow/insets";
import type { Card, CardView, Key, ProjectId, TypeDefinition } from "../ipc/generated";
import { engineQueryOptions } from "../ipc/queries";
import { MAX_EXPANSION_DEPTH, cardKey, cardRefKey, findPage, shownView, useOpenBoard, usePageView, type PageTarget } from "../state/boards";
import { computeArrows, type Arrow, type ArrowCard, type ArrowInset } from "./arrows";
import { cardQuery, lastVerifiedCode } from "./cardData";

export interface BoardArrows {
  /** Loaded models by `cardRefKey(project, key)`. */
  readonly models: ReadonlyMap<string, CardView>;
  readonly arrows: readonly Arrow[];
  /** What each card's body shows, by card id (`cardBody`). */
  readonly bodies: ReadonlyMap<string, CardBody>;
  /** Whether every card's model has loaded (or failed). */
  readonly settled: boolean;
}

/** Loaded results by reference key, keeping only the ones that loaded. */
function loadedByRef<T>(wanted: readonly (readonly [string, unknown])[], data: readonly (T | undefined)[]): Map<string, T> {
  const map = new Map<string, T>();
  wanted.forEach(([ref], position) => {
    const value = data[position];
    if (value !== undefined) {
      map.set(ref, value);
    }
  });
  return map;
}

const LOADING: InsetModel = { state: "loading" };

/** The drawn insets of plans, outermost first, with the blocks they show. */
function drawnInsets(plans: readonly InsetPlan[], into: ArrowInset[] = []): ArrowInset[] {
  for (const plan of plans) {
    if (drawnFlow(plan.model)) {
      into.push({ path: plan.path, owner: plan.owner });
      drawnInsets(plan.children, into);
    }
  }
  return into;
}

/**
 * The calls each expanded function card shows expanded in place (`useCallInsets` draws them), with the
 * models they show, by card id. Each level's models load as the level above is known, from the cache.
 */
function useInsets(cards: readonly Card[], arrowCards: readonly ArrowCard[], models: ReadonlyMap<string, CardView>) {
  const client = useQueryClient();
  const expanding = useMemo(
    () =>
      arrowCards.flatMap((arrowCard, index) => {
        const card = cards[index];
        const paths = card?.expanded ?? [];
        const own = models.get(cardRefKey(arrowCard.project, arrowCard.key));
        const flow = own?.type === "block" && own.flow?.source === "current" ? own.flow : null;
        return arrowCard.body === "flow" && card?.folded !== true && paths.length > 0 && flow
          ? [{ id: arrowCard.id, project: arrowCard.project, key: arrowCard.key, flow, paths }]
          : [];
      }),
    [cards, arrowCards, models],
  );
  // What the expanded calls need, from what is cached now.
  const wanted: { project: ProjectId; key: Key }[] = [];
  for (const { project, key, flow, paths } of expanding) {
    planInsets(flow, key, paths, (callee) => {
      if (!wanted.some((item) => item.project === project && item.key === callee)) {
        wanted.push({ project, key: callee });
      }
      const data = client.getQueryData<CardView>(cardQuery(project, callee).queryKey);
      return data ? { state: "ready", view: data } : LOADING;
    }, MAX_EXPANSION_DEPTH);
  }
  const loaded = useQueries({
    queries: wanted.map(({ project, key }) => cardQuery(project, key)),
    // Structurally shared: `data` keeps its identity while no model changed.
    combine: (results) => results.map((result) => result.data),
  });
  const wantedKey = JSON.stringify(wanted.map(({ project, key }) => cardRefKey(project, key)));
  return useMemo(() => {
    const byRef = new Map<string, CardView>();
    (JSON.parse(wantedKey) as string[]).forEach((ref, index) => {
      const data = loaded[index];
      if (data) {
        byRef.set(ref, data);
      }
    });
    const insets = new Map<string, ArrowInset[]>();
    for (const { id, project, key, flow, paths } of expanding) {
      const plans = planInsets(flow, key, paths, (callee) => {
        const data = byRef.get(cardRefKey(project, callee));
        return data ? { state: "ready", view: data } : LOADING;
      }, MAX_EXPANSION_DEPTH);
      insets.set(id, drawnInsets(plans));
    }
    return { insets, models: byRef };
  }, [expanding, wantedKey, loaded]);
}

/** The arrows of a page; its cards are of its board's project, and each shows its own view. */
export function useBoardArrows(target: PageTarget, cards: readonly Card[]): BoardArrows {
  const boardProject = target.board.project;
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const arrowCards = useMemo(
    (): ArrowCard[] =>
      cards.map((card) => {
        const key = cardKey(card);
        return {
          id: card.id,
          key,
          project: boardProject,
          entrance: "entrance" in card.ref,
          body: cardBody(key, shownView(lens, card, defaultView)),
        };
      }),
    [cards, boardProject, lens, defaultView],
  );
  const wanted = useMemo(() => {
    const byRef = new Map<string, { project: ProjectId; key: string; fields: boolean }>();
    for (const card of arrowCards) {
      const ref = cardRefKey(card.project, card.key);
      const fields = card.body === "fields" || byRef.get(ref)?.fields === true;
      byRef.set(ref, { project: card.project, key: card.key, fields });
    }
    return [...byRef];
  }, [arrowCards]);
  const types = useMemo(() => wanted.filter(([, entry]) => entry.fields), [wanted]);
  const loaded = useQueries({
    queries: wanted.map(([, { project, key }]) => cardQuery(project, key)),
    // Structurally shared: `data` keeps its identity while no model changed.
    combine: (results) => ({ data: results.map((result) => result.data), settled: results.every((result) => !result.isPending) }),
  });
  const loadedTypes = useQueries({
    queries: types.map(([, { project, key }]) => engineQueryOptions("type_definition", { project, key })),
    combine: (results) => results.map((result) => result.data),
  });
  const models = useMemo(() => loadedByRef<CardView>(wanted, loaded.data), [wanted, loaded.data]);
  const definitions = useMemo(() => loadedByRef<TypeDefinition>(types, loadedTypes), [types, loadedTypes]);
  const inline = useInsets(cards, arrowCards, models);
  const arrows = useMemo(() => {
    const withInsets = arrowCards.map((card) => {
      const insets = inline.insets.get(card.id);
      return insets && insets.length > 0 ? { ...card, insets } : card;
    });
    return computeArrows(withInsets, new Map([...inline.models, ...models]), lastVerifiedCode, definitions);
  }, [arrowCards, models, inline, definitions]);
  const bodies = useMemo(() => new Map(arrowCards.map((card) => [card.id, card.body])), [arrowCards]);
  return { models, arrows, bodies, settled: loaded.settled };
}
