// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IndexLog, ProblemItem, Relations } from "../ipc/generated";
import { select } from "../state/selection";
import { useTabs } from "../state/tabs";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { factLines, IndexLogPanel } from "./IndexLogPanel";
import { problemGroups, ProblemsPanel } from "./ProblemsPanel";
import { callerOwner, ReferencesPanel } from "./ReferencesPanel";

const boards = vi.hoisted(() => ({
  openBlockCard: vi.fn<(key: string) => void>(),
  openEntrance: vi.fn<(key: string) => void>(),
  keys: new Set<string>(),
}));

vi.mock("../state/boards/api", () => ({
  openBlockCard: boards.openBlockCard,
  openEntrance: boards.openEntrance,
  useActiveBoardKeys: () => boards.keys,
}));

const PROBLEMS: ProblemItem[] = [
  { category: "board", path: ".unfold/boards/orders.json", line: null, column: null, message: "2 descriptions outdated", fileState: null },
  { category: "doesnt_parse", path: "src/services/payment.ts", line: 42, column: 7, message: "Expected `)`", fileState: "last_good" },
  { category: "settings", path: ".unfold/config.toml", line: 3, column: 1, message: "Unknown key `scop`", fileState: null },
];

const LOG: IndexLog = {
  entries: [
    {
      atMs: 1_700_000_000_000,
      action: "Compile",
      files: 34,
      updateMs: 120,
      commitMs: 30,
      filesWritten: 34,
      filesRemoved: 0,
      edgesWritten: 200,
      text: "Compile · 34 files (typescript 34) · 8 threads · 120ms\nfiles: read 34, from cache 0, extracted 34, removed 0; inputs read 2; discovery full\nstages: discover 2.0ms, extract 60.0ms\nedges: 200 (resolved 150, external 30, unresolved 20)\n",
    },
  ],
  stats: {
    filesCurrent: 34,
    filesLastGood: 0,
    filesFailed: 0,
    inputs: 2,
    blocks: 900,
    edgesResolved: 150,
    edgesExternal: 30,
    edgesUnresolved: 20,
    entrances: 12,
    definitions: 80,
    applications: 3,
    databaseBytes: 3 * 1024 * 1024,
  },
};

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";

function relations(key: string): Relations {
  const base = fixture("relations", "relations_create_order");
  if (key === PLACE) {
    return {
      ...base,
      key,
      callers: [
        { owner: CREATE_ORDER, ownerName: "createOrder", source: `${CREATE_ORDER}/variable:order/call[0]`, path: "src/routes/orders.ts", line: 32, kind: "calls", position: null },
      ],
      callersTotal: 1,
      targetNames: { [key]: "OrderService.place" },
      reachedFrom: [{ key: 'entrance:["express.route","http","POST","/orders"]', title: "POST /orders", kind: "http", method: "POST", trigger: "/orders", path: "src/routes/orders.ts" }],
    };
  }
  return {
    ...base,
    key,
    callers: [{ owner: "src/server.ts#", ownerName: "src/server.ts", source: "src/server.ts#call[0]", path: "src/server.ts", line: 9, kind: "references", position: 1 }],
    callersTotal: 1,
  };
}

let engine: FakeEngine;

beforeEach(() => {
  useTabs.setState({ tabs: [], activeId: null });
  boards.openBlockCard.mockClear();
  boards.openEntrance.mockClear();
  select(null);
  engine = installFakeEngine({
    handlers: {
      problems: () => PROBLEMS,
      index_log: () => LOG,
      relations: ({ key }) => relations(key),
      peek: ({ key }) => ({ ...fixture("peek", "peek_create_order"), key, name: "place", qualifiedName: "OrderService.place" }),
      open_board: ({ id }) => ({
        status: "opened",
        board: {
          id,
          place: "published",
          name: "orders",
          document: { format: "unfold-board", version: 1, title: "Orders", defaultView: "code", cards: [], groups: [], labels: {}, guards: {} },
          revision: "r1",
          health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
          outline: { rows: [], unlabeled: [] },
          label: null,
        },
      }),
    },
  });
});

afterEach(() => {
  engine.uninstall();
});

describe("Problems", () => {
  it("groups problems by category in a fixed order", () => {
    expect(problemGroups(PROBLEMS).map((group) => group.label)).toEqual(["Settings", "Files that don't parse", "Boards"]);
  });

  it("opens a file problem at its line and a board problem as its board", async () => {
    renderWithClient(<ProblemsPanel />);
    fireEvent.click(await screen.findByRole("option", { name: /^src\/services\/payment\.ts:42/ }));
    expect(useTabs.getState().tabs[0]).toMatchObject({ path: "src/services/payment.ts", line: 42 });
    expect(screen.getByText("Expected `)` · showing its last version that parsed")).toBeTruthy();
    fireEvent.click(screen.getByRole("option", { name: /^\.unfold\/boards\/orders\.json/ }));
    await waitFor(() => {
      expect(useTabs.getState().tabs.map((tab) => tab.id)).toContain("board:.unfold/boards/orders.json");
    });
    expect(engine.callsOf("open_board")).toEqual([{ id: ".unfold/boards/orders.json" }]);
  });
});

describe("Index log", () => {
  it("splits the engine's facts into labels and values", () => {
    const { summary, facts } = factLines(LOG.entries[0]?.text ?? "");
    expect(summary).toContain("Compile · 34 files");
    expect(facts.map((fact) => fact.label)).toEqual(["files", "stages", "edges"]);
  });

  it("shows the graph's counts and each update", async () => {
    renderWithClient(<IndexLogPanel />);
    expect(await screen.findByText("150 resolved · 30 external · 20 unresolved")).toBeTruthy();
    expect(screen.getByText("3.0 MB")).toBeTruthy();
    expect(screen.getByText("discover 2.0ms, extract 60.0ms")).toBeTruthy();
  });
});

describe("References", () => {
  it("shows callers as a tree that expands into their callers, and reached-from entrances", async () => {
    renderWithClient(<ReferencesPanel />);
    expect(screen.getByText("Nothing selected")).toBeTruthy();
    act(() => {
      select({ key: PLACE });
    });
    expect(await screen.findByText("OrderService.place")).toBeTruthy();
    const caller = await screen.findByRole("treeitem", { name: /^createOrder/ });
    expect(caller.getAttribute("aria-expanded")).toBe("false");

    fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowDown" });
    fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowRight" });
    expect(await screen.findByText("src/server.ts")).toBeTruthy();
    await waitFor(() => {
      expect(engine.callsOf("relations").map((args) => args["key"])).toEqual([PLACE, CREATE_ORDER]);
    });

    fireEvent.click(caller);
    expect(boards.openBlockCard).toHaveBeenCalledWith(CREATE_ORDER);
    fireEvent.click(screen.getByText("POST /orders"));
    expect(boards.openEntrance).toHaveBeenCalledWith('entrance:["express.route","http","POST","/orders"]');
  });

  it("reads a caller row's owner from its id", () => {
    expect(callerOwner(`callers\u0000a#call[0]\u0001a.ts#function:f`)).toBe("a.ts#function:f");
    expect(callerOwner("callers")).toBeNull();
  });
});
