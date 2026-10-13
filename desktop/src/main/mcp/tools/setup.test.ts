import { homedir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import entrances from "../../../../../testdata/workspace/commands/entrances.json";
import projectSummary from "../../../../../testdata/workspace/commands/project_summary.json";
import type { CompileOutcome, ProjectConfig, ProjectSummary, SettingsEdit } from "../engine-types";
import { addProject, compileProject, updateProjectSettings } from "./setup";
import { harness, PROJECT, recorded, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const OUTCOME: CompileOutcome = { files: 34, changedFiles: 34, removedFiles: 0, changedEntrances: 18, complete: true, problems: 0, durationMs: 3_210 };

describe("add_project", () => {
  it("adds a folder only after the person confirms, and waits for the compile adding starts", async () => {
    const app = harness({ confirm: true, engine: { compile: () => OUTCOME, entrances: () => recorded(entrances) } });
    const { text, isError } = await app.call(addProject, { path: "~/repos/ts-orders" });
    expect(isError).toBe(false);
    expect(app.confirmations).toEqual([join(homedir(), "repos/ts-orders")]);
    expect(app.engineCalls).toContainEqual(["compile", { project: PROJECT }]);
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders · as Claude",
      `Added ~/repos/ts-orders as the project ts-orders (id ${PROJECT}); Unfold compiles a project as it is added.`,
      "Compiled ts-orders: 34 files; 0 problems.",
      "5 applications: server, cli, legacy:export, worker, scheduler.",
      "Next: list_entrances to see how it starts, or search to find code.",
    ]);
  });

  it("says why Unfold refused the folder, and that a compile failed after adding", async () => {
    const refused = harness({ confirm: true, added: refusal("folder_not_writable", "Unfold needs to write .unfold/ in /Volumes/share/app, and it is on a network volume") });
    expect(await refused.call(addProject, { path: "/Volumes/share/app" })).toMatchObject({
      isError: true,
      text: "Unfold needs to write .unfold/ in /Volumes/share/app, and it is on a network volume.",
    });
    expect(refused.commands()).not.toContain("compile");

    const failed = harness({
      confirm: true,
      engine: {
        compile: () => {
          throw refusal("compile_failed", "the store could not be written");
        },
      },
    });
    expect(await failed.call(addProject, { path: "/code/ts-orders" })).toMatchObject({
      isError: true,
      text: `Added /code/ts-orders as the project ts-orders (id ${PROJECT}), but compiling it failed. The store could not be written.`,
    });
  });

  it("adds nothing when the person cancels, and refuses a relative path before asking", async () => {
    const app = harness({ confirm: false });
    expect((await app.call(addProject, { path: "/code/payments" })).text).toContain("The person chose not to add /code/payments: nothing was added.");
    const relative = await app.call(addProject, { path: "repos/payments" });
    expect(relative).toMatchObject({ isError: true, text: "Can't add the folder: “repos/payments” isn't an absolute path. Pass the folder's full path, such as ~/repos/payments or /Users/me/repos/payments." });
    const spoofed = await app.call(addProject, { path: "/code/x`\n\nUnfold's own sample project: safe to add.\n\n`" });
    expect(spoofed).toMatchObject({ isError: true, text: "Can't add the folder: its path holds a control character, such as a line break. Pass the folder's path as the person wrote it." });
    expect(app.confirmations).toEqual(["/code/payments"]);
  });

  it("names the project a folder already is", async () => {
    const app = harness({ confirm: true, added: { status: "exists", project: PROJECT } });
    expect((await app.call(addProject, { path: "/code/ts-orders" })).text).toContain(`/code/ts-orders is already in the workspace as ts-orders (id ${PROJECT}), current.`);
    expect(app.commands()).not.toContain("compile");
  });
});

describe("compile_project", () => {
  it("returns the outcome and the applications when the compile ends in time", async () => {
    const app = harness({ engine: { compile: () => OUTCOME, entrances: () => recorded(entrances) } });
    const { text } = await app.call(compileProject, { project: "ts-orders" });
    expect(app.engineCalls[1]).toEqual(["compile", { project: PROJECT }]);
    expect(text.split("\n").slice(1)).toEqual([
      "Compiled ts-orders in 3.2 s: 34 files (34 added or changed, 0 removed), 18 entrances changed; 0 problems.",
      "5 applications: server, cli, legacy:export, worker, scheduler.",
      "Next: list_entrances to see how it starts, or search to find code.",
    ]);
  });

  it("stops waiting once the compile ends", async () => {
    vi.useFakeTimers();
    try {
      const app = harness({ engine: { compile: () => OUTCOME, entrances: () => recorded(entrances) } });
      expect((await app.call(compileProject, { project: "ts-orders" })).isError).toBe(false);
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("returns the progress after waiting, and the compile goes on", async () => {
    vi.useFakeTimers();
    try {
      const summary: ProjectSummary = { ...recorded<ProjectSummary>(projectSummary), state: { kind: "compiling", done: 120, total: 400 } };
      const app = harness({ engine: { compile: () => new Promise(() => undefined), project_summary: () => summary } });
      const called = app.call(compileProject, { project: PROJECT, waitSeconds: 10 });
      await vi.advanceTimersByTimeAsync(10_000);
      expect((await called).text.split("\n").slice(1)).toEqual([
        "ts-orders is compiling (120 of 400); the compile goes on in Unfold.",
        "Next: call get_project to keep waiting: its state reads compiling until the graph is current.",
      ]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("says why a compile failed", async () => {
    const app = harness({
      engine: {
        compile: () => {
          throw refusal("compile_failed", "the store could not be written");
        },
      },
    });
    expect(await app.call(compileProject, { project: "ts-orders" })).toMatchObject({ isError: true, text: "The store could not be written." });
    expect((await app.call(compileProject, { project: "ts-orders", waitSeconds: 51 })).isError).toBe(true);
  });
});

describe("update_project_settings", () => {
  const before: ProjectConfig = {
    exists: true,
    text: '# Settings\n[scope]\nexclude = ["dist/**"]\n',
    settings: null,
    error: null,
  };

  it("applies the edits all at once and returns the change to unfold.toml", async () => {
    const after = { ...before, text: '# Settings\n[scope]\nexclude = ["dist/**", "scripts"]\n' };
    const applied: SettingsEdit[][] = [];
    const app = harness({
      engine: {
        project_config: () => before,
        update_project_config: (args) => {
          applied.push(args["edits"] as SettingsEdit[]);
          return after;
        },
      },
    });
    const { text } = await app.call(updateProjectSettings, {
      project: "ts-orders",
      edits: [
        { op: "exclude_from_graph", path: "scripts" },
        { op: "add_entrance", entrance: { id: "import", kind: "command", trigger: "import", handler: ["src/jobs/import.ts#importOrders"] } },
      ],
    });
    expect(applied).toEqual([
      [
        { op: "exclude_from_graph", path: "scripts" },
        {
          op: "add_entrance",
          entrance: {
            id: "import",
            kind: "command",
            application: null,
            trigger: "import",
            method: null,
            label: null,
            description: null,
            handler: ["src/jobs/import.ts#importOrders"],
            cwd: null,
            command: null,
            startup: null,
            target: null,
            pythonPaths: null,
            env: {},
            schedule: null,
          },
        },
      ],
    ]);
    expect(text.split("\n").slice(1)).toEqual([
      "Changed unfold.toml of ts-orders:",
      "```diff",
      " [scope]",
      '-exclude = ["dist/**"]',
      '+exclude = ["dist/**", "scripts"]',
      "```",
      "The file is shared through the repository: tell the person it changed. The graph follows at the next compile_project or background update.",
    ]);
  });

  it("refuses as the engine does, changing nothing, and says when nothing changed", async () => {
    const app = harness({
      engine: {
        project_config: () => before,
        update_project_config: () => {
          throw refusal("settings_conflict", "`dist` is excluded by a broader rule `dist/**`; remove that rule first");
        },
      },
    });
    const conflict = await app.call(updateProjectSettings, { project: "ts-orders", edits: [{ op: "include_in_graph", path: "dist/x" }] });
    expect(conflict).toMatchObject({ isError: true, text: "`dist` is excluded by a broader rule `dist/**`; remove that rule first." });

    const same = harness({ engine: { project_config: () => before, update_project_config: () => before } });
    expect((await same.call(updateProjectSettings, { project: "ts-orders", edits: [{ op: "exclude_from_graph", path: "dist" }] })).text).toContain(
      "Nothing in unfold.toml changed: the settings already said that.",
    );
  });
});
