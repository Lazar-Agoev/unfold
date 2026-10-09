/** The card models of a page (from the query cache) and the arrows and ports derived from them. */

import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import type { Card, CardView, ProjectId } from "../ipc/generated";
import { cardKey, cardRefKey } from "../state/boards";
import { resolveRefProject, useProjectIndex } from "../state/workspace";
import { computeArrows, portsByCard, type Arrow, type ArrowCard } from "./arrows";
import { cardQuery, lastVerifiedCode } from "./cardData";

export interface BoardArrows {
  /** Loaded models by `cardRefKey(project, key)`. */
  readonly models: ReadonlyMap<string, CardView>;
  readonly arrows: readonly Arrow[];
  readonly ports: ReadonlyMap<string, readonly string[]>;
  /** Whether every card's model has loaded (or failed). */
  readonly settled: boolean;
}

/** The arrows of a page of a board of `boardProject`; its cards resolve by shared name (D-3). */
export function useBoardArrows(boardProject: ProjectId, cards: readonly Card[]): BoardArrows {
  const index = useProjectIndex();
  const arrowCards = useMemo(
    (): ArrowCard[] =>
      cards.map((card) => ({
        id: card.id,
        key: cardKey(card),
        project: resolveRefProject(index, boardProject, card.ref.project),
        entrance: "entrance" in card.ref,
      })),
    [cards, index, boardProject],
  );
  const wanted = useMemo(() => {
    const byRef = new Map<string, { project: ProjectId; key: string }>();
    for (const card of arrowCards) {
      if (card.project !== null) {
        byRef.set(cardRefKey(card.project, card.key), { project: card.project, key: card.key });
      }
    }
    return [...byRef];
  }, [arrowCards]);
  const loaded = useQueries({
    queries: wanted.map(([, { project, key }]) => cardQuery(project, key)),
    // Structurally shared: `data` keeps its identity while no model changed.
    combine: (results) => ({ data: results.map((result) => result.data), settled: results.every((result) => !result.isPending) }),
  });
  const models = useMemo(() => {
    const map = new Map<string, CardView>();
    wanted.forEach(([ref], position) => {
      const data = loaded.data[position];
      if (data) {
        map.set(ref, data);
      }
    });
    return map;
  }, [wanted, loaded.data]);
  const arrows = useMemo(() => computeArrows(arrowCards, models, lastVerifiedCode), [arrowCards, models]);
  const ports = useMemo(() => portsByCard(arrows), [arrows]);
  return { models, arrows, ports, settled: loaded.settled };
}
