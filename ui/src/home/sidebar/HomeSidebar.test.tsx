// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { BoardSummary, ProjectRow } from "../../ipc";
import { resetBoards } from "../../state/boards/store";
import { initialLayout, useLayout } from "../../state/layout";
import { useTabs } from "../../state/tabs";
import { useWindowStore } from "../../state/window";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { projectRow, workspaceView } from "../../test/workspace";
import { HomeSidebar } from "./HomeSidebar";

const API = "p-api0000000";
const WEB = "p-web0000000";

function board(title: string, folder: string | null, place: BoardSummary["place"] = "published"): BoardSummary {
  return {
    project: API,
    id: place === "draft" ? `.unfold/local/drafts/${title}.json` : `.unfold/boards/${title}.json`,
    place,
    name: title,
    title,
    folder,
    pages: 3,
    cards: 4,
    projects: [],
    changes: 0,
    updatedAtMs: 0,
    heldBy: null,
    label: null,
    error: null,
  };
}

let engine: FakeEngine;
let rows: ProjectRow[];

beforeEach(() => {
  rows = [
    projectRow(API, "storefront-api", { expanded: true, git: { branch: "main", detached: null, worktrees: 2 } }),
    projectRow(WEB, "storefront-web", { state: { kind: "compiling", done: 1, total: 9 }, git: { branch: "feat/cart-v2", detached: null, worktrees: 1 } }),
  ];
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView(rows),
      app_info: () => ({ username: "lazar", engineVersion: "1" }),
      drafts: () => ({ drafts: [board("Auth", null, "draft")] }),
      list_boards: ({ project }) => ({ project, boards: [board("Order lifecycle", "Checkout"), board("Auth", null, "draft")], emptyFolders: [] }),
      select_project: () => null,
      set_project_expanded: () => null,
      open_board: () => ({ status: "held", heldBy: 2 }),
    },
  });
  resetBoards();
  useTabs.setState({ boards: [], active: "home" });
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
      projectRow("p-payments00", "payments-service", { state: { kind: "paused", changed: 2 } }),
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
    expect((await slot("payments-service"))?.getAttribute("aria-label")).toBe("Background updates paused");
    expect((await slot("ledger"))?.getAttribute("aria-label")).toBe("Not compiled yet");
  });

  it("shows the workspace, Recents and Drafts with the count, and every project with its state", async () => {
    renderWithClient(<HomeSidebar />);
    expect(await screen.findByText("Lazar's workspace")).toBeTruthy();
    const drafts = screen.getByRole("button", { name: /Drafts/ });
    await waitFor(() => {
      expect(drafts.textContent).toBe("Drafts1");
    });
    const web = await screen.findByText("storefront-web");
    const webRow = web.closest("[data-project-id]");
    expect(webRow?.getAttribute("data-project-id")).toBe(WEB);
    // A feature branch shows after the name; main doesn't.
    expect(within(webRow as HTMLElement).getByText("feat/cart-v2")).toBeTruthy();
    expect(screen.queryByText("main")).toBeNull();
    expect(within(webRow as HTMLElement).getByRole("img", { name: "Compiling" })).toBeTruthy();
  });

  it("selects a project on click and shows its overview; the chevron only expands", async () => {
    useLayout.setState({ ...initialLayout, home: { ...initialLayout.home, view: "drafts" } });
    useTabs.setState({ boards: [{ project: API, id: "x" }], active: `${API}\u0000x` });
    renderWithClient(<HomeSidebar />);
    fireEvent.click(await screen.findByText("storefront-web"));
    await waitFor(() => {
      expect(engine.callsOf("select_project")).toEqual([{ project: WEB }]);
    });
    expect(useLayout.getState().home.view).toBe("overview");
    expect(useTabs.getState().active).toBe("home");

    fireEvent.click(screen.getByRole("button", { name: "Expand storefront-web" }));
    await waitFor(() => {
      expect(engine.callsOf("set_project_expanded")).toEqual([{ project: WEB, expanded: true }]);
    });
    expect(engine.callsOf("select_project")).toHaveLength(1);
  });

  it("lists an expanded project's folders and boards, and opens a board from its row", async () => {
    renderWithClient(<HomeSidebar />);
    const checkout = await screen.findByText("Checkout");
    expect(screen.queryByText("Order lifecycle")).toBeNull();
    fireEvent.click(checkout);
    expect(await screen.findByText("Order lifecycle")).toBeTruthy();
    expect(useLayout.getState().folders).toEqual([`${API}\u0000Checkout`]);

    fireEvent.click(screen.getByText("Auth"));
    await waitFor(() => {
      expect(engine.callsOf("open_board")).toEqual([{ project: API, id: ".unfold/local/drafts/Auth.json" }]);
    });
  });

  it("opens Recents and Drafts in Home's main area", async () => {
    renderWithClient(<HomeSidebar />);
    fireEvent.click(screen.getByRole("button", { name: /Recents/ }));
    expect(useLayout.getState().home.view).toBe("recents");
    expect(screen.getByRole("button", { name: /Recents/ }).getAttribute("aria-current")).toBe("page");
    // The project row isn't selected while Recents shows.
    const api = (await screen.findByText("storefront-api")).closest("[data-project-id]");
    expect(api?.getAttribute("aria-selected")).toBe("false");
  });

  it("says when there are no projects yet", async () => {
    rows = [];
    renderWithClient(<HomeSidebar />);
    expect(await screen.findByText("No projects yet")).toBeTruthy();
    // Without projects the Drafts row has no count.
    expect(screen.getByRole("button", { name: "Drafts" }).textContent).toBe("Drafts");
  });
});
