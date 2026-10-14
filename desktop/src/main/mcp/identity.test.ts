import { describe, expect, it } from "vitest";
import { AgentDirectory, agentIdentity } from "./identity";

const call = (name: string, clientInfo?: { name: string }) => ({
  jsonrpc: "2.0",
  id: 1,
  method: name,
  params: clientInfo ? { _meta: { "io.modelcontextprotocol/clientInfo": clientInfo } } : {},
});
const initialize = (name: string) => ({ jsonrpc: "2.0", id: 0, method: "initialize", params: { clientInfo: { name, version: "1" } } });

describe("agentIdentity", () => {
  it("names Claude Code and Codex, and any other client by its own name", () => {
    expect(agentIdentity("claude-code")).toEqual({ author: "agent:claude", name: "Claude" });
    expect(agentIdentity("Claude Code")).toEqual({ author: "agent:claude", name: "Claude" });
    expect(agentIdentity("codex-mcp-client")).toEqual({ author: "agent:codex", name: "Codex" });
    expect(agentIdentity("Codex")).toEqual({ author: "agent:codex", name: "Codex" });
    expect(agentIdentity("Zed")).toEqual({ author: "agent:zed", name: "Zed" });
    expect(agentIdentity("My Agent/2 (beta)")).toEqual({ author: "agent:myagent2beta", name: "Myagent2beta" });
    expect(agentIdentity("x".repeat(40)).author).toBe(`agent:${"x".repeat(32)}`);
  });

  it("never signs as a person, even with no usable name", () => {
    expect(agentIdentity(undefined)).toEqual({ author: "agent:unknown", name: "Unknown" });
    expect(agentIdentity(" ✨ ")).toEqual({ author: "agent:unknown", name: "Unknown" });
    expect(agentIdentity("person:lazar").author).toBe("agent:personlazar");
  });
});

describe("AgentDirectory", () => {
  it("knows each request's clientInfo, and remembers initialize's by User-Agent", () => {
    const directory = new AgentDirectory();
    expect(directory.identify("claude-code", null).author).toBe("agent:claude");
    directory.observe(initialize("codex-mcp-client"), "node");
    expect(directory.identify(undefined, "node").author).toBe("agent:codex");
    // Without initialize, the User-Agent's product token.
    expect(directory.identify(undefined, "claude-code/2.0.31 (cli)").author).toBe("agent:claude");
    expect(directory.identify(undefined, null).author).toBe("agent:unknown");
  });

  it("lists the agents seen, the one seen last first, with when each last acted", () => {
    let changes = 0;
    const directory = new AgentDirectory(() => {
      changes += 1;
    });
    directory.observe(call("tools/list", { name: "claude-code" }), null, 10);
    directory.observe([call("tools/call", { name: "codex" }), call("ping", { name: "codex" })], null, 20);
    directory.observe(call("tools/call", { name: "claude-code" }), null, 30);
    directory.observe(call("tools/list", { name: "claude-code" }), null, 40);
    directory.observe("not a message", null, 50);
    expect(directory.agents()).toEqual([
      { author: "agent:claude", name: "Claude", lastSeenMs: 40, lastActedMs: 30 },
      { author: "agent:codex", name: "Codex", lastSeenMs: 20, lastActedMs: 20 },
    ]);
    expect(changes).toBe(5);
  });

  it("remembers a bounded number of clients", () => {
    const directory = new AgentDirectory();
    for (let index = 0; index < 40; index += 1) {
      directory.observe(initialize(`client${index}`), `ua${index}`, index);
    }
    expect(directory.agents()).toHaveLength(32);
    expect(directory.agents()[0]?.author).toBe("agent:client39");
    expect(directory.identify(undefined, "ua0").author).toBe("agent:ua0");
    expect(directory.identify(undefined, "ua39").author).toBe("agent:client39");
  });
});
