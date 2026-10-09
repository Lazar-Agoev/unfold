import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import type { CommandArgs, CommandName, Events } from "./generated";
import { ancestorDirs, applyFilesChanged, applyGraphUpdated, clearFolderQueries, engineKey, keyFile } from "./queries";

let client: QueryClient;

function seed<C extends CommandName>(command: C, args: CommandArgs<C>): void {
  client.setQueryData(engineKey(command, args), {});
}

/** Which seeded queries are now stale, as `command:arg`. */
function stale(): string[] {
  return client
    .getQueryCache()
    .getAll()
    .filter((query) => query.state.isInvalidated)
    .map((query) => {
      const [, command, args] = query.queryKey as [string, string, Record<string, unknown>];
      const detail = args["path"] ?? args["key"] ?? args["query"];
      return `${command}:${typeof detail === "string" ? detail : ""}`;
    })
    .sort();
}

const UPDATED: Events["graph_updated"] = {
  type: "graph_updated",
  folder: "/f",
  changedFiles: ["src/routes/orders.ts"],
  removedFiles: [],
  changedKeys: ["src/routes/orders.ts#function:createOrder"],
  changedEntrances: [],
  inputsChanged: false,
  everything: false,
  complete: true,
};

beforeEach(() => {
  client = new QueryClient();
  seed("list_dir", { path: "" });
  seed("list_dir", { path: "src" });
  seed("list_dir", { path: "src/routes" });
  seed("list_dir", { path: "lib" });
  seed("file_view", { path: "src/routes/orders.ts" });
  seed("file_view", { path: "src/other.ts" });
  seed("file_outline", { path: "src/routes/orders.ts" });
  seed("file_outline", { path: "src/other.ts" });
  seed("card", { key: "src/routes/orders.ts#function:listOrders" });
  seed("card", { key: "src/other.ts#function:x" });
  seed("card", { key: "entrance:http:GET /orders" });
  seed("peek", { key: "src/other.ts#function:x" });
  seed("entrance_tree", {});
  seed("relations", { key: "src/other.ts#function:x", onBoard: [] });
  seed("search", { query: "a" });
  seed("search", { query: "a", kinds: ["definitions"] });
  seed("index_log", {});
  seed("recent_folders", {});
  seed("get_preferences", {});
});

describe("graph_updated", () => {
  it("invalidates only what the changed files and keys touch", () => {
    applyGraphUpdated(client, UPDATED);
    expect(stale()).toEqual([
      "card:entrance:http:GET /orders",
      "card:src/routes/orders.ts#function:listOrders",
      "file_outline:src/routes/orders.ts",
      "file_view:src/routes/orders.ts",
      "index_log:",
      "list_dir:",
      "list_dir:src",
      "list_dir:src/routes",
      "relations:src/other.ts#function:x",
      "search:a",
      "search:a",
    ]);
  });

  it("refreshes the entrance tree when entrances or inputs change, and everything when told", () => {
    applyGraphUpdated(client, { ...UPDATED, changedFiles: [], changedKeys: [], inputsChanged: true });
    expect(stale()).toContain("entrance_tree:");
    applyGraphUpdated(client, { ...UPDATED, everything: true });
    expect(stale()).not.toContain("recent_folders:");
    expect(stale()).toContain("card:src/other.ts#function:x");
  });
});

describe("files_changed", () => {
  it("invalidates the changed files' views, cards, tree levels, and file search", () => {
    applyFilesChanged(client, { type: "files_changed", folder: "/f", paths: ["src/other.ts"], truncated: false });
    expect(stale()).toEqual(["card:src/other.ts#function:x", "file_view:src/other.ts", "list_dir:", "list_dir:src", "search:a"]);
  });
});

describe("helpers", () => {
  it("knows a path's tree levels and a key's file, and clears folder data on a folder change", () => {
    expect(ancestorDirs("a/b/c.ts")).toEqual(["", "a", "a/b"]);
    expect(keyFile("src/a.ts#class:A/method:b")).toBe("src/a.ts");
    clearFolderQueries(client);
    expect(
      client
        .getQueryCache()
        .getAll()
        .map((query) => query.queryKey[1]),
    ).toEqual(["recent_folders", "get_preferences"]);
  });
});
