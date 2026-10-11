import { beforeEach, describe, expect, it, vi } from "vitest";
import { EngineHost, EngineRequestError } from "./engine-host";
import { forked, resetFakeElectron } from "./fake-electron";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

function started() {
  const host = new EngineHost({
    modulePath: "/app/out/main/engine-host.js",
    addonPath: "/app/unfold-node",
    folders: { appDataDir: "/data", logsDir: "/logs", fallbackCacheDir: "/cache" },
    windows: () => [],
  });
  host.start();
  const child = forked[0];
  if (!child) {
    throw new Error("the engine host wasn't forked");
  }
  return { host, child };
}

beforeEach(() => {
  resetFakeElectron();
});

describe("the main process's engine requests", () => {
  it("are trusted for the main process's own commands, and untrusted for agents", async () => {
    const { host, child } = started();
    const added = host.request(0, "add_project", { path: "/code/shop" });
    const summary = host.untrusted(0, "project_summary", { project: "p-aaaaaaaaaa" });
    expect(child.posted.slice(1)).toEqual([
      { type: "request", id: 1, windowId: 0, command: "add_project", args: { path: "/code/shop" }, trusted: true },
      { type: "request", id: 2, windowId: 0, command: "project_summary", args: { project: "p-aaaaaaaaaa" }, trusted: false },
    ]);
    child.emit("message", { type: "result", id: 2, ok: false, error: { code: "untrusted", message: "no" } });
    child.emit("message", { type: "result", id: 1, ok: true, value: { status: "added" } });
    expect(await added).toEqual({ status: "added" });
    await expect(summary).rejects.toEqual(new EngineRequestError({ code: "untrusted", message: "no" }));
  });

  it("hands the events the engine host forwards to whoever follows them", () => {
    const { host, child } = started();
    const heard: unknown[] = [];
    const stop = host.onEvent((event) => heard.push(event));
    const event = { type: "preferences_changed", preferences: { agents: { enabled: true, port: 29180 } } };
    child.emit("message", { type: "event", event });
    stop();
    child.emit("message", { type: "event", event });
    expect(heard).toEqual([event]);
  });
});
