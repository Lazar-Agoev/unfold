// @vitest-environment jsdom
import "../test/dom";
import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Toaster, dismissToast } from "../components";
import type { ProjectState } from "../ipc";
import { startCompile } from "../state/compile";
import { initialLayout, useLayout } from "../state/layout";
import { useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { projectRow, workspaceView } from "../test/workspace";
import { CompiledToasts } from "./CompiledToasts";

const WEB = "p-web0000001";
const API = "p-api0000001";

let engine: FakeEngine;
let state: ProjectState;
let finish: () => void;

beforeEach(() => {
  state = { kind: "current" };
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(API, "storefront-api"), projectRow(WEB, "storefront-web")]),
      project_summary: ({ project }) => ({ ...fixture("project_summary", "project_summary"), project, name: "storefront-web", entrances: 6, state }),
    },
    streams: {
      compile: () =>
        new Promise((resolve) => {
          finish = () => {
            resolve({ files: 1, changedFiles: 1, removedFiles: 0, changedEntrances: 6, complete: true, problems: 0, durationMs: 1 });
          };
        }),
    },
  });
  useLayout.setState(initialLayout);
  useTabs.setState({ boards: [], active: "home" });
  useWindowStore.setState({ info: null, selectedProject: API });
});

afterEach(() => {
  dismissToast(`compiled:${WEB}`);
  engine.uninstall();
});

async function compileOnce(project: string): Promise<void> {
  renderWithClient(
    <>
      <CompiledToasts />
      <Toaster />
    </>,
  );
  // The watchers mount once the workspace lists the projects.
  await waitFor(() => {
    expect(engine.callsOf("workspace")).toHaveLength(1);
  });
  await act(async () => {
    await Promise.resolve();
  });
  let done: Promise<void> = Promise.resolve();
  act(() => {
    done = startCompile(project);
  });
  await waitFor(() => {
    expect(engine.callsOf("compile")).toHaveLength(1);
  });
  await act(async () => {
    finish();
    await done;
  });
}

describe("the compiled toast (V-67)", () => {
  it("announces a Compile this window started, with the entrances it found", async () => {
    await compileOnce(WEB);
    expect(await screen.findByText("storefront-web compiled")).toBeTruthy();
    expect(screen.getByText("6 entrances found")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open Home" })).toBeTruthy();
  });

  it("stays quiet while Home shows that project", async () => {
    useWindowStore.setState({ info: null, selectedProject: WEB });
    await compileOnce(WEB);
    expect(engine.callsOf("project_summary")).toEqual([]);
    expect(screen.queryByText("storefront-web compiled")).toBeNull();
  });

  it("stays quiet when the Compile failed", async () => {
    state = { kind: "failed", message: "disk full" };
    await compileOnce(WEB);
    await waitFor(() => {
      expect(engine.callsOf("project_summary")).toHaveLength(1);
    });
    expect(screen.queryByText("storefront-web compiled")).toBeNull();
  });
});
