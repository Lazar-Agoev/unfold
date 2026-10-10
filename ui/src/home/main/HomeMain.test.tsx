// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ProblemItem, ProjectRow, ProjectSummary, WorkspaceView } from "../../ipc";
import { resetBoards } from "../../state/boards/store";
import { initialLayout, useLayout, type HomeView } from "../../state/layout";
import { useDialog, usePeekTarget } from "../../state/overlays";
import { useTabs } from "../../state/tabs";
import { useWindowStore } from "../../state/window";
import { fixture, installFakeEngine, type EngineHandlers, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { projectRow, workspaceView } from "../../test/workspace";
import { clearStagedScope } from "../notCompiled/staging";
import { HomeMain } from "./HomeMain";

const P = "p-orders0000";
/** A second project: Compile state is per project and per window, so each Compile test has its own. */
const Q = "p-payments00";

let engine: FakeEngine | null = null;

function summaryOf(row: ProjectRow): ProjectSummary {
  return { ...fixture("project_summary", "project_summary"), project: row.id, name: row.name, state: row.state };
}

function start(workspace: WorkspaceView, handlers: EngineHandlers = {}, view: HomeView = "overview"): FakeEngine {
  const row = workspace.projects[0];
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspace,
      select_project: () => null,
      ...(row ? { project_summary: () => summaryOf(row) } : {}),
      entrances: ({ project }) => ({ ...fixture("entrances", "entrances"), project }),
      list_boards: ({ project }) => ({ ...fixture("list_boards", "list_boards"), project, boards: fixture("list_boards", "list_boards").boards.map((board) => ({ ...board, project })) }),
      board_thumbnails: () => ({ thumbnails: [] }),
      compile_preview: () => fixture("compile_preview", "compile_preview"),
      scope_dir: () => fixture("scope_dir", "scope_dir_root"),
      ...handlers,
    },
    streams: {
      compile: () => Promise.resolve({ files: 34, changedFiles: 34, removedFiles: 0, changedEntrances: 18, complete: true, problems: 0, durationMs: 100 }),
    },
  });
  useLayout.setState({ ...initialLayout, home: { ...initialLayout.home, view } });
  useWindowStore.setState({ info: null, selectedProject: row?.id ?? null });
  return engine;
}

function Probe() {
  const peek = usePeekTarget();
  const dialog = useDialog();
  return (
    <output data-testid="probe">
      {peek ? `peek ${peek.path}:${peek.line ?? ""}` : ""}
      {dialog?.kind === "projectSettings" ? `settings ${dialog.section ?? ""}` : ""}
    </output>
  );
}

beforeEach(() => {
  resetBoards();
  clearStagedScope();
  useTabs.setState({ boards: [], active: "home" });
});

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("first launch (05)", () => {
  it("offers Add project and the folders opened before, and adds one back", async () => {
    const removedAtMs = Date.now() - 3 * 24 * 60 * 60 * 1000;
    const fake = start(workspaceView([], { removed: [{ path: "/Users/someone/code/storefront-api", name: "storefront-api", removedAtMs }] }), {
      readd_project: () => ({ status: "added", project: projectRow(P, "storefront-api") }),
    });
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("heading", { name: "Read a codebase by how it starts." })).toBeTruthy();
    expect(screen.getByText("Drop a folder here")).toBeTruthy();
    expect(screen.getByText("~/code/storefront-api")).toBeTruthy();
    expect(screen.getByText("last opened 3 days ago")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add storefront-api" }));
    await waitFor(() => {
      expect(fake.callsOf("readd_project")).toEqual([{ path: "/Users/someone/code/storefront-api" }]);
    });
    await waitFor(() => {
      expect(useWindowStore.getState().selectedProject).toBe(P);
    });
  });

  it("leaves out Opened before when nothing was removed", async () => {
    start(workspaceView([]));
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("button", { name: /Add project/ })).toBeTruthy();
    expect(screen.queryByText("Opened before")).toBeNull();
  });
});

describe("the not-compiled Home (06)", () => {
  it("keeps scope changes in memory and writes them just before the Compile (V-69)", async () => {
    const fake = start(workspaceView([projectRow(P, "ts-orders", { state: { kind: "not_compiled" }, git: { branch: "main", detached: null, worktrees: 1 } })]), {
      update_project_config: () => ({}) as never,
    });
    renderWithClient(<HomeMain />);
    expect(await screen.findByText("Unfold will read 34 TypeScript files.")).toBeTruthy();
    expect(screen.getByText("Not compiled yet")).toBeTruthy();
    expect(screen.getByText("JavaScript isn't graphed")).toBeTruthy();
    // Only the project menu in the header: no Settings or New board yet.
    expect(screen.queryByRole("button", { name: "New board" })).toBeNull();

    const scripts = screen.getByRole("checkbox", { name: "scripts" });
    expect((scripts as HTMLInputElement).checked).toBe(true);
    fireEvent.click(scripts);
    expect((scripts as HTMLInputElement).checked).toBe(false);
    expect(fake.callsOf("update_project_config")).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: /^Compile/ }));
    await waitFor(() => {
      expect(fake.callsOf("compile")).toHaveLength(1);
    });
    const order = fake.calls.map((call) => call.command).filter((command) => command === "update_project_config" || command === "compile");
    expect(order).toEqual(["update_project_config", "compile"]);
    expect(fake.callsOf("update_project_config")).toEqual([{ project: P, edits: [{ op: "exclude_from_graph", path: "scripts" }] }]);
  });

  it("compiles without writing anything when nothing changed", async () => {
    const fake = start(workspaceView([projectRow(Q, "ts-orders", { state: { kind: "not_compiled" } })]));
    renderWithClient(<HomeMain />);
    const button = await screen.findByRole("button", { name: /^Compile/ });
    fireEvent.click(button);
    await waitFor(() => {
      expect(fake.callsOf("compile")).toEqual([{ project: Q, mode: "check" }]);
    });
    expect(fake.callsOf("update_project_config")).toEqual([]);
  });
});

describe("Home's state line in the states a screenshot run can't reach (07)", () => {
  const line = () => screen.findByText((_, node) => node?.classList.contains("home-state-line") === true);

  it("says a failed Compile with the link joined to its detail (row 7)", async () => {
    start(workspaceView([projectRow(P, "ts-orders", { state: { kind: "failed", message: "disk full" } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(within(state).getByRole("img", { name: "Compile failed" })).toBeTruthy();
    await waitFor(() => {
      expect(state.textContent).toBe("Compile failed· the previous graph stays ·See Health");
    });
    expect(state.querySelector(".home-state-action-joined")).not.toBeNull();
  });

  it("shows a resting project's Home with the moon and when it was last checked (row 8)", async () => {
    start(workspaceView([projectRow(P, "ts-orders", { state: { kind: "resting", last: { kind: "current" }, atMs: Date.now() - 2 * 3_600_000 } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(within(state).getByRole("img", { name: "Resting" })).toBeTruthy();
    expect(state.textContent).toBe("Resting· last checked 2 hours ago");
    // Its last state's Home: the overview, not the first-Compile setup.
    expect(await screen.findByRole("region", { name: "How it starts" })).toBeTruthy();
  });

  it("shows only the header and the reason for an unavailable project", async () => {
    start(workspaceView([projectRow(P, "ts-orders", { state: { kind: "unavailable", reason: "The folder is gone." } })]));
    renderWithClient(<HomeMain />);
    const state = await line();
    expect(state.textContent).toBe("Unavailable· The folder is gone.");
    expect(screen.getByText("This project isn't available")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "How it starts" })).toBeNull();
  });
});

describe("the compiled Home (01)", () => {
  it("shows the header, How it starts, Boards, and the Health line", async () => {
    start(workspaceView([projectRow(P, "ts-orders", { path: "/Users/someone/code/ts-orders" })]));
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("heading", { name: "ts-orders" })).toBeTruthy();
    expect(screen.getByText("~/code/ts-orders")).toBeTruthy();
    expect(await screen.findByText("Graph current")).toBeTruthy();
    expect(await screen.findByText("18 entrances in 4 applications. Hover one to trace it on a new board.")).toBeTruthy();
    expect(await screen.findByRole("region", { name: "cli" })).toBeTruthy();
    expect(await screen.findByText("Order placement")).toBeTruthy();
    expect(screen.getByText("All files parse")).toBeTruthy();
    expect(screen.getByText("1 launch into code that isn't indexed")).toBeTruthy();
  });

  it("expands an application to every entrance with a filter (V-48)", async () => {
    const fake = start(workspaceView([projectRow(P, "ts-orders")]));
    renderWithClient(<HomeMain />);
    const server = await screen.findAllByRole("region");
    const withMore = server.find((region) => within(region).queryByRole("button", { name: "Filter" }));
    if (!withMore) {
      // Every application of the fixture fits in eight rows: nothing to expand.
      expect(fake.callsOf("entrances")).toEqual([{ project: P, perApplication: 8 }]);
      return;
    }
    fireEvent.click(within(withMore).getByRole("button", { name: "Filter" }));
    await waitFor(() => {
      expect(fake.callsOf("entrances").some((args) => args["perApplication"] === 500 && typeof args["application"] === "string")).toBe(true);
    });
  });

  it("filters and sorts Boards, remembered per window", async () => {
    start(workspaceView([projectRow(P, "ts-orders")]));
    renderWithClient(<HomeMain />);
    expect(await screen.findByText("Order placement")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "Drafts" }));
    expect(useLayout.getState().home.boardsFilter).toBe("drafts");
    expect(await screen.findByText("No drafts")).toBeTruthy();
  });
});

describe("Health and activity (11)", () => {
  const PROBLEMS: ProblemItem[] = [
    { category: "doesnt_parse", path: "src/a.ts", line: 3, column: 1, message: "Unexpected token", fileState: "last_good" },
    { category: "settings", path: ".unfold/config.toml", line: null, column: null, message: "[scope] exclude isn't a list", fileState: null },
  ];

  it("lists every problem with its action (V-77)", async () => {
    start(workspaceView([projectRow(P, "ts-orders", { state: { kind: "problems", count: 2 } })]), { problems: () => PROBLEMS }, "health");
    renderWithClient(
      <>
        <HomeMain />
        <Probe />
      </>,
    );
    expect(await screen.findByText("Doesn't parse — line 3: Unexpected token")).toBeTruthy();
    expect(screen.getByRole("radio", { name: "Health 2" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByTestId("probe").textContent).toBe("peek src/a.ts:3");
    fireEvent.click(within(screen.getByRole("region", { name: "Health and activity" })).getByRole("button", { name: "Settings" }));
    expect(screen.getByTestId("probe").textContent).toContain("settings scope");
    fireEvent.click(screen.getByRole("button", { name: "Back to overview" }));
    expect(useLayout.getState().home.view).toBe("overview");
  });

  it("shows the index log, newest first", async () => {
    start(workspaceView([projectRow(P, "ts-orders")]), {
      index_log: () => ({
        entries: [
          { atMs: 1, action: "Compile", files: 34, updateMs: 900, commitMs: 100, filesWritten: 34, filesRemoved: 0, edgesWritten: 1, text: "x\nfiles: read 34, from cache 0\nstates: current 34" },
          { atMs: Date.now() - 60_000, action: "Background update", files: 34, updateMs: 50, commitMs: 40, filesWritten: 1, filesRemoved: 0, edgesWritten: 1, text: "x\nfiles: read 1, from cache 33\nstates: current 33, failed 1" },
        ],
        stats: null,
      }),
    }, "activity");
    renderWithClient(<HomeMain />);
    const table = await screen.findByRole("table", { name: "Activity" });
    const cells = within(table).getAllByRole("row").map((row) => row.textContent);
    expect(cells[1]).toContain("Update");
    expect(within(table).getByText("Update").getAttribute("title")).toBe("Background update");
    expect(cells[1]).toContain("1 / 33 / 1");
    expect(cells[2]).toContain("34 / 0 / 0");
    expect(screen.getByRole("button", { name: "Copy diagnostics" })).toBeTruthy();
  });
});

describe("Recents and Drafts (V-65)", () => {
  it("lists the drafts of every project with their project", async () => {
    const draft = { ...fixture("list_boards", "list_boards").boards[0], project: P, id: ".unfold/local/drafts/trace.json", place: "draft" as const, title: "Trace" };
    start(workspaceView([projectRow(P, "ts-orders")]), { drafts: () => ({ drafts: [draft] as never }) }, "drafts");
    renderWithClient(<HomeMain />);
    expect(await screen.findByRole("heading", { name: "Drafts" })).toBeTruthy();
    expect(await screen.findByText("Trace")).toBeTruthy();
    expect(screen.getByText("ts-orders ·")).toBeTruthy();
  });

  it("says when nothing was opened yet", async () => {
    start(workspaceView([projectRow(P, "ts-orders")]), {}, "recents");
    renderWithClient(<HomeMain />);
    expect(await screen.findByText("No recent boards")).toBeTruthy();
  });
});
