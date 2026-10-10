// @vitest-environment jsdom
import "../test/dom";
import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { MenuCommand, Preferences } from "../ipc";
import { ActionIds, actions } from "../keyboard";
import { MENU_COMMAND_ACTIONS } from "../keyboard/keymap";
import { boardKey } from "../state/boards";
import * as store from "../state/boards/store";
import { initialLayout, useLayout } from "../state/layout";
import { useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { App } from "./App";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: class { m22 = 1; } });
}

const P = "p-orders0000";
const REF = { project: P, id: ".unfold/local/drafts/orders.json" };

const PREFERENCES: Preferences = {
  theme: "system",
  editorCommand: null,
  backgroundUpdates: true,
  threadLimit: null,
  keyBindings: {},
  updates: { enabled: true },
};

let engine: FakeEngine;

beforeEach(() => {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(P, "orders")]),
      select_project: () => null,
      get_preferences: () => PREFERENCES,
      list_boards: ({ project }) => ({ project, boards: [], emptyFolders: [] }),
      save_board_view: () => null,
    },
  });
  store.resetBoards();
  store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1"), page("page-2")])));
  useTabs.setState({ boards: [REF], active: boardKey(REF) });
  useLayout.setState(initialLayout);
  useWindowStore.setState({ info: null, selectedProject: null });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the application menu's commands", () => {
  it("each run an action the window registers, as does every shortcut of the keyboard table", async () => {
    renderWithClient(<App />);
    await screen.findByRole("toolbar", { name: "Board" });
    const missing = (Object.entries(MENU_COMMAND_ACTIONS) as [MenuCommand, string][]).filter(([, id]) => actions.get(id) === undefined);
    expect(missing).toEqual([]);
    expect(Object.values(ActionIds).filter((id) => actions.get(id) === undefined)).toEqual([]);
  });

  it("run their action when the menu sends them", async () => {
    renderWithClient(<App />);
    await screen.findByRole("toolbar", { name: "Board" });
    act(() => {
      engine.emit({ type: "menu_command", command: "toggle_sidebar" });
    });
    expect(useLayout.getState().sidebar.visible).toBe(false);
    act(() => {
      engine.emit({ type: "menu_command", command: "show_home" });
    });
    expect(useTabs.getState().active).toBe("home");
    await waitFor(() => {
      expect(actions.get(ActionIds.newPage)).toBeUndefined();
    });
  });
});
