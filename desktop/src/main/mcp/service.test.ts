import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentServerStatus } from "../../shared/agent-api";
import { AgentBridge } from "../agents/bridge";
import { ViewTracker } from "../agents/views";
import { agentSettings, McpService } from "./service";
import { NO_RENDERER } from "./tools/testing";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const services = {
  bridge: new AgentBridge({ windows: { get: () => undefined, all: () => [], create: () => { throw new Error("no windows"); }, deliver: () => undefined, focus: () => undefined }, views: new ViewTracker() }),
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

function setup() {
  const statuses: AgentServerStatus[] = [];
  const service = new McpService({ services, version: "1.0.0", onStatus: (status) => statuses.push(status) });
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

describe("agentSettings", () => {
  it("reads Preferences › Agents, and is off when it can't", () => {
    expect(agentSettings({ agents: { enabled: true, port: 31000 } })).toEqual({ enabled: true, port: 31000 });
    expect(agentSettings({ agents: { enabled: "yes", port: 70_000 } })).toEqual({ enabled: false, port: 29180 });
    expect(agentSettings({ theme: "dark" })).toEqual({ enabled: false, port: 29180 });
    expect(agentSettings(null)).toEqual({ enabled: false, port: 29180 });
  });
});

describe("McpService", () => {
  it("is off until Preferences › Agents turns it on, and reports each change", async () => {
    const { service, statuses } = setup();
    expect(service.status()).toEqual({ state: "off", port: 29180, address: "http://127.0.0.1:29180/mcp", agents: [] });
    await service.configure({ enabled: false, port: 29180 });
    expect(statuses).toEqual([]);

    await service.configure({ enabled: true, port: 0 });
    const port = service.status().port;
    expect(statuses.map((status) => status.state)).toEqual(["starting", "listening"]);
    expect(service.status()).toEqual({ state: "listening", port, address: `http://127.0.0.1:${port}/mcp`, agents: [] });

    await service.configure({ enabled: false, port: 0 });
    expect(statuses.at(-1)).toMatchObject({ state: "off" });
  });

  it("says the port is taken", async () => {
    const blocker = createServer();
    blockers.push(blocker);
    await new Promise<void>((resolve) => blocker.listen(0, "127.0.0.1", resolve));
    const port = (blocker.address() as AddressInfo).port;
    const { service } = setup();
    await service.configure({ enabled: true, port });
    expect(service.status()).toEqual({ state: "port_taken", port, address: `http://127.0.0.1:${port}/mcp`, agents: [] });
  });

  it("lists the agents that connect, reports their activity at most once a second, and forgets them when turned off", async () => {
    const { service, statuses } = setup();
    await service.configure({ enabled: true, port: 0 });
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

    await service.configure({ enabled: false, port: listening.port });
    expect(service.status().agents).toEqual([]);
    expect(statuses.at(-1)).toMatchObject({ state: "off", agents: [] });
  });
});
