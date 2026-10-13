/**
 * Settings › Agents: the commands that add Unfold's MCP server to Claude Code and Codex, each with a
 * copy button, and a line saying whether an agent has connected, or why none can (another program holds
 * the port, or the server failed). The server's state comes from the main process
 * (`agent_server_status`, then `agent_server_changed`).
 */

import type { AgentServerStatus } from "@unfold/desktop/agent-api";
import { useEffect, useState } from "react";
import { IconButton, showToast } from "../components";
import { onShellEvent, toUnfoldError, windowCommand } from "../ipc";

/** The address agents reach the server at, until the main process says. */
const DEFAULT_ADDRESS = "http://127.0.0.1:29180/mcp";

/** The agents with a command to copy, and the identity each signs as. */
const AGENTS = [
  { label: "Claude Code", author: "agent:claude", command: (address: string) => `claude mcp add --transport http --scope user unfold ${address}` },
  { label: "Codex", author: "agent:codex", command: (address: string) => `codex mcp add unfold --url ${address}` },
] as const;

/** The MCP server's state, as the main process reports it; null until it has. */
function useAgentServerStatus(): AgentServerStatus | null {
  const [status, setStatus] = useState<AgentServerStatus | null>(null);
  useEffect(() => {
    let heard = false;
    const stop = onShellEvent("agent_server_changed", (event) => {
      heard = true;
      setStatus(event.status);
    });
    windowCommand("agent_server_status").then(
      (current) => {
        if (!heard) {
          setStatus(current);
        }
      },
      () => undefined,
    );
    return () => {
      heard = true;
      stop();
    };
  }, []);
  return status;
}

async function copy(text: string, what: string): Promise<void> {
  try {
    await windowCommand("copy_to_clipboard", { text });
    showToast({ title: `Copied the ${what} command` });
  } catch (error) {
    showToast({ title: `Couldn't copy the ${what} command`, body: toUnfoldError(error).message, tone: "danger" });
  }
}

/** The port in an address (`http://127.0.0.1:29180/mcp` → `29180`). */
function portOf(address: string): string {
  return new URL(address).port;
}

/** What the status line says, and whether it has the green dot; null while there is nothing to say. */
function statusLine(status: AgentServerStatus | null): { readonly text: string; readonly connected: boolean } | null {
  switch (status?.state) {
    case "listening": {
      if (status.agents.length === 0) {
        return null;
      }
      const names = status.agents.map((agent) => AGENTS.find((known) => known.author === agent.author)?.label ?? agent.name);
      return { text: `${names.join(", ")} connected`, connected: true };
    }
    case "port_taken":
      return { text: `Another program holds port ${portOf(status.address)}, so agents can't reach Unfold.`, connected: false };
    case "failed":
      return { text: status.message ?? "The agent server couldn't start.", connected: false };
    case "starting":
    case undefined:
      return null;
  }
}

export function AgentsSection() {
  const status = useAgentServerStatus();
  const address = status?.address ?? DEFAULT_ADDRESS;
  const line = statusLine(status);
  return (
    <section className="app-settings-section" aria-labelledby="app-settings-agents">
      <h3 id="app-settings-agents" className="app-settings-caps">
        Agents
      </h3>
      <p className="app-settings-note">Run one in a terminal, once. Agents reach Unfold while it's open.</p>
      {AGENTS.map((agent) => {
        const command = agent.command(address);
        return (
          <div key={agent.label} className="app-settings-command">
            <span>{agent.label}</span>
            <code title={command}>{command}</code>
            <IconButton
              icon="copy"
              label={`Copy the ${agent.label} command`}
              size={24}
              onClick={() => {
                void copy(command, agent.label);
              }}
            />
          </div>
        );
      })}
      {line ? (
        <p className="app-settings-status" role="status">
          {line.connected ? <span className="app-settings-dot" aria-hidden="true" /> : null}
          {line.text}
        </p>
      ) : null}
    </section>
  );
}
