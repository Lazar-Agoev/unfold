/**
 * Add's search: names in the board's project (entrances, definitions, and files) as one ranked list.
 * Entrances and definitions share the engine's score and merge by it, entrances first on a tie; files,
 * which the engine ranks by their path alone, follow.
 */

import { declarationWord } from "../code/typeKinds";
import type { FileHit, ProjectId, SearchArgs, SearchHit, SearchResults } from "../ipc/generated";

export type ResultItem =
  | { readonly type: "entrance"; readonly id: string; readonly hit: SearchHit }
  | { readonly type: "definition"; readonly id: string; readonly hit: SearchHit }
  | { readonly type: "file"; readonly id: string; readonly hit: FileHit };

/** The `search` arguments for a query in one project. */
export function searchArgs(query: string, project: ProjectId): SearchArgs {
  return { query, projects: [project] };
}

/** The results as one list, best first. */
export function resultItems(results: SearchResults): ResultItem[] {
  const names: ResultItem[] = [
    ...results.entrances.hits.map((hit) => ({ type: "entrance" as const, id: `entrance:${hit.key}`, hit })),
    ...results.definitions.hits.map((hit) => ({ type: "definition" as const, id: `definition:${hit.key}`, hit })),
  ].sort((a, b) => b.hit.score - a.hit.score);
  const files = results.files.hits.map((hit): ResultItem => ({ type: "file", id: `file:${hit.path}`, hit }));
  return [...names, ...files];
}

/** A result's kind as its row shows it: `entrance`, `file`, or a definition's kind (`function`, `class`, `interface`, `type`). */
export function kindText(item: ResultItem): string {
  switch (item.type) {
    case "entrance":
      return "entrance";
    case "definition":
      return declarationWord(item.hit.kind) ?? item.hit.kind;
    case "file":
      return "file";
  }
}
