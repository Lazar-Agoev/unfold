import { describe, expect, it } from "vitest";
import type { FileHit, SearchHit, SearchResults } from "../ipc/generated";
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

function file(path: string): FileHit {
  return { project: "p-a", path, name: path.replace(/^.*\//, ""), key: path, definitions: 2, score: 900 };
}

function results(definitions: SearchHit[], entrances: SearchHit[], files: FileHit[] = []): SearchResults {
  const group = <T>(hits: T[]) => ({ hits, total: hits.length, truncated: false });
  return { query: "order", definitions: group(definitions), entrances: group(entrances), files: group(files) };
}

describe("Add's search", () => {
  it("looks for definitions, types, entrances, and files in the board's project", () => {
    expect(searchArgs("order", "p-a")).toEqual({ query: "order", kinds: ["definitions", "entrances", "files"], projects: ["p-a"] });
  });

  it("ranks entrances and definitions together by score, entrances first on a tie", () => {
    const items = resultItems(results([hit("place_order", "function", 300), hit("Order", "class", 500)], [hit("Place an order", "entrance", 300)]));
    expect(items.map((item) => [item.kind === "card" ? item.hit.kind : "file", item.hit.name])).toEqual([
      ["class", "Order"],
      ["entrance", "Place an order"],
      ["function", "place_order"],
    ]);
  });

  it("adds a definition as a block card and an app or entrance as a start card", () => {
    const items = resultItems(results([hit("Cart", "type", 2)], [hit("Shop API", "app", 3), hit("Place an order", "entrance", 1)]));
    expect(items.map((item) => (item.kind === "card" ? item.ref : null))).toEqual([{ entrance: "key-Shop API" }, { block: "key-Cart" }, { entrance: "key-Place an order" }]);
  });

  it("lists files after the names, whatever their score", () => {
    const items = resultItems(results([hit("place_order", "function", 1)], [], [file("tests/test_orders.py")]));
    expect(items.map((item) => [item.kind, item.id])).toEqual([
      ["card", "definition:key-place_order"],
      ["file", "file:tests/test_orders.py"],
    ]);
  });
});
