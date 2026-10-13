// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntrancesArgs, EntrancesView, ProjectSummary } from "../../ipc";
import { INVOICE_SCOPE, INVOICE_START, NO_LAUNCHES, OVERVIEW, UNREACHED_SCOPE } from "../../test/entrances";
import { fixture, installFakeEngine, type EngineHandlers, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { HowItStarts } from "./HowItStarts";

const { traceFromHome } = vi.hoisted(() => ({ traceFromHome: vi.fn(() => Promise.resolve()) }));
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

  it("is read-only: a launch's menu only traces it, and nothing adds or edits launches", async () => {
    start({ entrances: catalog(OVERVIEW) });
    fireEvent.contextMenu(await screen.findByText("uvicorn api.main:app --host 0.0.0.0 --port 8000"));
    const menu = screen.getByRole("menu", { name: "Entrance menu" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Trace on a New Board"]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Trace on a New Board" }));
    expect(traceFromHome).toHaveBeenLastCalledWith(P, INVOICE_START.key);
    expect(within(section()).queryByRole("button", { name: "Add launch" })).toBeNull();
    expect(within(section()).queryByRole("button", { name: "Fix…" })).toBeNull();
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

    fireEvent.contextMenu(screen.getByRole("button", { name: /^COMMAND report/ }));
    fireEvent.click(within(screen.getByRole("menu", { name: "Entrance menu" })).getByRole("menuitem", { name: "Trace on a New Board" }));
    expect(traceFromHome).toHaveBeenLastCalledWith(P, "entrance:configured:report");
  });
});

describe("How it starts before any launch", () => {
  it("says why routes without launches belong to no service, and lists them as unreached", async () => {
    start({ entrances: catalog(NO_LAUNCHES) });
    expect(await screen.findByText("31 routes, but no launches yet, so Unfold can't tell which service serves them.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Not reached from a launch · 31" })).toBeTruthy();
    expect(within(section()).queryByRole("heading", { level: 3 })).toBeNull();
  });
});
