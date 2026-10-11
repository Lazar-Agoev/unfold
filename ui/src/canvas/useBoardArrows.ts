/**
 * The card models of a page (from the query cache), the definitions of its type cards that show their
 * fields, and the arrows derived from them.
 */

import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import { cardBody, type CardBody } from "../code/typeKinds";
import type { Card, CardView, ProjectId, TypeDefinition } from "../ipc/generated";
import { engineQueryOptions } from "../ipc/queries";
import { cardKey, cardRefKey, effectiveView, findPage, useOpenBoard, usePageView, type PageTarget } from "../state/boards";
import { resolveRefProject, useProjectIndex } from "../state/workspace";
import { computeArrows, type Arrow, type ArrowCard } from "./arrows";
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

/** The arrows of a page; its cards resolve by shared name (shared project names), and each shows its own view. */
export function useBoardArrows(target: PageTarget, cards: readonly Card[]): BoardArrows {
  const index = useProjectIndex();
  const boardProject = target.board.project;
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const exceptions = usePageView(target, (view) => view.lensExceptions);
  const arrowCards = useMemo(
    (): ArrowCard[] =>
      cards.map((card) => {
        const key = cardKey(card);
        return {
          id: card.id,
          key,
          project: resolveRefProject(index, boardProject, card.ref.project),
          entrance: "entrance" in card.ref,
          body: cardBody(key, effectiveView(lens, card.view, defaultView, exceptions.includes(card.id))),
        };
      }),
    [cards, index, boardProject, lens, exceptions, defaultView],
  );
  const wanted = useMemo(() => {
    const byRef = new Map<string, { project: ProjectId; key: string; fields: boolean }>();
    for (const card of arrowCards) {
      if (card.project !== null) {
        const ref = cardRefKey(card.project, card.key);
        const fields = card.body === "fields" || byRef.get(ref)?.fields === true;
        byRef.set(ref, { project: card.project, key: card.key, fields });
      }
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
  const arrows = useMemo(() => computeArrows(arrowCards, models, lastVerifiedCode, definitions), [arrowCards, models, definitions]);
  const bodies = useMemo(() => new Map(arrowCards.map((card) => [card.id, card.body])), [arrowCards]);
  return { models, arrows, bodies, settled: loaded.settled };
}
