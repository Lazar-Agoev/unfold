/**
 * Add's search: names in the board's project (modules, definitions and types, apps and entrances) as
 * one ranked list, best first by the engine's score, entrances first on a tie; then files, which the
 * engine ranks by their path alone. Each row's kind is the engine's.
 */

import type { CardRef, FileHit, ProjectId, SearchArgs, SearchHit, SearchResults } from "../ipc/generated";

/** A result: a name and the card it adds, or a file, which opens in the code panel. */
export type ResultItem =
  | { readonly kind: "card"; readonly id: string; readonly hit: SearchHit; readonly ref: CardRef }
  | { readonly kind: "file"; readonly id: string; readonly hit: FileHit };

/** The `search` arguments for a query in one project. */
export function searchArgs(query: string, project: ProjectId): SearchArgs {
  return { query, kinds: ["definitions", "entrances", "files"], projects: [project] };
}

/** The results as one list, best first. */
export function resultItems(results: SearchResults): ResultItem[] {
  const names: ResultItem[] = [
    ...results.entrances.hits.map((hit) => ({ kind: "card" as const, id: `entrance:${hit.key}`, hit, ref: { entrance: hit.key } })),
    ...results.definitions.hits.map((hit) => ({ kind: "card" as const, id: `definition:${hit.key}`, hit, ref: { block: hit.key } })),
  ].sort((a, b) => b.hit.score - a.hit.score);
  const files = results.files.hits.map((hit): ResultItem => ({ kind: "file", id: `file:${hit.path}`, hit }));
  return [...names, ...files];
}
