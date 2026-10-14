/**
 * A row of Add's list: the result's kind in small gray text, its name, and where it is: a
 * definition's `file:line`, an entrance's app and what starts it, an app's command.
 */

import type { ResultItem } from "./search";

/** A result's accessible name: `function OrderService.place, src/orders/service.ts`. */
export function resultText({ hit }: ResultItem): string {
  return hit.path === "" ? `${hit.kind} ${hit.name}` : `${hit.kind} ${hit.qualifiedName}, ${hit.path}`;
}

export function ResultRow({ item }: { readonly item: ResultItem }) {
  const { hit } = item;
  const start = hit.path === "";
  const where = start ? hit.qualifiedName : hit.line === null ? hit.path : `${hit.path}:${hit.line}`;
  return (
    <div className="add-row">
      <span className="add-row-kind">{hit.kind}</span>
      <span className={start ? "add-row-name ui" : "add-row-name"}>{start ? hit.name : hit.qualifiedName}</span>
      {where === "" ? null : <span className={start ? "add-row-meta" : "add-row-meta code"}>{where}</span>}
    </div>
  );
}
