// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { Toaster } from "../../components";
import type { BoardSummary, ProblemItem, ProjectRow, ProjectSummary, WorkspaceView } from "../../ipc";
import { resetBoards } from "../../state/boards/store";
import { initialLayout, useLayout, type HomeView } from "../../state/layout";
import { usePeekTarget } from "../../state/overlays";
import { useShownBoardStore } from "../../state/shownBoard";
import { useWindowStore } from "../../state/window";
import { OVERVIEW } from "../../test/entrances";
import { fixture, installFakeEngine, type EngineHandlers, type FakeEngine, type FakeEngineOptions } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { boardState, projectRow, workspaceView } from "../../test/workspace";
import { HomeMain } from "./HomeMain";

const P = "p-orders0000";

let engine: FakeEngine | null = null;

function summaryOf(row: ProjectRow, overrides: Partial<ProjectSummary> = {}): ProjectSummary {
  return { ...fixture("project_summary", "project_summary"), project: row.id, name: row.name, state: row.state, ...overrides };
}

function start(
  workspace: WorkspaceView,
  handlers: EngineHandlers = {},
  view: HomeView = "overview",
  windowHandlers: FakeEngineOptions["windowHandlers"] = {},
): FakeEngine {
  const row = workspace.projects[0];
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspace,
      select_project: () => null,
      ...(row ? { project_summary: () => summaryOf(row) } : {}),
      entrances: ({ project }) => ({ ...OVERVIEW, project }),
      list_boards: ({ project }) => ({ ...fixture("list_boards", "list_boards"), project, boards: fixture("list_boards", "list_boards").boards.map((board) => ({ ...board, project })) }),
      ...handlers,
    },
    windowHandlers,
  });
  useLayout.setState({ ...initialLayout, home: { ...initialLayout.home, view } });
  useWindowStore.setState({ info: null, selectedProject: row?.id ?? null });
  return engine;
}

function PeekProbe() {
  const peek = usePeekTarget();
  return <output data-testid="peek">{peek ? `${peek.path}:${peek.line ?? ""}` : ""}</output>;
}

beforeEach(() => {
  resetBoards();
  useShownBoardStore.setState({ board: null });
});

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("an empty workspace", () => {
  it("offers Add project, and shows the project it adds", async () => {
    const fake = start(workspaceView([]), {}, "overview", { add_project: () => ({ status: "added", project: P }) });
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("heading", { name: "Read a codebase by how it starts." })).toBeTruthy();
    expect(screen.queryByText("Opened before")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Add project" }));
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(P);
    });
    expect(fake.windowCalls.map((call) => call.command)).toEqual(["add_project"]);
  });

  it("says why a folder can't be added", async () => {
    const message = "Unfold needs to write .unfold/ in /Volumes/share/app, and it is on a network volume";
    start(workspaceView([]), {}, "overview", {
      add_project: () => {
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "folder_not_writable", message };
      },
    });
    renderWithClient(
      <>
        <HomeMain />
        <Toaster />
      </>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Add project" }));
    expect(await screen.findByText("Couldn't add the folder")).toBeTruthy();
    expect(screen.getByText(message)).toBeTruthy();
    expect(useWindowStore.getState().selectedProject).toBeNull();
  });
});

describe("Home's header and state line", () => {
  const line = () => screen.findByText((_, node) => node?.classList.contains("home-state-line") === true);

  it("reads like the frame: path, name, the project menu, New board, and the state line", async () => {
    // Midday, so a minute ago is still today.
    vi.useFakeTimers({ toFake: ["Date"], now: new Date(2026, 5, 15, 12) });
    onTestFinished(() => {
      vi.useRealTimers();
    });
    const row = projectRow(P, "shop", { path: "/Users/someone/code/shop" });
    start(workspaceView([row]), { project_summary: () => summaryOf(row, { files: 214, languages: ["python"], updatedAtMs: Date.now() - 5_000 }) });
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("heading", { name: "shop" })).toBeTruthy();
    expect(screen.getByText("~/code/shop")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Project menu" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "New board" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Settings" })).toBeNull();
    const state = await line();
    expect(within(state).getByRole("img", { name: "Graph current" })).toBeTruthy();
    await waitFor(() => {
      expect(state.textContent).toBe("Graph current· 214 files · Python · updated just now");
    });
  });

  it("shows progress and Cancel while compiling", async () => {
    const fake = start(workspaceView([projectRow(P, "shop", { state: { kind: "compiling", done: 412, total: 1058 } })]), { cancel_compile: () => ({ cancelled: true }) });
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(state.textContent).toBe("Compiling· 412 of 1,058 filesCancel");
    fireEvent.click(within(state).getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(fake.callsOf("cancel_compile")).toEqual([{ project: P }]);
    });
  });

  it("says a failed Compile with the link joined to its detail", async () => {
    start(workspaceView([projectRow(P, "shop", { state: { kind: "failed", message: "disk full" } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(within(state).getByRole("img", { name: "Compile failed" })).toBeTruthy();
    await waitFor(() => {
      expect(state.textContent).toBe("Compile failed· the previous graph stays ·See Health");
    });
    expect(state.querySelector(".home-state-action-joined")).not.toBeNull();
  });

  it("shows a resting project's Home with the moon and when it was last checked", async () => {
    // Midday, so two hours ago is still today.
    vi.useFakeTimers({ toFake: ["Date"], now: new Date(2026, 5, 15, 12) });
    onTestFinished(() => {
      vi.useRealTimers();
    });
    start(workspaceView([projectRow(P, "shop", { state: { kind: "resting", last: { kind: "current" }, atMs: Date.now() - 2 * 3_600_000 } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(within(state).getByRole("img", { name: "Resting" })).toBeTruthy();
    expect(state.textContent).toBe("Resting· last checked 2 hours ago");
    expect(await screen.findByRole("region", { name: "How it starts" })).toBeTruthy();
  });

  it("shows only the header and the reason for an unavailable project", async () => {
    start(workspaceView([projectRow(P, "shop", { state: { kind: "unavailable", reason: "The folder is gone." } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(state.textContent).toBe("Unavailable· The folder is gone.");
    expect(screen.getByText("This project isn't available")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "How it starts" })).toBeNull();
  });
});

describe("the overview", () => {
  it("shows How it starts, Boards, and the Health line", async () => {
    start(workspaceView([projectRow(P, "shop")]));
    renderWithClient(<HomeMain />);
    expect(await screen.findByText("4 applications · 5 launches · 14 routes. Hover a row to trace it on a new board.")).toBeTruthy();
    expect(await screen.findByRole("region", { name: "Invoice HITL API" })).toBeTruthy();
    expect(await screen.findByText("Order placement")).toBeTruthy();
    const health = screen.getByRole("region", { name: "Health" });
    expect(health.textContent).toBe("HealthAll files parse");
    expect(within(health).queryByRole("button")).toBeNull();
  });

  it("expands an application to its entrances", async () => {
    const fake = start(workspaceView([projectRow(P, "shop")]));
    renderWithClient(<HomeMain />);
    fireEvent.click(await screen.findByRole("button", { name: "Invoice HITL API's entrances" }));
    await waitFor(() => {
      expect(fake.callsOf("entrances")).toContainEqual({ project: P, application: "invoice-hitl", limit: 12 });
    });
  });

  it("lists Boards by last edit, each with its folder and when it was edited, as the engine orders them", async () => {
    vi.useFakeTimers({ toFake: ["Date"], now: new Date(2026, 9, 13, 12) });
    onTestFinished(() => {
      vi.useRealTimers();
    });
    const now = Date.now();
    const board = (id: string, title: string, folder: string | null, updatedAtMs: number): BoardSummary => ({
      project: P,
      id: `.unfold/boards/${id}.json`,
      name: id,
      title,
      folder,
      pages: 1,
      cards: 2,
      updatedAtMs,
      label: null,
      error: null,
    });
    const boards = [
      board("place-an-order", "Place an order", "Checkout", now - 2 * 60_000),
      board("refunds", "Refunds", "Checkout", now - 24 * 3_600_000),
      board("data-model", "Data model", null, new Date(2026, 9, 1, 10).getTime()),
    ];
    const fake = start(workspaceView([projectRow(P, "shop")]), {
      list_boards: () => ({ project: P, boards, emptyFolders: [] }),
      open_board: ({ project, id }) => boardState(project, id),
    });
    renderWithClient(<HomeMain />);
    const list = await screen.findByRole("group", { name: "Boards" });
    const rows = within(list).getAllByRole("button");
    expect(rows.map((row) => row.textContent)).toEqual([
      "Place an orderCheckoutedited 2 minutes ago",
      "RefundsCheckoutedited yesterday",
      "Data modeledited Oct 1",
    ]);
    expect(within(screen.getByRole("region", { name: "Boards" })).queryByRole("radiogroup")).toBeNull();
    fireEvent.click(within(list).getByText("Refunds"));
    await waitFor(() => {
      expect(fake.callsOf("open_board")).toEqual([{ project: P, id: ".unfold/boards/refunds.json" }]);
    });
  });

  it("offers New board when the project has no boards", async () => {
    start(workspaceView([projectRow(P, "shop")]), { list_boards: () => ({ project: P, boards: [], emptyFolders: [] }) });
    renderWithClient(<HomeMain />);
    expect(await screen.findByText("No boards yet")).toBeTruthy();
  });
});

describe("the Health line", () => {
  const board = { id: ".unfold/boards/refunds.json", title: "Refunds", items: 1 };

  function health(row: ProjectRow, value: ProjectSummary["health"]): EngineHandlers {
    return { project_summary: () => summaryOf(row, { health: value }) };
  }

  it("leads to the one board that needs review", async () => {
    const row = projectRow(P, "shop");
    const fake = start(workspaceView([row]), { ...health(row, { doesntParse: 0, boardsNeedingReview: 1, boards: [board] }), open_board: ({ project, id }) => boardState(project, id) });
    renderWithClient(<HomeMain />);
    const line = await screen.findByRole("region", { name: "Health" });
    expect(line.textContent).toBe("Health1 board needs reviewAll files parse");
    fireEvent.click(within(line).getByRole("button", { name: "1 board needs review" }));
    await waitFor(() => {
      expect(fake.callsOf("open_board")).toEqual([{ project: P, id: board.id }]);
    });
  });

  it("counts boards and files that need a look, and leads to Health", async () => {
    const row = projectRow(P, "shop", { state: { kind: "problems", count: 2 } });
    start(workspaceView([row]), { ...health(row, { doesntParse: 2, boardsNeedingReview: 3, boards: [board] }), problems: () => [] });
    renderWithClient(<HomeMain />);
    const line = await screen.findByRole("region", { name: "Health" });
    expect(line.textContent).toBe("Health3 boards need review2 files don't parse");
    fireEvent.click(within(line).getByRole("button", { name: "2 files don't parse" }));
    expect(useLayout.getState().home.view).toBe("health");
  });
});

describe("Health", () => {
  const PROBLEMS: ProblemItem[] = [
    { category: "doesnt_parse", path: "src/a.ts", line: 3, column: 1, message: "Unexpected token", fileState: "last_good" },
    { category: "settings", path: "unfold.toml", line: null, column: null, message: "[scope] exclude isn't a list", fileState: null },
  ];

  it("lists every problem with its action", async () => {
    const fake = start(workspaceView([projectRow(P, "shop", { state: { kind: "problems", count: 2 } })]), { problems: () => PROBLEMS, open_in_editor: () => null }, "health");
    renderWithClient(
      <>
        <HomeMain />
        <PeekProbe />
      </>,
    );
    expect(await screen.findByText("Doesn't parse — line 3: Unexpected token")).toBeTruthy();
    const section = screen.getByRole("region", { name: "Health" });
    expect(within(section).queryByRole("radiogroup")).toBeNull();
    fireEvent.click(within(section).getByRole("button", { name: "Open" }));
    expect(screen.getByTestId("peek").textContent).toBe("src/a.ts:3");
    fireEvent.click(within(section).getByRole("button", { name: "Open in editor" }));
    await waitFor(() => {
      expect(fake.callsOf("open_in_editor")).toEqual([{ project: P, path: "unfold.toml" }]);
    });
    fireEvent.click(screen.getByRole("button", { name: "Back to overview" }));
    expect(useLayout.getState().home.view).toBe("overview");
  });
});
