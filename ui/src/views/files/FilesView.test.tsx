// @vitest-environment jsdom
import "../../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DirEntry, DirListing } from "../../ipc/generated";
import { useLayout } from "../../state/layout";
import { getSelection } from "../../state/selection";
import { useTabs } from "../../state/tabs";
import { useWindow } from "../../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { FilesView } from "./FilesView";
import { dirId, useFilesView } from "./model";

const boards = vi.hoisted(() => ({
  openBlockCard: vi.fn<(key: string) => void>(),
  keys: new Set<string>(["src/routes/orders.ts#function:createOrder"]),
}));

vi.mock("../../state/boards/api", () => ({
  openBlockCard: boards.openBlockCard,
  useActiveBoardKeys: () => boards.keys,
}));

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";

function folder(name: string, path: string): DirEntry {
  return { name, path, kind: "folder", language: null, state: null, pending: false, entrances: 0, launches: 0, skipped: null, unfold: false };
}

/** The routes listing with marks the golden doesn't carry: entrances, a pending file, one that doesn't parse. */
function routes(): DirListing {
  const listing = fixture("list_dir", "list_dir_routes");
  return {
    ...listing,
    entries: listing.entries.map((entry) => {
      switch (entry.name) {
        case "orders.ts":
          return { ...entry, entrances: 4 };
        case "admin.ts":
          return { ...entry, pending: true };
        case "health.ts":
          return { ...entry, state: "last_good" as const };
        default:
          return entry;
      }
    }),
  };
}

function root(): DirListing {
  const listing = fixture("list_dir", "list_dir_root");
  return {
    ...listing,
    entries: [
      ...listing.entries.map((entry) => (entry.name === "package.json" ? { ...entry, launches: 7 } : entry)),
      { ...folder("node_modules", "node_modules"), skipped: "default" as const },
    ],
  };
}

let engine: FakeEngine;

beforeEach(() => {
  useWindow.setState({ info: { windowId: 1, key: "w1", folder: "/work/ts-orders", platform: "darwin" } });
  useFilesView.setState({ expanded: new Set([dirId(""), dirId("src"), dirId("src/routes")]), showIgnored: true, selectedId: null });
  useTabs.setState({ tabs: [], activeId: null });
  boards.openBlockCard.mockClear();
  engine = installFakeEngine({
    handlers: {
      list_dir: ({ path }) => {
        if (path === "") {
          return root();
        }
        if (path === "src") {
          return { path: "src", entries: [folder("routes", "src/routes")], truncated: false };
        }
        if (path === "src/routes") {
          return routes();
        }
        return { path, entries: [], truncated: false };
      },
      file_outline: () => fixture("file_outline", "file_outline_orders"),
      update_folder_config: () => ({ exists: true, text: "", settings: null, error: null }),
    },
  });
});

afterEach(() => {
  engine.uninstall();
});

const row = (name: string) => screen.getByRole("treeitem", { name: new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`) });

describe("FilesView", () => {
  it("shows the folder level by level with its quiet marks", async () => {
    renderWithClient(<FilesView />);
    await screen.findByText("orders.ts");

    expect(within(row("orders.ts")).getByLabelText("4 entrances").textContent).toBe("→4");
    expect(within(row("admin.ts")).getByLabelText("Changed since the graph")).toBeTruthy();
    expect(within(row("health.ts")).getByLabelText("Doesn't parse · showing its last version that parsed")).toBeTruthy();
    expect(within(row("report-processor.js")).getByText("not indexed")).toBeTruthy();
    expect(within(row("package.json")).getByText("7 launches")).toBeTruthy();
    expect(within(row(".unfold")).getByText("shared settings")).toBeTruthy();
    expect(row("node_modules").className).toContain("tree-row-dimmed");
    // A collapsed file with something on the board shows the dot; its definitions show it once open.
    expect(within(row("orders.ts")).getByLabelText("On the open board")).toBeTruthy();
    expect(engine.callsOf("list_dir").map((args) => args["path"])).toEqual(["", "src", "src/routes"]);
  });

  it("hides ignored folders with the toggle", async () => {
    renderWithClient(<FilesView />);
    await screen.findByText("node_modules");
    fireEvent.click(screen.getByRole("button", { name: "Hide ignored files" }));
    expect(screen.queryByText("node_modules")).toBeNull();
  });

  it("expands an indexed file into its outline and opens a definition's card on click", async () => {
    renderWithClient(<FilesView />);
    fireEvent.click(await screen.findByText("orders.ts"));
    const createOrder = await screen.findByText("createOrder");
    expect(within(row("createOrder")).getByLabelText("On the open board")).toBeTruthy();
    expect(within(row("listOrders")).queryByLabelText("On the open board")).toBeNull();

    fireEvent.click(createOrder);
    expect(boards.openBlockCard).toHaveBeenCalledWith(CREATE_ORDER);
  });

  it("opens a source tab on double-click", async () => {
    renderWithClient(<FilesView />);
    fireEvent.doubleClick(await screen.findByText("middleware.ts"));
    expect(useTabs.getState().tabs).toEqual([{ kind: "source", id: "source:src/routes/middleware.ts", path: "src/routes/middleware.ts", line: null, reveal: 0 }]);
  });

  it("offers the context menu actions for definitions, files, and folders", async () => {
    renderWithClient(<FilesView />);
    fireEvent.click(await screen.findByText("orders.ts"));
    fireEvent.contextMenu(await screen.findByText("createOrder"));
    const menu = screen.getByRole("menu", { name: "File actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Open card",
      "Open source",
      "Open in editor",
      "Show callers",
      "Add as Entrance…",
      "Reveal in Finder",
    ]);
    fireEvent.click(within(menu).getByText("Show callers"));
    expect(getSelection()).toEqual({ key: CREATE_ORDER });
    expect(useLayout.getState().panelTab).toBe("references");

    fireEvent.contextMenu(screen.getByText("routes"));
    const folderMenu = screen.getByRole("menu", { name: "File actions" });
    expect(within(folderMenu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Exclude from graph",
      "Graph only this folder",
      "Reveal in Finder",
    ]);
    await act(async () => {
      fireEvent.click(within(folderMenu).getByText("Exclude from graph"));
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(engine.callsOf("update_folder_config")).toEqual([{ edits: [{ op: "exclude_from_graph", path: "src/routes" }] }]);
    });

    fireEvent.contextMenu(screen.getByText("report-processor.js"));
    const fileMenu = screen.getByRole("menu", { name: "File actions" });
    expect(within(fileMenu).getByText("Include in graph")).toBeTruthy();
    expect(within(fileMenu).getByRole("menuitem", { name: "Open card" }).getAttribute("aria-disabled")).toBe("true");
  });
});
