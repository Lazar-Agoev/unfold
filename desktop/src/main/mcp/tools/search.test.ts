import { describe, expect, it, vi } from "vitest";
import entrances from "../../../../../testdata/workspace/commands/entrances.json";
import searchPlace from "../../../../../testdata/workspace/commands/search_place.json";
import searchWorker from "../../../../../testdata/workspace/commands/search_worker.json";
import type { EntrancesView, SearchResults } from "../engine-types";
import { listEntrances, search } from "./search";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const NONE = { hits: [], total: 0, truncated: false };

/** The engine's name search: the recorded answer for its query, narrowed to the kinds asked. */
function names(args: Readonly<Record<string, unknown>>): SearchResults {
  const found = recorded<SearchResults>(args["query"] === "place" ? searchPlace : searchWorker);
  const kinds = args["kinds"] as string[];
  const types = (args["definitionKinds"] as string[] | undefined)?.includes("type") === true;
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
      "- Worker (app): key entrance:worker",
      "- Send an email (entrance of Worker): key entrance:worker/send-an-email",
      "- Build a report (entrance of Worker): key entrance:worker/build-a-report",
      "- Process an order (entrance of Worker): key entrance:worker/process-an-order",
      "Files (4):",
      "- src/worker.ts: 2 definitions",
      "- src/queues/email-worker.ts: 4 definitions",
      "- src/queues/orders-worker.ts: 2 definitions",
      "- src/queues/report-worker.ts: 2 definitions",
      "Next: read_code or get_relations with a key; add_cards puts it on a page.",
    ]);
    const asked = app.engineCalls.filter(([command]) => command === "search").map(([, args]) => args);
    expect(asked).toEqual([
      { query: "worker", kinds: ["definitions"], definitionKinds: ["function"], limit: 10, projects: [PROJECT] },
      { query: "worker", kinds: ["definitions"], definitionKinds: ["type", "class"], limit: 10, projects: [PROJECT] },
      { query: "worker", kinds: ["entrances", "files"], limit: 10, projects: [PROJECT] },
    ]);
    const place = await app.call(search, { query: "place", kinds: ["functions"] });
    expect(place.text).toContain("Functions (1):\n- OrderService.place (function) at src/services/order-service.ts:13: key src/services/order-service.ts#OrderService.place");
  });

  it("pages every kind with one cursor", async () => {
    const app = harness({ engine: { search: names } });
    const first = await app.call(search, { query: "worker", kinds: ["entrances"], limit: 3 });
    expect(first.text).toContain("Entrances (1–3 of 4):");
    expect(first.text).toContain('More: search with cursor "3" for the next page.');
    const second = await app.call(search, { query: "worker", kinds: ["entrances"], limit: 3, cursor: "3" });
    expect(second.text).toContain("Entrances (4–4 of 4):\n- Process an order");
    expect(second.text).not.toContain("More:");
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ limit: 6 });
    expect((await app.call(search, { query: "worker", cursor: "soon" })).text).toBe(
      "“soon” isn't a cursor of this list. Leave cursor out for the first page, or pass the next cursor a result gave.",
    );
  });

  it("narrows to projects by name, and says when nothing matches", async () => {
    const app = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-python0000", "py-orders")],
      engine: { search: (args) => ({ ...names(args), definitions: NONE, entrances: NONE, files: NONE }) },
    });
    const nothing = await app.call(search, { query: "zzz", projects: ["py-orders"] });
    expect(nothing.text).toBe("Unfold · py-orders · as Claude\nNothing matches “zzz”. Try fewer or shorter words.");
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ projects: ["p-python0000"] });
  });

  it("fits the largest page under the size guard, keeping its cursor, and reaches every hit across pages", async () => {
    const deep = `src/${"very-long-folder-name/".repeat(8)}module.ts`;
    const many = (kind: string, count: number) =>
      Array.from({ length: count }, (_, index) => ({ project: PROJECT, key: `${deep}#Service${index}.${kind}${index}`, name: `${kind}${index}`, qualifiedName: `Service${index}.${kind}${index}`, path: deep, kind: "function", line: index + 1, app: null, score: 1 }));
    const group = (kind: string) => ({ hits: many(kind, 120), total: 120, truncated: false });
    const app = harness({
      engine: {
        search: (args): SearchResults => ({ query: "x", definitions: group(String(args["definitionKinds"])), entrances: group("entrance"), files: { hits: [], total: 0, truncated: false } }),
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

/** The recorded apps, each with `entries` more entrances (with long keys) after its own. */
function declared(entries: number): () => EntrancesView {
  return () => {
    const view = recorded<EntrancesView>(entrances);
    const long = `src/${"very-long-folder-name/".repeat(6)}jobs.ts`;
    return {
      ...view,
      apps: view.apps.map((app) => ({
        ...app,
        entrances: [
          ...app.entrances,
          ...Array.from({ length: entries }, (_, index) => ({ key: `${app.key ?? ""}/job-${index}`, name: `Job ${index}`, on: `job-${index}`, code: [{ key: `${long}#job${index}`, state: "found" as const }] })),
        ],
      })),
    };
  };
}

describe("list_entrances", () => {
  it("lists each app as unfold.toml declares it, with its entrances, their code, and each code's state", async () => {
    const app = harness({ engine: { entrances: declared(0) } });
    const { text } = await app.call(listEntrances, { project: "ts-orders" });
    expect(app.engineCalls.filter(([command]) => command === "entrances")).toEqual([["entrances", { project: PROJECT }]]);
    const lines = text.split("\n");
    expect(lines[0]).toBe("Unfold · ts-orders · as Claude");
    expect(lines).toContain("Server (key entrance:server; run npm run start; code src/server.ts# (found)), entrances (9):");
    expect(lines).toContain(
      "- Place an order: key entrance:server/place-an-order; on POST /orders; code src/routes/middleware.ts#requireUser (found), src/routes/middleware.ts#validateBody (found), src/routes/orders.ts#createOrder (found)",
    );
    expect(lines.at(-1)).toBe("Next: read_code with an app's or entrance's key shows its code; create_board with trace makes a board from it.");
    const filtered = await app.call(listEntrances, { project: "ts-orders", filter: "POST" });
    expect(filtered.text).toContain("Server (key entrance:server; run npm run start; code src/server.ts# (found)), entrances matching “POST” (1):");
  });

  it("narrows to one app, pages it, and names the apps when one isn't there", async () => {
    const app = harness({ engine: { entrances: declared(9) } });
    const server = await app.call(listEntrances, { project: "ts-orders", app: "Server", limit: 3 });
    expect(server.text).toContain("Server (key entrance:server; run npm run start; code src/server.ts# (found)), entrances (1–3 of 18):");
    expect(server.text).toContain('More: list_entrances with cursor "3" for the next page.');
    expect(server.text).not.toContain("Worker (");
    const byKey = await app.call(listEntrances, { project: "ts-orders", app: "entrance:server", limit: 3 });
    expect(byKey.text).toContain("Server (key entrance:server;");
    const missing = await app.call(listEntrances, { project: "ts-orders", app: "web" });
    expect(missing.text).toMatch(/^Can't list the entrances: ts-orders has no app “web”\. Its apps: Server, /);
  });

  it("defaults to the project of the board in view", async () => {
    const app = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-python0000", "py-orders")],
      engine: { entrances: declared(0) },
    });
    expect((await app.call(listEntrances, {})).text).toBe(
      "Can't tell which project you mean: the workspace has 2 projects and no board is in view. Pass project: a name or id from list_projects.",
    );
    app.report({ board: { project: "p-python0000", id: ".unfold/boards/a.json" }, page: "page-1", selection: { kind: "none" }, visible: [], held: [] });
    expect((await app.call(listEntrances, {})).text.split("\n")[0]).toBe("Unfold · py-orders · as Claude");
  });

  it("fits the largest page of several apps under the size guard, keeping its cursor", async () => {
    const app = harness({ engine: { entrances: declared(150) } });
    const { text } = await app.call(listEntrances, { project: "ts-orders", limit: 100 });
    expect(text).not.toContain("Cut here");
    const next = Number(/More: list_entrances with cursor "(\d+)"/.exec(text)?.[1]);
    expect(next).toBeGreaterThan(0);
    expect(next).toBeLessThan(100);
  });
});
