// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntrancesArgs, EntrancesView, ProjectSummary } from "../../ipc";
import type * as Overlays from "../../state/overlays";
import { DEPLOY, INVOICE_SCOPE, INVOICE_START, NO_LAUNCHES, OVERVIEW, SUGGESTIONS, UNREACHED_SCOPE } from "../../test/entrances";
import { fixture, installFakeEngine, type EngineHandlers, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { HowItStarts } from "./HowItStarts";

const { openDialog, traceFromHome } = vi.hoisted(() => ({ openDialog: vi.fn(), traceFromHome: vi.fn(() => Promise.resolve()) }));
vi.mock("../../state/overlays", async (original) => ({ ...(await original<typeof Overlays>()), openDialog }));
vi.mock("../actions", () => ({ traceFromHome }));

const P = "p-orders0000";
let engine: FakeEngine | null = null;

function summary(): ProjectSummary {
  return { ...fixture("project_summary", "project_summary"), project: P, name: "pathway-agents" };
}

function catalog(overview: EntrancesView) {
  return (args: EntrancesArgs): EntrancesView => {
    if (args.unreached) {
      return UNREACHED_SCOPE;
    }
    if (args.application === "invoice-hitl") {
      return args.cursor === undefined ? INVOICE_SCOPE : { ...INVOICE_SCOPE, rows: [], nextCursor: null };
    }
    return overview;
  };
}

function start(handlers: EngineHandlers): FakeEngine {
  engine = installFakeEngine({ handlers });
  renderWithClient(<HowItStarts project={P} summary={summary()} />);
  return engine;
}

function section() {
  return screen.getByRole("region", { name: "How it starts" });
}

beforeEach(() => {
  openDialog.mockClear();
  traceFromHome.mockClear();
});

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("How it starts with launches", () => {
  it("lists applications by category with their launches, schedules, and what they handle", async () => {
    start({ entrances: catalog(OVERVIEW) });
    expect(await screen.findByText("4 applications · 5 launches · 14 routes. Hover a row to trace it on a new board.")).toBeTruthy();
    expect(screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual(["Needs a look", "Services", "Workers", "Scheduled"]);
    expect(screen.getByText("Unfold can't tell what a launch runs until you point it at the code")).toBeTruthy();
    expect(screen.getByText("Schedules as written in unfold.toml · times in the scheduler's zone")).toBeTruthy();

    const dashboard = screen.getByRole("region", { name: "Dashboard builder" });
    expect(within(dashboard).getByText("Query service called only by pathway-web's server.")).toBeTruthy();
    expect(within(dashboard).getAllByText("data-pipelines $")).toHaveLength(2);
    expect(within(dashboard).getByText("Can't read sh -c")).toBeTruthy();
    expect(within(dashboard).getByText("0 10 * * *")).toBeTruthy();
    expect(within(dashboard).getByText("daily at 10:00")).toBeTruthy();
    expect(within(dashboard).getByText("22 fn")).toBeTruthy();

    expect(within(screen.getByRole("region", { name: "Invoice HITL API" })).getByText("14 routes")).toBeTruthy();
    const worker = screen.getByRole("region", { name: "Review worker" });
    expect(within(worker).getByText("pathway-agents $")).toBeTruthy();
    expect(within(worker).getByText("npm run worker")).toBeTruthy();
    // Nothing but its launch: no expand chevron.
    expect(within(worker).queryByRole("button")).toBeNull();

    const evaluation = screen.getByRole("region", { name: "Purchase ingestion eval" });
    expect(within(evaluation).getByText("not indexed")).toBeTruthy();
    expect(within(evaluation).getByText("evals/ is in .gitignore, so this code isn't indexed")).toBeTruthy();
  });

  it("opens the editor from Fix…, Add launch, and a launch's menu", async () => {
    start({ entrances: catalog(OVERVIEW) });
    fireEvent.click(await screen.findByRole("button", { name: "Fix…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "edit", id: "dashboard-web" });

    fireEvent.click(within(section()).getByRole("button", { name: "Add launch" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "add" });

    fireEvent.contextMenu(screen.getByText("uvicorn api.main:app --host 0.0.0.0 --port 8000"));
    const menu = screen.getByRole("menu", { name: "Entrance menu" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Trace on a New Board", "Edit…", "Remove…"]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Remove…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "removeEntrance", project: P, id: "invoice-start", name: "Start API" });

    // A detected launch: Edit… opens an override of it, and there is nothing to remove.
    fireEvent.contextMenu(screen.getByText("npm run worker"));
    const detected = screen.getByRole("menu", { name: "Entrance menu" });
    expect(within(detected).queryByRole("menuitem", { name: "Remove…" })).toBeNull();
    fireEvent.click(within(detected).getByRole("menuitem", { name: "Edit…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "override", key: 'entrance:["package.script","process","","worker","src/worker.ts"]' });
  });

  it("expands an application to its launch, routes by path, and a router other applications mount", async () => {
    const fake = start({ entrances: catalog(OVERVIEW) });
    const invoice = await screen.findByRole("region", { name: "Invoice HITL API" });
    const toggle = within(invoice).getByRole("button", { name: "Invoice HITL API's entrances" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(await within(invoice).findByText("Start API")).toBeTruthy();
    expect(fake.callsOf("entrances")).toContainEqual({ project: P, application: "invoice-hitl", limit: 12 });
    expect(within(invoice).getByText("declared in unfold.toml")).toBeTruthy();
    expect(within(invoice).getByText("184 fn")).toBeTruthy();
    expect(within(invoice).getByText("/runs", { selector: ".home-scope-heading-title" })).toBeTruthy();
    expect(within(invoice).getByText("· mounted from dashboard_builder/router.py")).toBeTruthy();

    const shared = within(invoice).getByRole("button", { name: /\/model\/\{model\}, \/explore/ });
    expect(shared.textContent).toContain("also in");
    expect(shared.textContent).toContain("Dashboard builder");
    expect(shared.textContent).toContain("2 routes");
    expect(within(invoice).queryByRole("button", { name: /^GET \/model/ })).toBeNull();
    fireEvent.click(shared);
    expect(within(invoice).getByRole("button", { name: /^GET \/model\/\{model\}/ })).toBeTruthy();

    // A detected route has no menu.
    fireEvent.contextMenu(within(invoice).getByRole("button", { name: /^POST \/runs/ }));
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(within(invoice).getByRole("button", { name: /^POST \/runs/ }));
    expect(traceFromHome).toHaveBeenCalledWith(P, "entrance:post-runs");
    fireEvent.keyDown(within(invoice).getByRole("button", { name: /^LAUNCH Start API/ }), { key: "Enter" });
    expect(traceFromHome).toHaveBeenLastCalledWith(P, INVOICE_START.key);

    expect(within(invoice).getByText("10 more routes")).toBeTruthy();
    fireEvent.click(within(invoice).getByRole("button", { name: "Filter" }));
    expect(within(invoice).getByRole("searchbox", { name: "Filter Invoice HITL API's entrances" })).toBeTruthy();
    await waitFor(() => {
      expect(fake.callsOf("entrances")).toContainEqual({ project: P, application: "invoice-hitl", limit: 100 });
    });
    fireEvent.click(await within(invoice).findByRole("button", { name: "Show 10 more" }));
    await waitFor(() => {
      expect(fake.callsOf("entrances")).toContainEqual({ project: P, application: "invoice-hitl", limit: 100, cursor: "c5" });
    });
  });

  it("lists the entrances no launch reaches when opened", async () => {
    const fake = start({ entrances: catalog(OVERVIEW) });
    const disclosure = await screen.findByRole("button", { name: "Not reached from a launch · 2" });
    expect(fake.callsOf("entrances").some((args) => args["unreached"] === true)).toBe(false);
    fireEvent.click(disclosure);
    expect(await screen.findByRole("button", { name: /^COMMAND import/ })).toBeTruthy();
    expect(fake.callsOf("entrances")).toContainEqual({ project: P, unreached: true, limit: 100 });
    expect(screen.getByText("Commands")).toBeTruthy();

    // A configured handler entrance keeps Edit… and Remove….
    fireEvent.contextMenu(screen.getByRole("button", { name: /^COMMAND import/ }));
    const menu = screen.getByRole("menu", { name: "Entrance menu" });
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Edit…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "edit", id: "import" });
    fireEvent.contextMenu(screen.getByRole("button", { name: /^COMMAND report/ }));
    fireEvent.click(within(screen.getByRole("menu", { name: "Entrance menu" })).getByRole("menuitem", { name: "Remove…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "removeEntrance", project: P, id: "report", name: "report" });
  });
});

describe("How it starts before any launch", () => {
  it("explains what Unfold can't tell and offers launches from the deploy files and the code", async () => {
    start({ entrances: catalog(NO_LAUNCHES), launch_suggestions: () => SUGGESTIONS, import_deploy_files: () => DEPLOY });
    expect(await screen.findByText("31 routes, but no launches yet, so Unfold can't tell which service serves them.")).toBeTruthy();
    const card = screen.getByRole("region", { name: "Tell Unfold how pathway-agents starts" });
    expect(
      await within(card).findByText(
        "There are no package scripts or pyproject entry points here, so routes belong to no application, and imports like shared, api, and utils read as packages: 249 of 7,239 imports resolve. Give Unfold the commands you deploy with. It reads them and never runs them.",
      ),
    ).toBeTruthy();

    const deploy = await within(card).findByRole("region", { name: "From your deploy files" });
    expect(within(deploy).getByText("6 start commands")).toBeTruthy();
    expect(within(deploy).getByText("2 web services, 1 worker, and 3 cron jobs in render.yaml and data-pipelines/render.yaml")).toBeTruthy();
    expect(within(deploy).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "WEBinvoice-hitl-api",
      "WEBrecipe-cost-api",
      "WORKERreview-worker",
      "CRONtoken-refresh-daily30 7 * * *",
      "and 2 more",
    ]);
    fireEvent.click(within(deploy).getByRole("button", { name: "Review 6 launches…" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "review", launches: DEPLOY.launches });

    const code = within(card).getByRole("region", { name: "From your code" });
    expect(within(code).getByText("2 FastAPI apps that no launch reaches. Each suggestion opens prefilled; check it against how you deploy.")).toBeTruthy();
    expect(within(code).getByText("data-pipelines $ uvicorn api.main:app")).toBeTruthy();
    // The project's own folder reads as its name.
    expect(within(code).getByText("pathway-agents $ uvicorn server:app")).toBeTruthy();
    expect(within(code).getByText("12 routes")).toBeTruthy();
    fireEvent.click(within(code).getByRole("button", { name: "Add launch for server.py" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "add", prefill: { command: "uvicorn server:app", cwd: "." } });

    fireEvent.click(within(card).getByRole("button", { name: "Add a launch by hand" }));
    expect(openDialog).toHaveBeenLastCalledWith({ kind: "launchEditor", project: P, mode: "add" });
    expect(within(card).getByText("Launches are saved to unfold.toml, so your team gets them too")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Not reached from a launch · 31" })).toBeTruthy();
  });

  it("shows only the panels that have something to offer", async () => {
    start({
      entrances: catalog({ ...NO_LAUNCHES, counts: { applications: 0, launches: 0, routes: 0, entrances: 0 }, unreached: 0 }),
      launch_suggestions: () => ({ routes: 0, imports: { resolved: 0, total: 0 }, localLooking: [], apps: [] }),
      import_deploy_files: () => ({ files: [], launches: [] }),
    });
    expect(await screen.findByText("No launches or routes yet.")).toBeTruthy();
    const card = screen.getByRole("region", { name: "Tell Unfold how pathway-agents starts" });
    expect(await within(card).findByText(/^There are no package scripts or pyproject entry points here\. Give Unfold/)).toBeTruthy();
    expect(within(card).queryByRole("region", { name: "From your deploy files" })).toBeNull();
    expect(within(card).queryByRole("region", { name: "From your code" })).toBeNull();
    expect(within(card).getByRole("button", { name: "Add a launch by hand" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Not reached from a launch/ })).toBeNull();
  });
});
