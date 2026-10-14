/**
 * Add's search: names in the board's project (modules, definitions and types, apps and entrances) as
 * one ranked list, best first by the engine's score, entrances first on a tie. Each row's kind is the
 * engine's.
 */

import type { CardRef, ProjectId, SearchArgs, SearchHit, SearchResults } from "../ipc/generated";

/** A result: its hit, and the card it adds. */
export interface ResultItem {
  readonly id: string;
  readonly hit: SearchHit;
  readonly ref: CardRef;
}

/** The `search` arguments for a query in one project. */
export function searchArgs(query: string, project: ProjectId): SearchArgs {
  return { query, kinds: ["definitions", "entrances"], projects: [project] };
}

/** The results as one list, best first. */
export function resultItems(results: SearchResults): ResultItem[] {
  return [
    ...results.entrances.hits.map((hit) => ({ id: `entrance:${hit.key}`, hit, ref: { entrance: hit.key } })),
    ...results.definitions.hits.map((hit) => ({ id: `definition:${hit.key}`, hit, ref: { block: hit.key } })),
  ].sort((a, b) => b.hit.score - a.hit.score);
}
