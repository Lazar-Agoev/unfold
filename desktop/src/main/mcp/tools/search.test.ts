import { describe, expect, it, vi } from "vitest";
import entrances from "../../../../../testdata/workspace/commands/entrances.json";
import fileViewPage from "../../../../../testdata/workspace/commands/file_view_orders_page.json";
import searchPlace from "../../../../../testdata/workspace/commands/search_place.json";
import searchWorker from "../../../../../testdata/workspace/commands/search_worker.json";
import type { FileView, SearchResults, TextSearchFile } from "../engine-types";
import { listEntrances, search } from "./search";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const NONE = { hits: [], total: 0, truncated: false };

/** The engine's name search: the recorded answer for its query, narrowed to the kinds asked. */
function names(args: Readonly<Record<string, unknown>>): SearchResults {
  const found = recorded<SearchResults>(args["query"] === "place" ? searchPlace : searchWorker);
  const kinds = args["kinds"] as string[];
  const types = (args["definitionKinds"] as string[] | undefined)?.includes("interface") === true;
  return {
    ...found,
    definitions: kinds.includes("definitions") && !types ? found.definitions : NONE,
    entrances: kinds.includes("entrances") ? found.entrances : NONE,
    files: kinds.includes("files") ? found.files : NONE,
  };
}

describe("search", () => {
  it("finds entrances, functions, types, and files by name, each with its key", async () => {
    const app = harness({ engine: { search: names } });
    const { text } = await app.call(search, { query: "worker" });
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders · as Claude",
      "Entrances (4):",
      `- worker (process, worker) declared in package.json: key entrance:["package.script","process","","worker","src/worker.ts"]`,
      `- emails (worker, worker) declared in src/queues/email-worker.ts: key entrance:["bullmq.worker","worker","","emails","(…)"]`,
      `- orders (worker, worker) declared in src/queues/orders-worker.ts: key entrance:["bullmq.worker","worker","","orders","processOrder"]`,
      `- new URL("../../report-processor.js", import.meta.url) (worker, worker) declared in src/queues/report-worker.ts: key entrance:["bullmq.worker","worker","","","new URL(\\"../../report-processor.js\\", import.meta.url)"]`,
      "Files (4):",
      "- src/worker.ts: 1 definition",
      "- src/queues/email-worker.ts: 1 definition",
      "- src/queues/orders-worker.ts: 0 definitions",
      "- src/queues/report-worker.ts: 0 definitions",
      "Next: read_code or get_relations with a key; add_cards puts it on a page.",
    ]);
    const asked = app.engineCalls.filter(([command]) => command === "search").map(([, args]) => args);
    expect(asked).toEqual([
      { query: "worker", kinds: ["definitions"], definitionKinds: ["function", "method", "constructor", "getter", "setter"], limit: 10, projects: [PROJECT] },
      { query: "worker", kinds: ["definitions"], definitionKinds: ["interface", "type_alias", "enum", "class"], limit: 10, projects: [PROJECT] },
      { query: "worker", kinds: ["entrances", "files"], limit: 10, projects: [PROJECT] },
    ]);
    const place = await app.call(search, { query: "place", kinds: ["functions"] });
    expect(place.text).toContain("Functions (1):\n- OrderService.place (method) at src/services/order-service.ts:13: key src/services/order-service.ts#class:OrderService/method:place");
  });

  it("pages every kind with one cursor", async () => {
    const app = harness({ engine: { search: names } });
    const first = await app.call(search, { query: "worker", kinds: ["entrances"], limit: 3 });
    expect(first.text).toContain("Entrances (1–3 of 4):");
    expect(first.text).toContain('More: search with cursor "3" for the next page.');
    const second = await app.call(search, { query: "worker", kinds: ["entrances"], limit: 3, cursor: "3" });
    expect(second.text).toContain("Entrances (4–4 of 4):\n- new URL");
    expect(second.text).not.toContain("More:");
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ limit: 6 });
    expect((await app.call(search, { query: "worker", cursor: "soon" })).text).toBe(
      "“soon” isn't a cursor of this list. Leave cursor out for the first page, or pass the next cursor a result gave.",
    );
  });

  it("finds text, naming the function each line is in", async () => {
    const files: TextSearchFile[] = [
      { project: PROJECT, path: "src/routes/orders.ts", matches: [{ line: 12, text: "  res.json(await repo.list());", column: 0, ranges: [[6, 10]] }] },
      { project: PROJECT, path: "src/changed.ts", matches: [{ line: 3, text: "repo.list()", column: 0, ranges: [[0, 4]] }] },
      { project: PROJECT, path: "README.md", matches: [{ line: 9, text: "Call repo.list for orders.", column: 0, ranges: [[5, 9]] }] },
    ];
    const app = harness({
      engine: {
        search_text: (_args, stream) => {
          files.forEach(stream);
          return { files: 3, matches: 3, searched: 40, truncated: false, filesTruncated: false, projects: [] };
        },
        file_view: (args) => {
          const view = recorded<FileView>(fileViewPage);
          return args["path"] === "src/routes/orders.ts" ? view : { ...view, graph: args["path"] === "src/changed.ts" ? "not_compiled" : "not_indexed", outline: [] };
        },
      },
    });
    const { text } = await app.call(search, { query: "repo.list", kinds: ["text"], caseSensitive: true });
    expect(text.split("\n").slice(1)).toEqual([
      "Text in 3 files (3):",
      "- src/routes/orders.ts:12 in function listOrders (key src/routes/orders.ts#function:listOrders): res.json(await repo.list());",
      "- src/changed.ts:3 (the file changed since the last compile: compile to name its function): repo.list()",
      "- README.md:9: Call repo.list for orders.",
      "Next: read_code or get_relations with a key; add_cards puts it on a page.",
    ]);
    expect(app.engineCalls.find(([command]) => command === "search_text")?.[1]).toEqual({ query: "repo.list", projects: [PROJECT], caseSensitive: true });
    expect(app.engineCalls.find(([command]) => command === "file_view")?.[1]).toEqual({ project: PROJECT, path: "src/routes/orders.ts", range: { startLine: 12, endLine: 12 } });
  });

  it("narrows to projects by name, says when nothing matches, and keeps text options for text", async () => {
    const app = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-python0000", "py-orders")],
      engine: { search: (args) => ({ ...names(args), definitions: NONE, entrances: NONE, files: NONE }) },
    });
    const nothing = await app.call(search, { query: "zzz", projects: ["py-orders"] });
    expect(nothing.text).toBe('Unfold · py-orders · as Claude\nNothing matches “zzz”. Try fewer or shorter words, or kinds ["text"].');
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ projects: ["p-python0000"] });
    expect((await app.call(search, { query: "x", regex: true })).text).toBe('Can\'t search: regex, caseSensitive, and wholeWord apply to text only. Add "text" to kinds, or leave them out.');
  });

  it("fits the largest page under the size guard, keeping its cursor, and reaches every hit across pages", async () => {
    const deep = `src/${"very-long-folder-name/".repeat(8)}module.ts`;
    const many = (kind: string, count: number) =>
      Array.from({ length: count }, (_, index) => ({ project: PROJECT, key: `${deep}#class:Service${index}/method:${kind}${index}`, name: `${kind}${index}`, qualifiedName: `Service${index}.${kind}${index}`, path: deep, kind: "method", line: index + 1, application: null, score: 1, nameMatches: [] }));
    const group = (kind: string) => ({ hits: many(kind, 120), total: 120, truncated: false });
    const app = harness({
      engine: {
        search: (args): SearchResults => ({ query: "x", definitions: group(String(args["definitionKinds"])), entrances: group("entrance"), files: { hits: [], total: 0, truncated: false }, projects: [], tookMs: 0 }),
      },
    });
    const seen = new Set<string>();
    let cursor: string | undefined;
    for (let calls = 0; calls < 20; calls += 1) {
      const { text } = await app.call(search, { query: "x", kinds: ["entrances", "functions", "types"], limit: 50, ...(cursor !== undefined ? { cursor } : {}) });
      expect(text).not.toContain("Cut here");
      for (const [key] of text.matchAll(/key \S+/g)) {
        seen.add(key);
      }
      cursor = /More: search with cursor "(\d+)"/.exec(text)?.[1];
      if (cursor === undefined) {
        break;
      }
    }
    expect(cursor).toBeUndefined();
    expect(seen.size).toBe(3 * 120);
  });
});

describe("list_entrances", () => {
  it("lists each application with its launches and entrances, with keys", async () => {
    const app = harness({ engine: { entrances: () => recorded(entrances) } });
    const { text } = await app.call(listEntrances, { project: "ts-orders", filter: "o" });
    expect(app.engineCalls.at(-1)).toEqual(["entrances", { project: PROJECT, perApplication: 25, filter: "o" }]);
    const lines = text.split("\n");
    expect(lines.slice(0, 3)).toEqual([
      "Unfold · ts-orders · as Claude",
      "cli (startup src/cli.ts; started by process orders (package.json), process cli (package.json)), entrances matching “o” (4):",
      `- command import (command, manual) declared in .unfold/config.toml, reaches 7 definitions: key entrance:["manual","command","","import","src/jobs/import-orders.ts#importOrders"]`,
    ]);
    expect(lines).toContain("server (startup src/server.ts; started by process start (package.json), process dev (package.json)), entrances matching “o” (1–8 of 9):");
    expect(lines).toContain(`- worker unknown trigger (worker, bullmq.worker) declared in src/queues/report-worker.ts:6, reaches 0 definitions: key ${String(recorded<{ applications: { entrances: { key: string }[] }[] }>(entrances).applications[3]?.entrances[2]?.key)}`);
    expect(lines).toContain("Launches into code Unfold doesn't index: process legacy:export (package.json).");
    expect(lines.at(-1)).toBe("Next: read_code with an entrance's key shows its handlers; create_board with trace makes a board from it.");
  });

  it("narrows to one application, pages it, and names the applications when one isn't there", async () => {
    const app = harness({ engine: { entrances: () => recorded(entrances) } });
    const server = await app.call(listEntrances, { project: "ts-orders", application: "server", limit: 3 });
    expect(server.text).toContain("server (startup src/server.ts; started by process start (package.json), process dev (package.json)), entrances (1–3 of 9):");
    expect(server.text).toContain('More: list_entrances with cursor "3" for the next page.');
    expect(server.text).not.toContain("cli (");
    const missing = await app.call(listEntrances, { project: "ts-orders", application: "web" });
    expect(missing.text).toBe("Can't list the entrances: ts-orders has no application “web”. Its applications: cli, scheduler, server, worker.");
  });

  it("defaults to the project of the board in view", async () => {
    const app = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-python0000", "py-orders")],
      engine: { entrances: () => recorded(entrances) },
    });
    expect((await app.call(listEntrances, {})).text).toBe(
      "Can't tell which project you mean: the workspace has 2 projects and no board is in view. Pass project: a name or id from list_projects.",
    );
    app.report({ board: { project: "p-python0000", id: ".unfold/boards/a.json" }, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [] });
    expect((await app.call(listEntrances, {})).text.split("\n")[0]).toBe("Unfold · py-orders · as Claude");
  });

  it("fits the largest page of several applications under the size guard, keeping its cursor", async () => {
    const row = (application: string, index: number) => ({
      key: `entrance:["manual","command","","${application}-${index}","src/${"very-long-folder-name/".repeat(6)}jobs.ts#${application}${index}"]`,
      title: `command ${application}-${index}`,
      kind: "command",
      method: null,
      trigger: `${application}-${index}`,
      triggerStatus: "exact",
      label: null,
      detector: "manual",
      path: ".unfold/config.toml",
      line: null,
      language: null,
      group: null,
      reaches: 3,
    });
    const map = recorded<{ applications: { name: string; total: number; entrances: unknown[] }[]; unreached: { total: number; entrances: unknown[] } }>(entrances);
    const big = {
      ...map,
      applications: map.applications.map((item) => ({ ...item, total: 150, entrances: Array.from({ length: 150 }, (_, index) => row(item.name, index)) })),
      unreached: { total: 150, entrances: Array.from({ length: 150 }, (_, index) => row("unreached", index)) },
    };
    const app = harness({ engine: { entrances: () => big } });
    const { text } = await app.call(listEntrances, { project: "ts-orders", limit: 100 });
    expect(text).not.toContain("Cut here");
    const next = Number(/More: list_entrances with cursor "(\d+)"/.exec(text)?.[1]);
    expect(next).toBeGreaterThan(0);
    expect(next).toBeLessThan(100);
    // Every application shows the same rows, so the cursor skips none.
    expect(text.match(/^- command cli-/gm)).toHaveLength(next);
    expect(text.match(/^- command unreached-/gm)).toHaveLength(next);
  });
});
