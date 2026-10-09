// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RecentState } from "../ipc/generated";
import { useWindow } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { scopeEdits } from "./ChooseFoldersDialog";
import { FirstCompile, languageSentence, skippedClauses } from "./FirstCompile";
import { relativeTime } from "./time";
import { Welcome } from "./Welcome";

const NOW = new Date(2026, 9, 9, 15, 0).getTime();
const HOUR = 3_600_000;

let engine: FakeEngine;
let recent: RecentState;
const openDropped = vi.fn(() => Promise.resolve({ windowId: 1, folder: "/dropped" }));

beforeEach(() => {
  recent = {
    folders: [
      { path: "/Users/someone/code/orders-service", name: "orders-service", openedAtMs: Date.now() - 2 * HOUR },
      { path: "/Users/someone/Desktop/repos/graph-compiler", name: "graph-compiler", openedAtMs: Date.now() - 24 * HOUR },
    ],
    welcomeBounds: null,
  };
  useWindow.setState({ info: { windowId: 1, key: "w1", folder: null, platform: "darwin" } });
  engine = installFakeEngine({
    handlers: {
      recent_folders: () => recent,
      forget_recent_folder: ({ path }) => ({ ...recent, folders: recent.folders.filter((folder) => folder.path !== path) }),
      folder_summary: () => fixture("folder_summary", "folder_summary"),
    },
    windowHandlers: { open_folder: () => ({ windowId: 1, folder: "/Users/someone/code/orders-service" }) },
  });
  const api = window.unfold;
  if (api) {
    Object.defineProperty(api, "openDroppedFolder", {
      configurable: true,
      value: openDropped,
    });
  }
});

afterEach(() => {
  engine.uninstall();
});

describe("relativeTime", () => {
  it("reads like a person would say it", () => {
    expect(relativeTime(NOW - 10_000, NOW)).toBe("just now");
    expect(relativeTime(NOW - 5 * 60_000, NOW)).toBe("5 minutes ago");
    expect(relativeTime(NOW - 2 * HOUR, NOW)).toBe("2 hours ago");
    expect(relativeTime(NOW - 20 * HOUR, NOW)).toBe("yesterday");
    expect(relativeTime(NOW - 3 * 24 * HOUR, NOW)).toBe("3 days ago");
    expect(relativeTime(new Date(2026, 2, 4).getTime(), NOW)).toBe("Mar 4");
    expect(relativeTime(new Date(2024, 2, 4).getTime(), NOW)).toBe("Mar 4, 2024");
  });
});

describe("Welcome", () => {
  it("lists recent folders with relative times, opens one, and forgets one", async () => {
    renderWithClient(<Welcome />);
    const list = await screen.findByRole("list");
    const rows = within(list).getAllByRole("listitem");
    expect(rows[0]?.textContent).toContain("orders-service~/code/orders-service2 hours ago");
    expect(rows[1]?.textContent).toContain("yesterday");

    fireEvent.click(within(list).getByTitle("/Users/someone/code/orders-service"));
    await waitFor(() => {
      expect(engine.windowCalls).toContainEqual({ command: "open_folder", args: { path: "/Users/someone/code/orders-service", target: "this_window" } });
    });
    expect(useWindow.getState().info?.folder).toBe("/Users/someone/code/orders-service");

    fireEvent.click(screen.getByRole("button", { name: "Remove graph-compiler from Recent" }));
    await waitFor(() => {
      expect(screen.queryByText("graph-compiler")).toBeNull();
    });
    expect(engine.callsOf("forget_recent_folder")).toEqual([{ path: "/Users/someone/Desktop/repos/graph-compiler" }]);
  });

  it("opens a folder dropped on it", async () => {
    renderWithClient(<Welcome />);
    const file = new File([], "orders-service");
    fireEvent.drop(screen.getByRole("main"), { dataTransfer: { files: [file], types: ["Files"] } });
    await waitFor(() => {
      expect(openDropped).toHaveBeenCalledWith(file, { target: "this_window" });
    });
  });
});

describe("first Compile", () => {
  const summary = fixture("folder_summary", "folder_summary");

  it("summarizes languages and what is skipped", () => {
    expect(languageSentence(summary)).toBe("34 TypeScript files.");
    expect(languageSentence({ ...summary, languages: [...summary.languages, { language: "python", files: 50 }] })).toBe(
      "34 TypeScript files and 50 Python files.",
    );
    expect(skippedClauses(summary.skipped)).toEqual(["files at the top level, scripts (JavaScript isn't graphed)"]);
  });

  it("narrows the scope with graph-only, then include, edits", () => {
    expect(scopeEdits(["src", "lib"], ["src", "lib", "docs"], [])).toEqual([
      { op: "graph_only", path: "src" },
      { op: "include_in_graph", path: "lib" },
    ]);
    expect(scopeEdits(["src", "lib"], ["src", "lib"], [])).toEqual([]);
  });

  it("promises to read only, and offers Choose folders and Compile", async () => {
    renderWithClient(<FirstCompile />);
    expect(await screen.findByText("ts-orders")).toBeTruthy();
    expect(screen.getByText("Reads files only. Runs nothing and installs nothing.")).toBeTruthy();
    expect(screen.getByText("Creates .unfold/ in this folder for the graph and your boards.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose folders…" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Compile/ })).toBeTruthy();
  });
});
