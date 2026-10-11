/** How an entrance row reads, on Home's How it starts and in Add's entrance browser. */

import type { CatalogRow } from "../ipc/generated";

/** The method column: the HTTP method, or the kind for other entrances (`DELETE`, `COMMAND`). */
export function methodColumn(row: Pick<CatalogRow, "method" | "kind">): string {
  return row.method ?? row.kind.toUpperCase();
}
