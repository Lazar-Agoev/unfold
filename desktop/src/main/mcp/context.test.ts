import { describe, expect, it, vi } from "vitest";
import { AgentBridge } from "../agents/bridge";
import { ViewTracker } from "../agents/views";
import { EngineRequestError } from "../engine-host";
import { toolContext, type McpServices } from "./context";
import { ToolError } from "./results";
import { NO_RENDERER } from "./tools/testing";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const ROWS = [
  { id: "p-aaaaaaaaaa", name: "shop", path: "/code/shop" },
  { id: "p-bbbbbbbbbb", name: "payments", path: "/code/payments" },
  { id: "p-cccccccccc", name: "app", path: "/code/main/app" },
  { id: "p-dddddddddd", name: "app", path: "/code/feature/app" },
];

function setup(overrides: Partial<McpServices> = {}) {
  const engineCalls: string[] = [];
  const confirmations: [string, string][] = [];
  const added: string[] = [];
  const services: McpServices = {
    bridge: new AgentBridge({ window: { get: () => undefined, current: () => undefined, deliver: () => undefined, focus: () => undefined }, openWindow: () => Promise.reject(new Error("no window")), views: new ViewTracker() }),
    renderer: NO_RENDERER,
    engine: (command) => {
      engineCalls.push(command);
      return Promise.resolve({ projects: ROWS });
    },
    engineAs: () => Promise.reject(new Error("no window commands here")),
    confirmAddProject: (agent, path) => {
      confirmations.push([agent.name, path]);
      return Promise.resolve(true);
    },
    addProject: (path) => {
      added.push(path);
      return Promise.resolve({ status: "added", project: "p-cccccccccc" });
    },
    ...overrides,
  };
  return { context: toolContext(services, CLAUDE, new AbortController().signal), engineCalls, confirmations, added };
}

describe("finding projects", () => {
  it("by id, by name, or by a link to one of its boards, reading the workspace once per call", async () => {
    const { context, engineCalls } = setup();
    expect((await context.project("p-bbbbbbbbbb")).name).toBe("payments");
    expect((await context.project(" shop ")).id).toBe("p-aaaaaaaaaa");
    expect((await context.project("unfold://payments/.unfold/boards/refunds.json?page=page-1")).id).toBe("p-bbbbbbbbbb");
    expect(engineCalls).toEqual(["workspace"]);
    await expect(context.project("orders")).rejects.toThrow(
      "Can't find the project: no project named or with the id “orders” is in the workspace. list_projects shows each project's name and id.",
    );
  });

  it("refuses a folder name two projects share, and takes either one's id", async () => {
    const { context } = setup();
    await expect(context.project("app")).rejects.toThrow(
      "Can't tell which project you mean: 2 projects have a folder named “app”: p-cccccccccc (/code/main/app), p-dddddddddd (/code/feature/app). Pass the project's id.",
    );
    expect((await context.project("p-dddddddddd")).path).toBe("/code/feature/app");
    await expect(context.project("unfold://app/.unfold/boards/a.json")).rejects.toThrow("The link can't name one project: 2 projects have a folder named “app”");
    expect((await context.project("unfold://p-dddddddddd/.unfold/boards/a.json")).id).toBe("p-dddddddddd");
  });

  it("quotes an unknown name short", async () => {
    const { context } = setup();
    const refused = await context.project("x".repeat(40_000)).then(
      () => "",
      (error: unknown) => (error instanceof Error ? error.message : ""),
    );
    expect(refused).toMatch(/^Can't find the project: no project named or with the id “x+…” is in the workspace\./);
    expect(refused.length).toBeLessThan(300);
  });

  it("resolves links, and says why text isn't one", async () => {
    const { context } = setup();
    expect(await context.link("unfold://shop/.unfold/boards/a.json?page=p1&item=c1")).toEqual({
      project: "p-aaaaaaaaaa",
      board: ".unfold/boards/a.json",
      page: "p1",
      item: "c1",
    });
    await expect(context.link("https://example.com")).rejects.toThrow(ToolError);
    await expect(context.link("unfold://elsewhere/b.json")).rejects.toThrow(/No project named or with the id “elsewhere”/);
  });
});

describe("adding a project", () => {
  it("adds the folder only after the person confirms", async () => {
    const { context, confirmations, added } = setup();
    expect(await context.addProject("/Users/me/repos/payments")).toEqual({ status: "added", project: "p-cccccccccc" });
    expect(confirmations).toEqual([["Claude", "/Users/me/repos/payments"]]);
    expect(added).toEqual(["/Users/me/repos/payments"]);
  });

  it("adds nothing when the person cancels", async () => {
    const { context, added } = setup({ confirmAddProject: () => Promise.resolve(false) });
    expect(await context.addProject("/Users/me/repos/payments")).toBeNull();
    expect(added).toEqual([]);
  });

  it("turns the engine's refusal into one the agent can act on", async () => {
    const { context } = setup({
      addProject: () => Promise.reject(new EngineRequestError({ code: "not_a_folder", message: "`/tmp/x` is not a folder" })),
    });
    await expect(context.addProject("/tmp/x")).rejects.toThrow(new ToolError("`/tmp/x` is not a folder.", "not_a_folder"));
  });
});

describe("engine requests", () => {
  it("add what to do to the engine's reason", async () => {
    const { context } = setup({
      engine: () => Promise.reject(new EngineRequestError({ code: "not_compiled", message: "the project has no graph yet" })),
    });
    await expect(context.engine("entrances", { project: "p-aaaaaaaaaa" })).rejects.toThrow(
      new ToolError("The project has no graph yet. Call compile_project first (ask the person, unless they asked you to set things up).", "not_compiled"),
    );
  });
});
