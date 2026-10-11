// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { EntrancesView, LaunchCheck, LaunchDraftSpec, LaunchRow, ManualEntranceView, ProjectConfig, SettingsEdit } from "../ipc/generated";
import type { DialogRequest } from "../state/overlays";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { LaunchEditorDialog } from "./LaunchEditor";
import { RemoveEntranceDialog } from "./RemoveEntranceDialog";

const PROJECT = "p-1";

const TABLE: ManualEntranceView = {
  id: "",
  kind: "command",
  application: null,
  trigger: null,
  method: null,
  label: null,
  description: null,
  handler: [],
  cwd: null,
  command: null,
  startup: null,
  target: null,
  pythonPaths: null,
  env: {},
  schedule: null,
  line: 1,
};

const SH_C = 'sh -c "uvicorn dashboard_builder.app:app --port ${PORT:-8010}"';

function config(extra: Partial<ProjectConfig> = {}): ProjectConfig {
  return {
    exists: true,
    text: "# Shared settings.\n",
    error: null,
    name: { value: "pathway-agents", isDefault: true },
    settings: {
      languages: null,
      include: [],
      exclude: [],
      disabledDetectors: [],
      pythonProjects: [],
      evidence: [],
      applications: [{ id: "dashboard-builder", name: "Dashboard builder", description: null }],
      overrides: [],
      entrances: [
        { ...TABLE, id: "shutdown", kind: "event", trigger: "SIGTERM", label: "Shut down", handler: ["src/server.py#shutdown"], line: 9 },
        { ...TABLE, id: "dashboard-web", kind: "process", application: "dashboard-builder", label: "Web", cwd: "data-pipelines", command: SH_C, line: 58 },
      ],
    },
    ...extra,
  };
}

const READS: LaunchCheck = {
  parts: [
    { text: "python3 -m uvicorn", meaning: "Runs Uvicorn through Python", irrelevant: false },
    { text: "api:app", meaning: "Serves app from the module api", irrelevant: false },
    { text: "--host … --port $PORT", meaning: "Server options. They don't change the code", irrelevant: true },
  ],
  problem: null,
  suggestion: null,
  targetSetByHand: false,
  importPaths: [{ path: "data-pipelines/food-cost", origin: "from --app-dir" }],
  starts: { path: "data-pipelines/food-cost/api.py", name: "app", description: "A FastAPI application with 2 routes", status: "ok", reason: null },
  imports: { count: 18, examples: ["compute_recipe_costs", "conversion_router", "utils.llm"] },
  routes: null,
  fromCurrentGraph: true,
};

/** What an empty draft reads as. */
const NOTHING: LaunchCheck = { ...READS, parts: [], importPaths: [], starts: null, imports: null };

const UNREADABLE: LaunchCheck = {
  parts: [],
  problem: { label: "Can't read sh -c", message: "It doesn't look inside a quoted shell command." },
  suggestion: {
    quoted: "uvicorn dashboard_builder.app:app",
    target: { type: "python_asgi", module: "dashboard_builder.app", attribute: "app", factory: false },
    pythonPaths: ["data-pipelines"],
  },
  targetSetByHand: false,
  importPaths: [],
  starts: null,
  imports: null,
  routes: { count: 5, application: null, shared: [{ path: "dashboard_builder/router.py", alsoIn: ["Invoice HITL API"], routes: 4 }] },
  fromCurrentGraph: true,
};

const DETECTED: LaunchRow = {
  key: "entrance:package.script:start",
  id: null,
  label: "start",
  folder: "apps/api",
  command: "npm run start",
  schedule: null,
  scheduleGloss: null,
  description: null,
  status: "ok",
  problem: null,
  note: null,
  targetSetByHand: false,
  starts: { path: "apps/api/src/server.ts", name: null, kind: "Node file" },
  importPaths: [],
  reaches: null,
  routes: null,
  declaredIn: { path: "apps/api/package.json", line: 5 },
  editable: "override",
  match: { detector: "package.script", path: "apps/api/package.json", pointer: "/scripts/start" },
};

const CATALOG: EntrancesView = {
  view: "overview",
  project: PROJECT,
  counts: { applications: 1, launches: 1, routes: 0, entrances: 1 },
  categories: [
    {
      category: "workers",
      applications: [{ id: "start", name: "start", description: null, origin: "inferred", entrances: 1, routes: 0, alsoIn: [], launches: [DETECTED] }],
    },
  ],
  unreached: 0,
};

let engine: FakeEngine | null = null;

interface Install {
  readonly project?: ProjectConfig;
  readonly check?: (draft: LaunchDraftSpec) => LaunchCheck;
  readonly update?: (edits: SettingsEdit[]) => ProjectConfig | Promise<ProjectConfig>;
}

function install({ project = config(), check = () => READS, update = () => project }: Install = {}): FakeEngine {
  engine = installFakeEngine({
    handlers: {
      project_config: () => project,
      project_summary: () => fixture("project_summary", "project_summary"),
      check_launch: (args) => check(args.draft),
      entrances: () => CATALOG,
      scope_dir: ({ path }) => ({ path, truncated: false, folders: path === "" ? [{ name: "data-pipelines", path: "data-pipelines", skipped: null, sources: 3 }] : [] }),
      update_project_config: (args) => update(args.edits),
    },
  });
  return engine;
}

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

type EditorRequest = Extract<DialogRequest, { readonly kind: "launchEditor" }>;

let closed = 0;

async function openEditor(request: EditorRequest): Promise<HTMLElement> {
  closed = 0;
  const { rerender } = renderWithClient(
    <LaunchEditorDialog
      request={request}
      onClose={() => {
        closed += 1;
        rerender(<></>);
      }}
    />,
  );
  return await screen.findByRole("dialog");
}

function edits(fake: FakeEngine, call = 0): SettingsEdit[] {
  return (fake.callsOf("update_project_config")[call]?.["edits"] ?? []) as SettingsEdit[];
}

describe("the launch editor", () => {
  it("adds a launch, previewing what Unfold reads as the command is typed", async () => {
    const fake = install({ check: (draft) => (draft.command ? READS : NOTHING) });
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "add" });
    expect(within(dialog).getByRole("heading", { name: "Add launch" })).toBeTruthy();
    expect(await within(dialog).findByText("Paste the command to see what Unfold reads from it.")).toBeTruthy();
    expect(within(dialog).getByText("pathway-agents · unfold.toml · shared with your team")).toBeTruthy();
    expect(document.activeElement).toBe(within(dialog).getByLabelText("Command"));
    fireEvent.change(within(dialog).getByLabelText("Command"), { target: { value: "python3 -m uvicorn api:app --app-dir food-cost" } });
    expect(await within(dialog).findByText("Runs Uvicorn through Python")).toBeTruthy();
    // Only the settled command is asked for.
    expect(fake.callsOf("check_launch").at(-1)).toEqual({ project: PROJECT, draft: { command: "python3 -m uvicorn api:app --app-dir food-cost", cwd: ".", env: {} } });
    expect(within(dialog).getByText("Preview from the current graph · compiles on save")).toBeTruthy();
    expect(within(dialog).getByText("18 more imports resolve to code in this repository")).toBeTruthy();
    expect(within(dialog).getByText("compute_recipe_costs, conversion_router, and utils.llm now open as code instead of reading as packages.")).toBeTruthy();
    expect(within(dialog).getByText("from --app-dir")).toBeTruthy();

    fireEvent.change(within(dialog).getByLabelText("Application"), { target: { value: "\u0000new" } });
    fireEvent.change(within(dialog).getByLabelText("Application"), { target: { value: "Recipe cost API" } });
    fireEvent.change(within(dialog).getByLabelText("Launch name"), { target: { value: "Start API" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save launch" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(edits(fake)).toEqual([
      { op: "set_application", application: { id: "recipe-cost-api", name: "Recipe cost API", description: null } },
      {
        op: "add_entrance",
        entrance: {
          id: "recipe-cost-api-start-api",
          kind: "process",
          application: "recipe-cost-api",
          trigger: null,
          method: null,
          label: "Start API",
          description: null,
          handler: [],
          cwd: null,
          command: "python3 -m uvicorn api:app --app-dir food-cost",
          startup: null,
          target: null,
          pythonPaths: null,
          env: {},
          schedule: null,
        },
      },
    ]);
    await waitFor(() => {
      expect(closed).toBe(1);
    });
  });

  it("asks for a command before saving a launch", async () => {
    const fake = install();
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "add" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save launch" }));
    expect(await within(dialog).findByText(/Paste the command, or set the target by hand/)).toBeTruthy();
    expect(fake.callsOf("update_project_config")).toHaveLength(0);
  });

  it("fills an unreadable launch's target from the quoted part, with Undo", async () => {
    const fake = install({ check: (draft) => (draft.target ? { ...UNREADABLE, targetSetByHand: true } : UNREADABLE) });
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "edit", id: "dashboard-web" });
    expect(within(dialog).getByRole("heading", { name: "Edit launch" })).toBeTruthy();
    expect(within(dialog).getByText("pathway-agents · unfold.toml:58 · shared with your team")).toBeTruthy();
    expect(await within(dialog).findByText("Can't read sh -c")).toBeTruthy();
    expect(await within(dialog).findByText("Filled from the quoted part:")).toBeTruthy();
    expect(within(dialog).getByLabelText<HTMLSelectElement>("Target").value).toBe("python_asgi");
    expect(within(dialog).getByLabelText<HTMLInputElement>("Target module").value).toBe("dashboard_builder.app");
    expect(within(dialog).getByLabelText<HTMLInputElement>("Import path 1").value).toBe("data-pipelines");
    expect(within(dialog).getByText("set by hand")).toBeTruthy();
    expect(within(dialog).getByText("5 routes join Dashboard builder")).toBeTruthy();
    expect(
      within(dialog).getByText("4 of them come from dashboard_builder/router.py, which Invoice HITL API also mounts. Each application lists them under its own mount."),
    ).toBeTruthy();

    fireEvent.click(within(dialog).getByRole("button", { name: "Undo" }));
    expect(within(dialog).getByLabelText<HTMLSelectElement>("Target").value).toBe("");
    expect(within(dialog).getByText("read from the command")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Fill from it" }));

    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(edits(fake)).toEqual([
      {
        op: "replace_entrance",
        id: "dashboard-web",
        entrance: expect.objectContaining({
          id: "dashboard-web",
          command: SH_C,
          cwd: "data-pipelines",
          application: "dashboard-builder",
          target: { type: "python_asgi", module: "dashboard_builder.app", attribute: "app", factory: false },
          pythonPaths: ["data-pipelines"],
        }) as unknown,
      },
    ]);
  });

  it("starts a handler entrance from a card's definition", async () => {
    const fake = install();
    const dialog = await openEditor({
      kind: "launchEditor",
      project: PROJECT,
      mode: "add",
      intent: "handler",
      handler: { path: "src/routes/orders.py", kind: "function", name: "create", qualifiedName: "create", line: 3 },
    });
    expect(within(dialog).getByRole("heading", { name: "Add entrance" })).toBeTruthy();
    expect(within(dialog).getByRole("radio", { name: "Handles a route, queue, or event" }).getAttribute("aria-checked")).toBe("true");
    expect(within(dialog).getByLabelText<HTMLInputElement>("Handler 1").value).toBe("src/routes/orders.py#create");
    expect(document.activeElement).toBe(within(dialog).getByLabelText("Trigger"));
    fireEvent.change(within(dialog).getByLabelText("Kind"), { target: { value: "http" } });
    fireEvent.change(within(dialog).getByLabelText("Method"), { target: { value: "POST" } });
    fireEvent.change(within(dialog).getByLabelText("Trigger"), { target: { value: "/orders" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add handler" }));
    fireEvent.change(within(dialog).getByLabelText("Handler 2"), { target: { value: "src/auth.py#require_user" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Run handler 2 earlier" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save entrance" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(edits(fake)).toEqual([
      {
        op: "add_entrance",
        entrance: expect.objectContaining({
          id: "orders",
          kind: "http",
          method: "POST",
          trigger: "/orders",
          handler: ["src/auth.py#require_user", "src/routes/orders.py#create"],
          command: null,
        }) as unknown,
      },
    ]);
    // A handler isn't previewed.
    expect(fake.callsOf("check_launch")).toHaveLength(0);
  });

  it("describes a detected launch with an override", async () => {
    const fake = install();
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "override", key: DETECTED.key });
    expect(within(dialog).getByText("npm run start")).toBeTruthy();
    expect(within(dialog).queryByRole("radiogroup", { name: "What it does" })).toBeNull();
    expect(within(dialog).getByText("Detected in apps/api/package.json")).toBeTruthy();
    fireEvent.change(within(dialog).getByLabelText("Application"), { target: { value: "dashboard-builder" } });
    fireEvent.change(within(dialog).getByLabelText("Launch name"), { target: { value: "Start API" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(edits(fake)).toEqual([
      { op: "set_override", override: { match: DETECTED.match, application: "dashboard-builder", label: "Start API", description: null, schedule: null } },
    ]);
  });

  it("steps through imported launches, declaring a new application once", async () => {
    let project = config();
    const fake = install({
      update: (sent) => {
        const declared = sent.flatMap((edit) => (edit.op === "set_application" ? [edit.application] : []));
        const settings = project.settings;
        if (settings) {
          project = { ...project, settings: { ...settings, applications: [...settings.applications, ...declared] } };
        }
        return project;
      },
    });
    const launch = {
      service: "recipe-cost",
      serviceType: "web" as const,
      application: { id: "recipe-cost", name: "Recipe cost API", description: null },
      label: "Web",
      command: "uvicorn api:app",
      cwd: "food-cost",
      schedule: null,
      description: null,
      env: {},
      note: null,
      renderFile: "render.yaml",
    };
    const dialog = await openEditor({
      kind: "launchEditor",
      project: PROJECT,
      mode: "review",
      launches: [
        { ...launch, id: "recipe-cost-web", copiedFrom: "food-cost/Dockerfile, CMD" },
        { ...launch, id: "recipe-cost-worker", label: "Worker", copiedFrom: null },
        { ...launch, id: "recipe-cost-cron", label: "Cron", copiedFrom: null },
      ],
    });
    expect(within(dialog).getByText("1 of 3")).toBeTruthy();
    expect(within(dialog).getByText("Copied from food-cost/Dockerfile, CMD.")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save launch" }));
    expect(await screen.findByText("2 of 3")).toBeTruthy();
    expect(edits(fake, 0).map((edit) => edit.op)).toEqual(["set_application", "add_entrance"]);
    expect(screen.getByLabelText<HTMLInputElement>("Launch name").value).toBe("Worker");
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(await screen.findByText("3 of 3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save launch" }));
    await waitFor(() => {
      expect(closed).toBe(1);
    });
    expect(edits(fake, 1)).toEqual([{ op: "add_entrance", entrance: expect.objectContaining({ id: "recipe-cost-cron", label: "Cron", application: "recipe-cost" }) as unknown }]);
  });

  it("won't save while unfold.toml doesn't parse", async () => {
    install({ project: config({ error: { message: "unfold.toml:3:1: unknown field `bogus`", line: 3, column: 1 } }) });
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "add" });
    expect(within(dialog).getByRole("alert").textContent).toContain("Fix unfold.toml first");
    expect(within(dialog).getByRole<HTMLButtonElement>("button", { name: "Save launch" }).disabled).toBe(true);
  });

  it("shows Rust's refusal with its line and column, and stays open", async () => {
    install({
      update: () => {
        // eslint-disable-next-line @typescript-eslint/only-throw-error -- the preload rejects with plain data
        throw { code: "invalid_settings", message: "unfold.toml:14:11: `cwd` must be a folder in the project" };
      },
    });
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "add", prefill: { command: "python3 main.py", cwd: "../x" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save launch" }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("Line 14, column 11:");
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(closed).toBe(0);
  });

  it("picks the working folder from the project's folders", async () => {
    const fake = install();
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "add", prefill: { command: "python3 main.py" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Runs in pathway-agents" }));
    fireEvent.click(await within(dialog).findByRole("button", { name: "data-pipelines" }));
    expect(within(dialog).getByRole("button", { name: "Runs in data-pipelines" })).toBeTruthy();
    await waitFor(() => {
      expect(fake.callsOf("check_launch").at(-1)).toMatchObject({ draft: { cwd: "data-pipelines" } });
    });
  });
});

describe("a saved launch Unfold can't read", () => {
  it("shows a target set from the quoted part as filled from it", async () => {
    const project = config();
    const saved = project.settings?.entrances[1];
    if (!saved || !project.settings) {
      throw new Error("fixture");
    }
    const filled = { ...saved, target: UNREADABLE.suggestion?.target ?? null, pythonPaths: ["data-pipelines"] };
    install({ project: { ...project, settings: { ...project.settings, entrances: [filled] } }, check: () => ({ ...UNREADABLE, targetSetByHand: true }) });
    const dialog = await openEditor({ kind: "launchEditor", project: PROJECT, mode: "edit", id: "dashboard-web" });
    expect(await within(dialog).findByText("Filled from the quoted part:")).toBeTruthy();
    expect(within(dialog).getByText("Target set by hand · compiles on save")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Undo" }));
    expect(within(dialog).getByLabelText<HTMLSelectElement>("Target").value).toBe("");
  });
});

describe("removing an entrance", () => {
  it("removes it by ID after asking", async () => {
    const fake = install();
    renderWithClient(<RemoveEntranceDialog request={{ kind: "removeEntrance", project: PROJECT, id: "shutdown", name: "Shut down" }} onClose={() => undefined} />);
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("Remove Shut down from unfold.toml?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([{ project: PROJECT, edits: [{ op: "remove_entrance", id: "shutdown" }] }]);
    });
  });
});
