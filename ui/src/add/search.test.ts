import { describe, expect, it } from "vitest";
import type { SearchHit, SearchResults, TextSearchFile } from "../ipc/generated";
import {
  allSearchArgs,
  definitionColumn,
  definitionText,
  entranceColumn,
  kindCounts,
  kindSearchArgs,
  moreLabel,
  projectsWithResults,
  resultGroups,
  trimmedMatch,
  type TextResults,
} from "./search";

function hit(project: string, name: string, kind = "function", qualifiedName = name): SearchHit {
  return { project, key: `src/a.ts#function:${name}`, name, qualifiedName, path: "src/a.ts", kind, line: 3, application: null, score: 1, nameMatches: [] };
}

function results(definitions: SearchHit[], total = definitions.length): SearchResults {
  return {
    query: "refund",
    definitions: { hits: definitions, total, truncated: total > definitions.length },
    entrances: { hits: [hit("p-b", "refund", "worker")], total: 1, truncated: false },
    files: { hits: [], total: 0, truncated: false },
    projects: [
      { project: "p-a", definitions: total, entrances: 0, files: 0 },
      { project: "p-b", definitions: 0, entrances: 1, files: 0 },
      { project: "p-c", definitions: 0, entrances: 0, files: 0 },
    ],
    tookMs: 1,
  };
}

const FILE: TextSearchFile = {
  project: "p-c",
  path: "docs/payments.md",
  matches: [
    { line: 12, text: "    Partial refunds are capped", column: 0, ranges: [[12, 18]] },
    { line: 14, text: "refund", column: 0, ranges: [[0, 6]] },
  ],
};
const TEXT: TextResults = { files: [FILE], matches: 27, truncated: false };

describe("resultGroups", () => {
  const names = results(["processRefund", "issueStripeRefund", "refund_order", "useRefundStatus", "refundWorker", "markRefunded"].map((name) => hit("p-a", name)), 9);

  it("shows four rows of each kind on All, then a row that switches to the kind's tab", () => {
    const groups = resultGroups("all", names, undefined, TEXT);
    expect(groups.map((group) => group.label)).toEqual(["Entrances", "Functions", "Text"]);
    const functions = groups[1]?.items ?? [];
    expect(functions.map((item) => item.type)).toEqual(["function", "function", "function", "function", "more"]);
    expect(functions[4]).toMatchObject({ kind: "functions", label: "5 more functions" });
    expect(groups[2]?.items.at(-1)).toMatchObject({ type: "more", label: "25 more matches" });
  });

  it("shows every result of one kind on its tab, from its own search once loaded", () => {
    const own = results(["a", "b", "c", "d", "e", "f", "g", "h", "i"].map((name) => hit("p-a", name)));
    expect(resultGroups("functions", names, own, TEXT).map((group) => [group.id, group.items.length])).toEqual([["functions", 9]]);
    expect(resultGroups("functions", names, undefined, TEXT)[0]?.items).toHaveLength(6);
    expect(resultGroups("text", names, undefined, TEXT).map((group) => [group.id, group.items.length])).toEqual([["text", 2]]);
  });

  it("counts each kind and the projects with any result", () => {
    expect(kindCounts(names, TEXT)).toEqual({ entrances: 1, functions: 9, files: 0, text: 27 });
    expect(kindCounts(undefined, null)).toEqual({ entrances: null, functions: null, files: null, text: null });
    expect(projectsWithResults(names, TEXT)).toBe(3);
  });
});

describe("search arguments", () => {
  it("asks 50 per kind for All and 500 of one kind on its tab", () => {
    expect(allSearchArgs("refund")).toEqual({ query: "refund", limit: 50 });
    expect(kindSearchArgs("refund", "functions")).toEqual({ query: "refund", kinds: ["definitions"], limit: 500 });
    expect(kindSearchArgs("refund", "all")).toBeNull();
    expect(kindSearchArgs("refund", "text")).toBeNull();
  });
});

describe("row text", () => {
  it("names a definition by its kind: functions as FN, type declarations as TYPE with their word", () => {
    expect(definitionColumn("function")).toBe("FN");
    expect(definitionColumn("method")).toBe("METHOD");
    expect(definitionColumn("class")).toBe("CLASS");
    expect(["interface", "type_alias", "enum"].map(definitionColumn)).toEqual(["TYPE", "TYPE", "TYPE"]);
    expect(definitionText({ kind: "function", qualifiedName: "createOrder", path: "src/orders.ts" })).toBe("Function createOrder, src/orders.ts");
    expect(definitionText({ kind: "interface", qualifiedName: "Cart", path: "src/cart.ts" })).toBe("Interface Cart, src/cart.ts");
    expect(definitionText({ kind: "type_alias", qualifiedName: "CartId", path: "src/cart.ts" })).toBe("Type CartId, src/cart.ts");
    expect(definitionText({ kind: "enum", qualifiedName: "Currency", path: "src/cart.ts" })).toBe("Enum Currency, src/cart.ts");
  });

  it("names an HTTP entrance by its method and others by their kind", () => {
    expect(entranceColumn({ kind: "http", qualifiedName: "POST /orders createOrder" })).toBe("POST");
    expect(entranceColumn({ kind: "worker", qualifiedName: "refund processRefund" })).toBe("WORKER");
  });

  it("drops a match line's indentation and moves its ranges along", () => {
    expect(trimmedMatch(FILE.matches[0] ?? { line: 0, text: "", column: 0, ranges: [] })).toEqual({ text: "Partial refunds are capped", ranges: [[8, 14]] });
    const long = "x".repeat(40) + "refund";
    expect(trimmedMatch({ line: 1, text: `  ${long}`, column: 0, ranges: [[42, 48]] })).toEqual({ text: `…${"x".repeat(16)}refund`, ranges: [[17, 23]] });
    expect(trimmedMatch({ line: 1, text: "refund", column: 900, ranges: [[0, 6]] })).toEqual({ text: "…refund", ranges: [[1, 7]] });
    expect(moreLabel("files", 1)).toBe("1 more file");
    expect(moreLabel("text", 2000, true)).toBe("2,000+ more matches");
  });
});
