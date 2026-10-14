import { describe, expect, it } from "vitest";
import type { SearchHit, SearchResults } from "../ipc/generated";
import { resultItems, searchArgs } from "./search";

function hit(name: string, kind: string, score: number): SearchHit {
  const start = kind === "app" || kind === "entrance";
  return {
    project: "p-a",
    key: `key-${name}`,
    name,
    qualifiedName: start ? "Shop API · POST /orders" : name,
    path: start ? "" : "shop/service.py",
    kind,
    line: start ? null : 3,
    app: start ? "Shop API" : null,
    score,
  };
}

function results(definitions: SearchHit[], entrances: SearchHit[]): SearchResults {
  const group = <T>(hits: T[]) => ({ hits, total: hits.length, truncated: false });
  return { query: "order", definitions: group(definitions), entrances: group(entrances), files: group([]) };
}

describe("Add's search", () => {
  it("looks for definitions, types, and entrances in the board's project, not files yet", () => {
    expect(searchArgs("order", "p-a")).toEqual({ query: "order", kinds: ["definitions", "entrances"], projects: ["p-a"] });
  });

  it("ranks entrances and definitions together by score, entrances first on a tie", () => {
    const items = resultItems(results([hit("place_order", "function", 300), hit("Order", "class", 500)], [hit("Place an order", "entrance", 300)]));
    expect(items.map((item) => [item.hit.kind, item.hit.name])).toEqual([
      ["class", "Order"],
      ["entrance", "Place an order"],
      ["function", "place_order"],
    ]);
  });

  it("adds a definition as a block card and an app or entrance as a start card", () => {
    const items = resultItems(results([hit("Cart", "type", 2)], [hit("Shop API", "app", 3), hit("Place an order", "entrance", 1)]));
    expect(items.map((item) => item.ref)).toEqual([{ entrance: "key-Shop API" }, { block: "key-Cart" }, { entrance: "key-Place an order" }]);
  });
});
