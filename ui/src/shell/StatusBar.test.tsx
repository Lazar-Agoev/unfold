// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { useEffect } from "react";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { GraphStatus } from "../ipc/generated";
import { installEngineInvalidation } from "../ipc/queries";
import { useQueryClient } from "@tanstack/react-query";
import { useLayout } from "../state/layout";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { StatusBar } from "./StatusBar";
import { useCompile } from "./useCompile";

const CURRENT: GraphStatus = {
  state: { kind: "current", files: 34, languages: ["typescript"] },
  fallback: null,
  watching: "events",
  backgroundUpdates: true,
  complete: true,
};

function Harness() {
  const client = useQueryClient();
  useEffect(() => installEngineInvalidation(client), [client]);
  const compile = useCompile("/work/ts-orders");
  return <StatusBar compile={compile} folder="/work/ts-orders" />;
}

let engine: FakeEngine;

// One bridge for the file: the connection hook subscribes to the bridge once per window.
beforeAll(() => {
  engine = installFakeEngine({
    handlers: {
      graph_status: () => CURRENT,
      problems: () => [
        { category: "doesnt_parse", path: "src/a.ts", line: 3, column: 1, message: "Expected `}`", fileState: "last_good" },
      ],
      set_background_updates: ({ on }) => ({ ...CURRENT, backgroundUpdates: on, state: on ? CURRENT.state : { kind: "paused", changed: 0 } }),
    },
  });
});

beforeEach(() => {
  engine.calls.length = 0;
  engine.setConnection("connected");
});

afterAll(() => {
  engine.uninstall();
});

describe("StatusBar", () => {
  it("shows the graph state, follows graph_status events, and counts problems", async () => {
    renderWithClient(<Harness />);
    expect(await screen.findByText("Graph current · 34 files · TypeScript")).toBeTruthy();
    expect(await screen.findByRole("button", { name: "1 problem, show Problems" })).toBeTruthy();

    act(() => {
      engine.emit({ type: "graph_status", folder: "/work/ts-orders", status: { ...CURRENT, state: { kind: "updating", files: 2 } } });
    });
    expect(await screen.findByText("Updating · 2 files changed")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "1 problem, show Problems" }));
    expect(useLayout.getState().panelTab).toBe("problems");
  });

  it("opens the graph menu and pauses background updates from it", async () => {
    renderWithClient(<Harness />);
    fireEvent.click(await screen.findByText("Graph current · 34 files · TypeScript"));
    const menu = screen.getByRole("menu", { name: "Graph" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "CompileCtrl+Shift+B",
      "Rebuild from scratch",
      "Pause background updates",
      "Show index log",
      "Copy Diagnostics",
    ]);
    fireEvent.click(within(menu).getByText("Pause background updates"));
    await waitFor(() => {
      expect(engine.callsOf("set_background_updates")).toEqual([{ on: false }]);
    });
    expect(await screen.findByText("Background updates paused")).toBeTruthy();
  });

  it("says when the engine connection is being restored", async () => {
    renderWithClient(<Harness />);
    await screen.findByText("Graph current · 34 files · TypeScript");
    act(() => {
      engine.setConnection("reconnecting");
    });
    expect(screen.getByText("Reconnecting…")).toBeTruthy();
  });
});
