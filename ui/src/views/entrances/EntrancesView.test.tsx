// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntranceRow, EntranceTree } from "../../ipc/generated";
import { fixture, installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { EntrancesView, entranceRoots, methodColumn, triggerText } from "./EntrancesView";

const boards = vi.hoisted(() => ({
  openEntrance: vi.fn<(key: string) => void>(),
  keys: new Set<string>(),
}));

vi.mock("../../state/boards/api", () => ({
  openEntrance: boards.openEntrance,
  useActiveBoardKeys: () => boards.keys,
}));

function http(method: EntranceRow["method"], trigger: string | null, status: EntranceRow["triggerStatus"] = "exact"): EntranceRow {
  return {
    key: `entrance:["express.route","http","${method ?? ""}","${trigger ?? ""}"]`,
    title: `${method ?? ""} ${trigger ?? ""}`,
    kind: "http",
    method,
    trigger,
    triggerStatus: status,
    label: null,
    detector: "express.route",
    path: "src/routes/orders.ts",
    line: 47,
    language: "typescript",
  };
}

/**
 * One application around three HTTP routes, and the golden's commands and events (four manual CLI
 * commands and SIGTERM) as the entrances no launch reaches.
 */
function tree(): EntranceTree {
  const golden = fixture("entrance_tree", "entrance_tree");
  const all = golden.applications.flatMap((app) => app.entrances);
  return {
    ...golden,
    unreached: [...all.filter((row) => row.kind === "command"), ...all.filter((row) => row.kind === "event")],
    applications: [
      {
        startup: "src/server.ts",
        name: "server",
        language: "typescript",
        path: "src/server.ts",
        launches: [
          {
            key: 'entrance:["package.script","process","","start"]',
            title: "npm run start",
            trigger: "start",
            detector: "package.script",
            path: "package.json",
            startup: "src/server.ts",
            note: null,
          },
        ],
        entrances: [http("GET", "/orders"), http("POST", "/orders"), http("GET", null, "unknown")],
      },
    ],
    unindexedLaunches: [],
  };
}

let engine: FakeEngine;

beforeEach(() => {
  boards.openEntrance.mockClear();
  boards.keys = new Set([http("POST", "/orders").key]);
  engine = installFakeEngine({ handlers: { entrance_tree: tree } });
});

afterEach(() => {
  engine.uninstall();
});

describe("entrance rows", () => {
  it("use the method, or the kind, as the column, and say when the path is unknown", () => {
    expect(methodColumn(http("DELETE", "/orders/:id"))).toBe("DELETE");
    const [command] = tree().unreached;
    expect(command && methodColumn(command)).toBe("COMMAND");
    expect(triggerText(http("GET", null, "unknown"))).toBe("path unknown");
    expect(triggerText(http("GET", "/orders", "partial"))).toBe("/orders…");
  });

  it("group applications first, then the two fixed groups", () => {
    const roots = entranceRoots(tree());
    expect(roots.map((node) => (node.kind === "app" ? node.app.name : node.kind === "group" ? `${node.label} (${node.count})` : node.id))).toEqual([
      "server",
      "Not reached from a launch (5)",
      "Launches into code that isn't indexed (0)",
    ]);
  });
});

describe("EntrancesView", () => {
  it("shows applications with their launches and entrances, and opens an entrance on click", async () => {
    renderWithClient(<EntrancesView />);
    const server = await screen.findByRole("treeitem", { name: /^server/ });
    expect(server.getAttribute("aria-expanded")).toBe("true");
    expect(within(server).getByText("ts")).toBeTruthy();
    expect(screen.getByText("npm run start")).toBeTruthy();
    expect(screen.getByText("path unknown")).toBeTruthy();

    const post = screen.getAllByRole("treeitem").find((item) => item.textContent.startsWith("POST"));
    expect(post && within(post).getByLabelText("On the open board")).toBeTruthy();

    const get = screen.getAllByRole("treeitem").find((item) => item.textContent.startsWith("GET/orders"));
    expect(get).toBeTruthy();
    if (get) {
      fireEvent.click(get);
    }
    expect(boards.openEntrance).toHaveBeenCalledWith(http("GET", "/orders").key);
  });

  it("lists entrances that no launch reaches under their own group, with language tags", async () => {
    renderWithClient(<EntrancesView />);
    const group = await screen.findByRole("treeitem", { name: /^Not reached from a launch/ });
    expect(within(group).getByText("5")).toBeTruthy();
    fireEvent.click(group);
    expect(await screen.findByText("SIGTERM")).toBeTruthy();
    expect(screen.getByText("import")).toBeTruthy();
  });

  it("asks for a Compile before the first graph", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        entrance_tree: () => {
          // eslint-disable-next-line @typescript-eslint/only-throw-error -- the bridge rejects with plain data.
          throw { code: "not_compiled", message: "The folder has no graph yet." };
        },
      },
    });
    renderWithClient(<EntrancesView />);
    expect(await screen.findByText("No entrances yet")).toBeTruthy();
  });
});
