// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { GitInfo } from "../ipc";
import { getOpenBoard, openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { useShownBoardStore } from "../state/shownBoard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardState, projectRow, workspaceView } from "../test/workspace";
import { removeProject } from "./actions";
import { WorktreeButton } from "./WorktreeButton";

const P = "p-1";
const Q = "p-2";
const MINE = { project: P, id: ".unfold/local/drafts/a.json" };
const OTHER = { project: Q, id: ".unfold/local/drafts/b.json" };

const GIT: GitInfo = {
  branch: "main",
  detached: null,
  worktrees: [
    { root: "/work/unfold", path: "/work/unfold", branch: "main", detached: null, main: true, current: true, available: true },
    { root: "/work/unfold-feat", path: "/work/unfold-feat", branch: "feat/cart", detached: null, main: false, current: false, available: true },
    { root: "/work/gone", path: "/work/gone", branch: null, detached: "abc1234", main: false, current: false, available: false },
  ],
};

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
  resetBoards();
  useShownBoardStore.setState({ board: null });
});

async function install(handlers: Parameters<typeof installFakeEngine>[0] = {}): Promise<FakeEngine> {
  engine = installFakeEngine({
    ...handlers,
    handlers: {
      workspace: () =>
        workspaceView([
          projectRow(P, "unfold", { git: { branch: "main", detached: null, worktrees: 3 } }),
          projectRow(Q, "billing", { git: null }),
        ]),
      git_info: () => GIT,
      open_board: ({ project, id }) => boardState(project, id),
      close_board: () => null,
      switch_worktree: () => projectRow(P, "unfold"),
      remove_project: () => null,
      ...handlers.handlers,
    },
  });
  await openBoard(MINE, { background: true });
  await openBoard(OTHER, { background: true });
  return engine;
}

describe("the worktree switcher", () => {
  it("shows the branch and worktree count, lists the worktrees, and switches after closing this project's boards", async () => {
    const fake = await install();
    renderWithClient(<WorktreeButton project={P} />);
    const button = await screen.findByRole("button", { name: /Branch main/ });
    expect(button.textContent).toContain("3 worktrees");
    fireEvent.click(button);
    const menu = await screen.findByRole("menu", { name: "Worktrees" });
    const rows = await within(menu).findAllByRole("menuitemradio");
    expect(rows.map((row) => row.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
    expect(rows[2]?.getAttribute("aria-disabled")).toBe("true");
    expect(rows[2]?.textContent).toContain("detached abc1234");
    expect(menu.textContent).toContain("Drafts and the graph are per worktree.");
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: /feat\/cart/ }));
    await waitFor(() => {
      expect(fake.callsOf("switch_worktree")).toEqual([{ project: P, path: "/work/unfold-feat" }]);
    });
    const order = fake.calls.map((call) => call.command).filter((command) => command === "close_board" || command === "switch_worktree");
    expect(order).toEqual(["close_board", "switch_worktree"]);
    expect(fake.callsOf("close_board")).toEqual([{ project: P, id: MINE.id }]);
    expect(getOpenBoard(MINE)).toBeUndefined();
    expect(getOpenBoard(OTHER)).toBeDefined();
  });

  it("is absent for a project outside a repository", async () => {
    await install();
    const { container } = renderWithClient(<WorktreeButton project={Q} />);
    await waitFor(() => {
      expect(engine?.callsOf("workspace").length).toBeGreaterThan(0);
    });
    expect(container.textContent).toBe("");
  });
});

describe("Remove from workspace", () => {
  it("asks the main process, closes the project's open boards, then removes it", async () => {
    const fake = await install({ windowHandlers: { confirm_remove_project: () => ({ confirmed: true }) } });
    await removeProject({ id: P, name: "unfold" });
    expect(fake.windowCalls.map((call) => call.command)).toContain("confirm_remove_project");
    expect(fake.callsOf("close_board")).toEqual([{ project: P, id: MINE.id }]);
    expect(fake.callsOf("remove_project")).toEqual([{ project: P }]);
  });

  it("does nothing when the person says no", async () => {
    const fake = await install({ windowHandlers: { confirm_remove_project: () => ({ confirmed: false }) } });
    await removeProject({ id: P, name: "unfold" });
    expect(fake.callsOf("close_board")).toEqual([]);
    expect(fake.callsOf("remove_project")).toEqual([]);
  });
});
