import { describe, expect, it } from "vitest";
import type { ApplicationSpec, ImportedLaunch, ManualEntranceView } from "../ipc/generated";
import {
  checkSpecOf,
  draftOfImport,
  draftOfOverride,
  draftOfPrefill,
  draftOfTable,
  effectiveApplication,
  entranceId,
  hasProblems,
  newDraft,
  overrideOf,
  specOf,
  validateDraft,
  type LaunchDraft,
} from "./draft";

const RECIPE: ApplicationSpec = { id: "recipe-cost", name: "Recipe cost API", description: null };

function table(extra: Partial<ManualEntranceView>): ManualEntranceView {
  return {
    id: "t",
    kind: "process",
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
    line: 4,
    ...extra,
  };
}

describe("launch drafts and specs", () => {
  it("round-trips a saved launch with every field", () => {
    const saved = table({
      id: "dashboard-web",
      application: "recipe-cost",
      label: "Web",
      description: "The dashboard.",
      cwd: "data-pipelines",
      command: 'sh -c "uvicorn dashboard_builder.app:app"',
      target: { type: "python_asgi", module: "dashboard_builder.app", attribute: "app", factory: false },
      pythonPaths: ["data-pipelines"],
      env: { PYTHONPATH: "." },
      schedule: "30 7 * * *",
    });
    const draft = draftOfTable(saved, [RECIPE]);
    expect(draft.application).toEqual(RECIPE);
    expect(draft.scheduled).toBe(true);
    const { line: _line, ...spec } = saved;
    expect(specOf(draft, "dashboard-web", [RECIPE])).toEqual(spec);
  });

  it("round-trips a handler entrance and leaves a launch's fields out of it", () => {
    const saved = table({ id: "orders", kind: "http", method: "POST", trigger: "/orders", handler: ["src/auth.ts#requireUser", "src/orders.ts#create"] });
    const draft = draftOfTable(saved, []);
    expect(draft.intent).toBe("handler");
    const { line: _line, ...spec } = saved;
    expect(specOf(draft, "orders", [])).toEqual(spec);
    // Switching to a worker drops the method.
    expect(specOf({ ...draft, kind: "worker" }, "orders", []).method).toBeNull();
  });

  it("leaves blanks, the project's folder, and an empty schedule out", () => {
    const draft: LaunchDraft = { ...newDraft("process"), command: "  python3 main.py ", cwd: ".", label: " ", scheduled: false, schedule: "0 5 * * *" };
    expect(specOf(draft, "launch", [])).toMatchObject({ command: "python3 main.py", cwd: null, label: null, schedule: null, target: null, pythonPaths: null, env: {} });
  });

  it("starts from a prefill or a deploy-file import", () => {
    const prefill = draftOfPrefill({ command: "uvicorn api:app", cwd: "food-cost", label: "Start API", application: RECIPE, schedule: "30 7 * * *", env: { A: "1" } });
    expect(prefill).toMatchObject({ intent: "process", command: "uvicorn api:app", cwd: "food-cost", scheduled: true, env: [{ key: "A", value: "1" }] });
    const imported: ImportedLaunch = {
      service: "dashboard-builder",
      serviceType: "web",
      application: { id: "dashboard-builder", name: "Dashboard builder", description: "Builds dashboards." },
      id: "dashboard-builder-web",
      label: "Web",
      command: null,
      cwd: "data-pipelines",
      schedule: null,
      description: null,
      env: {},
      copiedFrom: "data-pipelines/Dockerfile.dashboard-builder, CMD",
      note: null,
      renderFile: "render.yaml",
    };
    expect(draftOfImport(imported)).toMatchObject({ command: "", cwd: "data-pipelines", label: "Web", scheduled: false, application: imported.application });
  });

  it("previews only what's set", () => {
    expect(checkSpecOf(newDraft("process"), null)).toEqual({ cwd: ".", env: {} });
    const draft: LaunchDraft = {
      ...newDraft("process"),
      command: "uvicorn a:app",
      cwd: "api",
      target: { type: "python_module", module: "a", path: "", attribute: "ignored", factory: false },
      pythonPaths: ["api"],
      env: [{ key: "X", value: "1" }],
    };
    expect(checkSpecOf(draft, "web")).toEqual({ id: "web", command: "uvicorn a:app", cwd: "api", target: { type: "python_module", module: "a" }, pythonPaths: ["api"], env: { X: "1" } });
  });

  it("writes a detected launch's override", () => {
    const match = { detector: "package.script", path: "apps/api/package.json", pointer: "/scripts/start" };
    const draft = draftOfOverride({ match, application: "recipe-cost", label: "Start", description: null, schedule: null }, [RECIPE]);
    expect(draft.application).toEqual(RECIPE);
    expect(overrideOf({ ...draft, description: "Serves it." }, match, [RECIPE])).toEqual({ match, application: "recipe-cost", label: "Start", description: "Serves it.", schedule: null });
  });
});

describe("IDs", () => {
  it("names a launch after its application and label, unique among the taken ones", () => {
    const draft: LaunchDraft = { ...newDraft("process"), application: RECIPE, label: "Start API" };
    expect(entranceId(draft, [], [RECIPE])).toBe("recipe-cost-start-api");
    expect(entranceId(draft, ["recipe-cost-start-api", "recipe-cost-start-api-2"], [RECIPE])).toBe("recipe-cost-start-api-3");
    expect(entranceId(newDraft("process"), ["launch"], [])).toBe("launch-2");
  });

  it("names a handler after its trigger, else its label, else its kind", () => {
    expect(entranceId({ ...newDraft("handler"), trigger: "/orders/:id" }, [], [])).toBe("orders-id");
    expect(entranceId({ ...newDraft("handler"), label: "Shut down" }, [], [])).toBe("shut-down");
    expect(entranceId(newDraft("handler"), [], [])).toBe("command");
  });

  it("gives a new application an ID no declared one has", () => {
    const draft: LaunchDraft = { ...newDraft("process"), newApplication: " Recipe cost " };
    expect(effectiveApplication(draft, [{ ...RECIPE, name: "Other" }])).toEqual({ id: "recipe-cost-2", name: "Recipe cost", description: null });
    expect(effectiveApplication({ ...draft, newApplication: "  " }, [])).toBeNull();
  });
});

describe("validation", () => {
  it("asks a launch for a command or a target", () => {
    expect(validateDraft(newDraft("process")).command).toMatch(/Paste the command/);
    expect(hasProblems(validateDraft({ ...newDraft("process"), command: "python3 main.py" }))).toBe(false);
    const target = { type: "python_asgi" as const, path: "", module: "a", attribute: "", factory: false };
    expect(validateDraft({ ...newDraft("process"), target }).target).toMatch(/attribute/);
  });

  it("checks the schedule, new application, paths, and environment", () => {
    const draft: LaunchDraft = {
      ...newDraft("process"),
      command: "x",
      scheduled: true,
      newApplication: "",
      pythonPaths: ["a", " "],
      env: [
        { key: "A", value: "1" },
        { key: "A", value: "2" },
      ],
    };
    const problems = validateDraft(draft);
    expect(problems.schedule).toMatch(/schedule/);
    expect(problems.application).toMatch(/Name/);
    expect(problems.pythonPaths).toMatch(/empty/);
    expect(problems.env).toMatch(/once/);
  });

  it("checks handler locators as Rust does", () => {
    expect(validateDraft(newDraft("handler")).handlerList).toMatch(/at least one/);
    const problems = validateDraft({ ...newDraft("handler"), handlers: ["src/a.ts#a.b.c", "src/b.py#f"] });
    expect(problems.handlers[0]).toMatch(/Owner\.member/);
    expect(problems.handlerList).toMatch(/mix/);
  });
});
