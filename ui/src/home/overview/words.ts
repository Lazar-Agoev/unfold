/** How it starts in words: the summary line and a scope's overflow. */

import type { CatalogCounts, CatalogRow } from "../../ipc";
import { formatCount, plural } from "../../lib/format";

/** The line beside the section's title, from the catalog's totals. */
export function summaryLine(counts: CatalogCounts, compiling: boolean): string {
  if (counts.launches > 0) {
    return `${plural(counts.applications, "application")} · ${plural(counts.launches, "launch", "launches")} · ${plural(counts.routes, "route")}. Hover a row to trace it on a new board.`;
  }
  if (counts.routes > 0) {
    return `${plural(counts.routes, "route")}, but no launches yet, so Unfold can't tell which service serves them.`;
  }
  return compiling ? "Entrances show up here as the compile finds them." : "No launches or routes yet.";
}

/** "2 more routes" when every row shown past the launches is a route, else "2 more entrances". */
export function moreWords(more: number, shown: readonly Pick<CatalogRow, "kind">[]): string {
  const routes = shown.every((row) => row.kind === "http" || row.kind === "process");
  return routes ? `${formatCount(more)} more ${more === 1 ? "route" : "routes"}` : `${formatCount(more)} more ${more === 1 ? "entrance" : "entrances"}`;
}
