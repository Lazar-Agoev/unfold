/**
 * A row of Add's list: the result's kind in small gray text, its name, and where it is: a
 * definition's `file:line`, an entrance's application and trigger. A file's path is its name.
 */

import { kindText, type ResultItem } from "./search";

/** A result's accessible name: `function OrderService.place, src/orders/service.ts`. */
export function resultText(item: ResultItem): string {
  switch (item.type) {
    case "entrance":
      return `entrance ${item.hit.name}`;
    case "definition":
      return `${kindText(item)} ${item.hit.qualifiedName}, ${item.hit.path}`;
    case "file":
      return `file ${item.hit.path}`;
  }
}

function Name({ item }: { readonly item: ResultItem }) {
  switch (item.type) {
    case "entrance":
      return <span className="add-row-name ui">{item.hit.name}</span>;
    case "definition":
      return <span className="add-row-name">{item.hit.qualifiedName}</span>;
    case "file":
      return <span className="add-row-name">{item.hit.path}</span>;
  }
}

function Meta({ item }: { readonly item: ResultItem }) {
  switch (item.type) {
    case "entrance": {
      const text = [item.hit.application, item.hit.qualifiedName].filter((part) => part !== null && part !== "").join(" · ");
      return text === "" ? null : <span className="add-row-meta">{text}</span>;
    }
    case "definition":
      return <span className="add-row-meta code">{item.hit.line === null ? item.hit.path : `${item.hit.path}:${item.hit.line}`}</span>;
    case "file":
      return null;
  }
}

export function ResultRow({ item }: { readonly item: ResultItem }) {
  return (
    <div className="add-row">
      <span className="add-row-kind">{kindText(item)}</span>
      <Name item={item} />
      <Meta item={item} />
    </div>
  );
}
