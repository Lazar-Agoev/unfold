// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ProjectConfig, ScopeDir, SettingsEdit } from "../ipc";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { projectRow, workspaceView } from "../test/workspace";
import { ProjectSettingsDialog } from "./ProjectSettingsDialog";
import type { ProjectSettingsSection } from "../state/overlays";

const PROJECT = "p-1";

const CONFIG: ProjectConfig = {
  exists: true,
  text: "[scope]\nexclude = [\"scripts\"]\n",
  error: null,
  name: { value: "storefront-api", isDefault: true },
  settings: { languages: null, include: [], exclude: ["scripts"], disabledDetectors: [], pythonProjects: [], entrances: [] },
};

const ROOT: ScopeDir = {
  path: "",
  truncated: false,
  folders: [
    { name: "dist", path: "dist", skipped: "default", sources: 0 },
    { name: "scripts", path: "scripts", skipped: "excluded", sources: 14 },
    { name: "src", path: "src", skipped: null, sources: 1102 },
  ],
};

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

function install(update: (edits: SettingsEdit[]) => ProjectConfig | Promise<ProjectConfig> = () => CONFIG): FakeEngine {
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(PROJECT, "storefront-api")]),
      project_config: () => CONFIG,
      project_summary: () => fixture("project_summary", "project_summary"),
      scope_dir: ({ path }) => (path === "" ? ROOT : { path, truncated: false, folders: [] }),
      set_background_updates: () => fixture("project_summary", "project_summary").status,
      update_project_config: (args) => update(args.edits),
    },
  });
  return engine;
}

let closed = 0;

async function open(section: ProjectSettingsSection = "scope"): Promise<HTMLElement> {
  closed = 0;
  const { rerender } = renderWithClient(
    <ProjectSettingsDialog
      project={PROJECT}
      section={section}
      onClose={() => {
        closed += 1;
        rerender(<></>);
      }}
    />,
  );
  return await screen.findByRole("dialog", { name: "storefront-api settings" });
}

const save = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Save" });

describe("Project settings", () => {
  it("shows the scope tree with skipped folders last and the saved rules", async () => {
    install();
    const dialog = await open();
    const tree = await within(dialog).findByRole("tree", { name: "Folders in storefront-api" });
    const rows = within(tree).getAllByRole("treeitem").map((row) => row.textContent);
    expect(rows[0]).toContain("scripts");
    expect(rows[0]).toContain("excluded");
    expect(rows[1]).toContain("src");
    expect(rows[2]).toContain("skipped by default");
    expect(within(dialog).getByText("File exists")).toBeTruthy();
    expect(within(dialog).getByRole("listitem").textContent).toContain("scripts");
    expect(save(dialog)).toHaveProperty("disabled", true);
  });

  it("stages changes across sections and saves them in one call", async () => {
    const fake = install();
    const dialog = await open();
    await within(dialog).findByRole("tree");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "Graph src" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove exclude scripts" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Detectors" }));
    fireEvent.click(within(dialog).getByRole("switch", { name: "Celery tasks" }));
    expect(fake.callsOf("update_project_config")).toHaveLength(0);
    fireEvent.click(save(dialog));
    await waitFor(() => {
      expect(closed).toBe(1);
    });
    expect(fake.callsOf("update_project_config")).toEqual([
      {
        project: PROJECT,
        edits: [
          { op: "remove_scope_pattern", list: "exclude", pattern: "scripts" },
          { op: "exclude_from_graph", path: "src" },
          { op: "set_detector", detector: "celery.task", enabled: false },
        ],
      },
    ]);
  });

  it("keeps the dialog and the staged changes when Rust refuses them", async () => {
    // Rejections are plain {code, message} data, as from the real preload.
    // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
    install(() => Promise.reject({ code: "invalid_settings", message: ".unfold/config.toml:2:3: bad pattern" }));
    const dialog = await open("general");
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "orders" } });
    fireEvent.click(save(dialog));
    expect(await within(dialog).findByRole("alert")).toBeTruthy();
    expect(within(dialog).getByRole("alert").textContent).toContain("Line 2, column 3: bad pattern");
    expect(closed).toBe(0);
    expect(within(dialog).getByLabelText("Name")).toHaveProperty("value", "orders");
  });

  it("won't save a name other projects can't use", async () => {
    const fake = install();
    const dialog = await open("general");
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "no spaces" } });
    fireEvent.click(save(dialog));
    expect(await within(dialog).findByText(/starting with a letter or digit/)).toBeTruthy();
    expect(fake.callsOf("update_project_config")).toHaveLength(0);
  });

  it("asks before closing with staged changes", async () => {
    install();
    const dialog = await open("detectors");
    fireEvent.click(within(dialog).getByRole("switch", { name: "Express routes" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    const confirm = await screen.findByRole("alertdialog", { name: "Discard changes?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Keep editing" }));
    expect(closed).toBe(0);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Discard changes" }));
    expect(closed).toBe(1);
  });

  it("applies background updates at once, outside the staged changes", async () => {
    const fake = install();
    const dialog = await open("general");
    const toggle = await within(dialog).findByRole("switch", { name: "Background updates" });
    await waitFor(() => {
      expect(toggle).toHaveProperty("disabled", false);
    });
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(fake.callsOf("set_background_updates")).toHaveLength(1);
    });
    expect(save(dialog)).toHaveProperty("disabled", true);
  });
});
