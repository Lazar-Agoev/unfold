/**
 * Preferences › Agents (design §5.2): the switch that lets agents connect to the app's local MCP
 * server, its port, the address and the commands that install it in Claude Code and Codex, and the
 * agents that connected, with when each last acted. The switch and the port save with the dialog;
 * the server's state comes from the main process (`agent_server_status`, then `agent_server_changed`).
 */

import type { AgentServerStatus, ConnectedAgent } from "@unfold/desktop/agent-api";
import { useEffect, useState } from "react";
import { IconButton, Switch, TextField, showToast } from "../components";
import { onShellEvent, toUnfoldError, windowCommand, type AgentPreferences } from "../ipc";
import { relativeTime } from "../lib/format";

/** The lowest port Preferences accepts; ports below are the system's. */
const MIN_PORT = 1024;
const MAX_PORT = 65_535;

/** Parses the port field: a whole number from 1024 to 65535. */
export function parseAgentPort(text: string): { ok: true; value: number } | { ok: false; message: string } {
  const value = Number(text.trim());
  if (text.trim() === "" || !Number.isInteger(value) || value < MIN_PORT || value > MAX_PORT) {
    return { ok: false, message: `A whole number from ${MIN_PORT} to ${MAX_PORT}.` };
  }
  return { ok: true, value };
}

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
    showToast({ title: `Copied ${what}` });
  } catch (error) {
    showToast({ title: `Couldn't copy ${what}`, body: toUnfoldError(error).message, tone: "danger" });
  }
}

/** A command or address to copy: its label, the text in code type, and a Copy button. */
function CopyRow({ label, text, what }: { readonly label: string; readonly text: string; readonly what: string }) {
  return (
    <div className="preferences-command">
      <span className="preferences-command-label">{label}</span>
      <code className="preferences-command-text">{text}</code>
      <IconButton
        icon="copy"
        label={`Copy ${what}`}
        size={24}
        onClick={() => {
          void copy(text, what);
        }}
      />
    </div>
  );
}

function agentLine(agent: ConnectedAgent): string {
  return agent.lastActedMs === null ? `${agent.name} · connected, hasn't acted yet` : `${agent.name} · last acted ${relativeTime(agent.lastActedMs)}`;
}

export interface AgentsSectionProps {
  /** The switch, as the form has it. */
  readonly enabled: boolean;
  /** The port field's text. */
  readonly port: string;
  /** Preferences › Agents as saved. */
  readonly saved: AgentPreferences;
  readonly onEnabledChange: (enabled: boolean) => void;
  readonly onPortChange: (port: string) => void;
}

export function AgentsSection({ enabled, port, saved, onEnabledChange, onPortChange }: AgentsSectionProps) {
  const status = useAgentServerStatus();
  const parsed = parseAgentPort(port);
  const shownPort = parsed.ok ? parsed.value : saved.port;
  const address = `http://127.0.0.1:${shownPort}/mcp`;
  // The server's state describes the form only once the form is saved.
  const savedForm = saved.enabled && saved.port === shownPort;
  const current = savedForm && status?.port === shownPort ? status : null;
  const portError = !parsed.ok
    ? parsed.message
    : current?.state === "port_taken"
      ? `Another program uses port ${shownPort}. Choose another port, then save.`
      : current?.state === "failed"
        ? `Agents can't connect: ${current.message ?? "the server couldn't start"}.`
        : null;
  const hint = !savedForm
    ? "Agents can connect once you save."
    : current?.state === "listening"
      ? "Listening. Run your agent's command below once; it then connects whenever Unfold is open."
      : current?.state === "starting"
        ? "Starting…"
        : undefined;
  const agents = current?.state === "listening" ? current.agents : [];

  return (
    <section className="preferences-section" aria-labelledby="preferences-agents">
      <h3 id="preferences-agents" className="preferences-caps">
        Agents
      </h3>
      <div className="preferences-row">
        <span className="preferences-text">
          <span className="preferences-label">Let agents connect</span>
          <span className="preferences-note">
            Claude Code, Codex, and other agents on this computer can read your projects and build boards while Unfold is open.
          </span>
        </span>
        <Switch checked={enabled} label="Let agents connect" onChange={onEnabledChange} />
      </div>
      {enabled ? (
        <>
          <TextField
            label="Port"
            inputMode="numeric"
            className="preferences-port"
            value={port}
            error={portError}
            {...(portError === null && hint !== undefined ? { hint } : {})}
            onChange={(event) => {
              onPortChange(event.target.value);
            }}
          />
          <div className="preferences-commands" role="group" aria-label="Connect an agent">
            <CopyRow label="Address" text={address} what="the address" />
            <CopyRow label="Claude Code" text={`claude mcp add --transport http --scope user unfold ${address}`} what="the Claude Code command" />
            <CopyRow label="Codex" text={`codex mcp add unfold --url ${address}`} what="the Codex command" />
          </div>
          <div className="preferences-agents" role="group" aria-label="Connected agents">
            <span className="preferences-label">Connected agents</span>
            {agents.length > 0 ? (
              <ul className="preferences-agents-list">
                {agents.map((agent) => (
                  <li key={agent.author}>{agentLine(agent)}</li>
                ))}
              </ul>
            ) : (
              <span className="preferences-note">None yet. An agent whose session started before Unfold reconnects from its MCP menu (/mcp in Claude Code).</span>
            )}
          </div>
        </>
      ) : null}
    </section>
  );
}
