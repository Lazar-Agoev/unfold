/** How an entrance row reads, on Home's How it starts and in Add's map and results. */

import type { EntranceRow } from "../ipc/generated";

/** The method column: the HTTP method, or the kind for other entrances (`DELETE`, `COMMAND`). */
export function methodColumn(row: Pick<EntranceRow, "method" | "kind">): string {
  return row.method ?? row.kind.toUpperCase();
}

/** The trigger as people read it: the label, the trigger (`/orders…` when partial), or "path unknown". */
export function triggerText(row: Pick<EntranceRow, "label" | "trigger" | "triggerStatus" | "kind">): string {
  if (row.label) {
    return row.label;
  }
  if (row.trigger !== null && row.triggerStatus !== "unknown") {
    return row.triggerStatus === "partial" ? `${row.trigger}…` : row.trigger;
  }
  return row.kind === "http" ? "path unknown" : "trigger unknown";
}
