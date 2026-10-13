// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardSummary, ProjectRow } from "../../ipc";
import { resetBoards } from "../../state/boards/store";
import { initialLayout, useLayout } from "../../state/layout";
import { getShownBoard, showBoard, useShownBoardStore } from "../../state/shownBoard";
import { useWindowStore } from "../../state/window";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { boardState, projectRow, workspaceView } from "../../test/workspace";
import { HomeSidebar } from "./HomeSidebar";

const API = "p-api0000000";
const WEB = "p-web0000000";

function board(title: string, folder: string | null): BoardSummary {
  return {
    project: API,
    id: `.unfold/boards/${title}.json`,
    name: title,
    title,
    folder,
    pages: 3,
    cards: 4,
    updatedAtMs: 0,
    label: null,
    error: null,
  };
}

let engine: FakeEngine;
let rows: ProjectRow[];

beforeEach(() => {
  rows = [
    projectRow(API, "storefront-api", { expanded: true }),
    projectRow(WEB, "storefront-web", { state: { kind: "compiling", done: 1, total: 9 } }),
  ];
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView(rows),
      app_info: () => ({ username: "lazar", engineVersion: "1" }),
      list_boards: ({ project }) => ({ project, boards: [board("Order lifecycle", "Checkout"), board("Auth", null)], emptyFolders: [] }),
      select_project: () => null,
      set_project_expanded: () => null,
      open_board: ({ project, id }) => boardState(project, id),
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
  useLayout.setState(initialLayout);
  useWindowStore.setState({ info: null, selectedProject: API });
});

afterEach(() => {
  engine.uninstall();
});

describe("the Home sidebar", () => {
  it("draws every project state in the row's slot, the ones a screenshot run can't reach included", async () => {
    rows = [
      projectRow(API, "storefront-api", { state: { kind: "failed", message: "disk full" } }),
      projectRow(WEB, "storefront-web", { state: { kind: "resting", last: { kind: "current" }, atMs: 0 } }),
      projectRow("p-billing000", "billing-worker", { state: { kind: "unavailable", reason: "The folder is gone." } }),
      projectRow("p-ledger0000", "ledger", { state: { kind: "not_compiled" } }),
    ];
    renderWithClient(<HomeSidebar />);
    const slot = async (name: string) => {
      const row = (await screen.findByText(name)).closest("[data-project-id]");
      return row?.querySelector(".state-indicator") ?? null;
    };
    expect((await slot("storefront-api"))?.getAttribute("aria-label")).toBe("Compile failed");
    // Resting: the moon, with the name undimmed.
    const resting = await slot("storefront-web");
    expect(resting?.getAttribute("aria-label")).toBe("Resting");
    expect(resting?.querySelector("svg")?.getAttribute("width")).toBe("14");
    expect((await slot("billing-worker"))?.getAttribute("aria-label")).toBe("The folder is gone.");
    expect((await slot("ledger"))?.getAttribute("aria-label")).toBe("Not compiled yet");
  });

  it("shows the workspace, then the projects list, with every project's state", async () => {
    renderWithClient(<HomeSidebar />);
    expect(await screen.findByText("Lazar's workspace")).toBeTruthy();
    expect(screen.queryByRole("navigation")).toBeNull();
    const web = await screen.findByText("storefront-web");
    const webRow = web.closest("[data-project-id]");
    expect(webRow?.getAttribute("data-project-id")).toBe(WEB);
    expect(within(webRow as HTMLElement).getByRole("img", { name: "Compiling" })).toBeTruthy();
  });

  it("selects a project on click and shows its overview; the chevron only expands", async () => {
    useLayout.setState({ ...initialLayout, home: { ...initialLayout.home, view: "health" } });
    showBoard({ project: API, id: "x" });
    renderWithClient(<HomeSidebar />);
    fireEvent.click(await screen.findByText("storefront-web"));
    await waitFor(() => {
      expect(engine.callsOf("select_project")).toEqual([{ project: WEB }]);
    });
    expect(useLayout.getState().home.view).toBe("overview");
    expect(getShownBoard()).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Expand storefront-web" }));
    await waitFor(() => {
      expect(engine.callsOf("set_project_expanded")).toEqual([{ project: WEB, expanded: true }]);
    });
    expect(engine.callsOf("select_project")).toHaveLength(1);
  });

  it("lists an expanded project's folders, one level deep, then its boards, and opens a board from its row", async () => {
    renderWithClient(<HomeSidebar />);
    const checkout = await screen.findByText("Checkout");
    expect(screen.queryByText("Order lifecycle")).toBeNull();
    fireEvent.click(checkout);
    const lifecycle = await screen.findByText("Order lifecycle");
    expect(useLayout.getState().folders).toEqual([`${API}\u0000Checkout`]);
    const levels = ["Checkout", "Order lifecycle", "Auth"].map((name) => screen.getByText(name).closest("[role=treeitem]")?.getAttribute("aria-level"));
    expect(levels).toEqual(["2", "3", "2"]);
    expect(lifecycle.closest("[role=treeitem]")?.textContent).toBe("Order lifecycle");

    fireEvent.click(screen.getByText("Auth"));
    await waitFor(() => {
      expect(engine.callsOf("open_board")).toEqual([{ project: API, id: ".unfold/boards/Auth.json" }]);
    });
  });

  it("offers a folder's boards and Rename…, but no folder inside it", async () => {
    renderWithClient(<HomeSidebar />);
    fireEvent.contextMenu(await screen.findByText("Checkout"));
    const menu = screen.getByRole("menu", { name: "Sidebar menu" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["New Board Here", "Rename…", "Delete"]);
  });

  it("says when there are no projects yet", async () => {
    rows = [];
    renderWithClient(<HomeSidebar />);
    expect(await screen.findByText("No projects yet")).toBeTruthy();
  });
});
