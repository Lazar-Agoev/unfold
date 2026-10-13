import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentServerStatus } from "../../shared/agent-api";
import { AgentBridge } from "../agents/bridge";
import { ViewTracker } from "../agents/views";
import { McpService } from "./service";
import { NO_RENDERER } from "./tools/testing";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const services = {
  bridge: new AgentBridge({ window: { get: () => undefined, current: () => undefined, deliver: () => undefined, focus: () => undefined }, openWindow: () => Promise.reject(new Error("no window")), views: new ViewTracker() }),
  renderer: NO_RENDERER,
  engine: () => Promise.resolve(null),
  engineAs: () => Promise.resolve(null),
  confirmAddProject: () => Promise.resolve(false),
  addProject: () => Promise.reject(new Error("not asked")),
};

const started: McpService[] = [];
const blockers: Server[] = [];

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all(started.splice(0).map((service) => service.close()));
  await Promise.all(blockers.splice(0).map((server) => new Promise((resolve) => server.close(resolve))));
});

function setup(port = 0) {
  const statuses: AgentServerStatus[] = [];
  const service = new McpService({ services, version: "1.0.0", onStatus: (status) => statuses.push(status), port });
  started.push(service);
  return { service, statuses };
}

/** Posts one JSON-RPC message, as a client on the current revision would. */
function post(status: AgentServerStatus, method: string, name: string): Promise<Response> {
  return fetch(status.address, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json, text/event-stream" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params: { _meta: { "io.modelcontextprotocol/clientInfo": { name, version: "1" } } } }),
  });
}

describe("McpService", () => {
  it("listens on the agents' port, 127.0.0.1:29180, unless told otherwise", () => {
    const service = new McpService({ services, version: "1.0.0", onStatus: () => undefined });
    expect(service.status()).toEqual({ state: "starting", address: "http://127.0.0.1:29180/mcp", agents: [] });
  });

  it("starts listening and reports it", async () => {
    const { service, statuses } = setup();
    await service.start();
    const address = service.status().address;
    expect(address).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/mcp$/);
    expect(address).not.toBe("http://127.0.0.1:0/mcp");
    expect(statuses).toEqual([{ state: "listening", address, agents: [] }]);
  });

  it("says another program holds the port", async () => {
    const blocker = createServer();
    blockers.push(blocker);
    await new Promise<void>((resolve) => blocker.listen(0, "127.0.0.1", resolve));
    const port = (blocker.address() as AddressInfo).port;
    const { service, statuses } = setup(port);
    await service.start();
    const taken = { state: "port_taken", address: `http://127.0.0.1:${port}/mcp`, agents: [] };
    expect(service.status()).toEqual(taken);
    expect(statuses).toEqual([taken]);
  });

  it("lists the agents that connect, and reports their activity at most once a second", async () => {
    const { service, statuses } = setup();
    await service.start();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const listening = service.status();
    const before = statuses.length;

    expect((await post(listening, "tools/list", "claude-code")).status).toBe(200);
    expect((await post(listening, "ping", "codex")).status).toBe(200);
    expect(service.status().agents.map((agent) => agent.name)).toEqual(["Codex", "Claude"]);
    expect(statuses).toHaveLength(before);
    vi.advanceTimersByTime(1_000);
    expect(statuses).toHaveLength(before + 1);
    expect(statuses.at(-1)?.agents.map((agent) => agent.name)).toEqual(["Codex", "Claude"]);
  });
});
