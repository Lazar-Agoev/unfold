import { describe, expect, it } from "vitest";
import type { FileHit, SearchHit, SearchResults } from "../ipc/generated";
import { kindText, resultItems, searchArgs } from "./search";

function hit(name: string, kind: string, score: number): SearchHit {
  return { project: "p-a", key: `src/a.ts#${kind}:${name}`, name, qualifiedName: name, path: "src/a.ts", kind, line: 3, application: null, score, nameMatches: [] };
}

function file(path: string, score: number): FileHit {
  return { project: "p-a", path, name: path.slice(path.lastIndexOf("/") + 1), definitions: 2, score, matches: [] };
}

function results(definitions: SearchHit[], entrances: SearchHit[], files: FileHit[]): SearchResults {
  const group = <T>(hits: T[]) => ({ hits, total: hits.length, truncated: false });
  return {
    query: "order",
    definitions: group(definitions),
    entrances: group(entrances),
    files: group(files),
    projects: [{ project: "p-a", definitions: definitions.length, entrances: entrances.length, files: files.length }],
    tookMs: 1,
  };
}

describe("Add's search", () => {
  it("looks only in the board's project", () => {
    expect(searchArgs("order", "p-a")).toEqual({ query: "order", projects: ["p-a"] });
  });

  it("ranks entrances and definitions together by score, entrances first on a tie, then files", () => {
    const items = resultItems(
      results([hit("place_order", "function", 300), hit("Order", "class", 500)], [hit("Place an order", "http", 300)], [file("tests/test_orders.py", 900)]),
    );
    expect(items.map((item) => [item.type, item.hit.name])).toEqual([
      ["definition", "Order"],
      ["entrance", "Place an order"],
      ["definition", "place_order"],
      ["file", "test_orders.py"],
    ]);
  });

  it("names each result's kind: entrance, file, or the definition's kind", () => {
    const items = resultItems(results([hit("save_order", "function", 2), hit("Cart", "interface", 2), hit("Total", "type_alias", 1)], [hit("orders", "worker", 3)], [file("src/orders.ts", 1)]));
    expect(items.map(kindText)).toEqual(["entrance", "function", "interface", "type", "file"]);
  });
});
