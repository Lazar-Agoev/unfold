/**
 * Who an agent is (D5, design §5.4). Every MCP request names its client: on the current revision
 * (2026-07-28) each request carries `clientInfo` in its `_meta`; a client on an earlier revision
 * names itself once, in `initialize`, and its later requests carry nothing but their HTTP headers.
 * So the directory remembers, per `User-Agent`, the name each `initialize` gave, and falls back to the
 * `User-Agent`'s product token. Claude Code signs as `agent:claude`, Codex as `agent:codex`, and any
 * other client by its own name, lowercased.
 *
 * The directory also keeps the agents seen since the server started, for Preferences › Agents.
 */

import type { AgentIdentity, ConnectedAgent } from "../../shared/agent-api";

/** The `_meta` key that carries `clientInfo` on every request of the current revision. */
const CLIENT_INFO_META_KEY = "io.modelcontextprotocol/clientInfo";
/** Who signs for a client that gives no usable name. */
const UNKNOWN = "unknown";
/** The longest name an author takes (D5). */
const MAX_NAME = 32;
/** The most `User-Agent`s, and agents, remembered: a handful of clients run on one machine. */
const MAX_REMEMBERED = 32;

/** Clients with names of their own (D5). */
const KNOWN: readonly { readonly match: RegExp; readonly slug: string }[] = [
  { match: /^claude[-_ ]?code/, slug: "claude" },
  { match: /^codex/, slug: "codex" },
];

/** Who signs for a client that names itself `clientName` (or nothing). */
export function agentIdentity(clientName: string | undefined): AgentIdentity {
  const lowered = (clientName ?? "").trim().toLowerCase();
  const own = lowered.replace(/[^a-z0-9._-]/g, "").slice(0, MAX_NAME);
  const slug = KNOWN.find((known) => known.match.test(lowered))?.slug ?? (own.length > 0 ? own : UNKNOWN);
  // Shown as the app shows any author: the name after `agent:`, capitalized.
  return { author: `agent:${slug}`, name: slug.charAt(0).toUpperCase() + slug.slice(1) };
}

/** `claude-code/2.0.31 (cli)` → `claude-code`. */
function productToken(userAgent: string): string | undefined {
  const [token = ""] = userAgent.split(/[/\s]/, 1);
  return token.length > 0 ? token : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** A `clientInfo`'s `name`, when it is shaped like one. */
function clientName(info: unknown): string | undefined {
  return isRecord(info) && typeof info["name"] === "string" ? info["name"] : undefined;
}

/** The client's name in a request's `_meta` envelope (the current revision), if it carries one. */
export function envelopeClientName(envelope: unknown): string | undefined {
  return isRecord(envelope) ? clientName(envelope[CLIENT_INFO_META_KEY]) : undefined;
}

/** Sets `key` last in a map that keeps at most {@link MAX_REMEMBERED} entries, dropping the oldest. */
function remember<V>(map: Map<string, V>, key: string, value: V): void {
  map.delete(key);
  map.set(key, value);
  for (const oldest of map.keys()) {
    if (map.size <= MAX_REMEMBERED) {
      break;
    }
    map.delete(oldest);
  }
}

/** The agents that talk to the MCP server: who each request is from, and who has connected. */
export class AgentDirectory {
  /** The client name each `User-Agent` gave in its last `initialize`. */
  private readonly names = new Map<string, string>();
  /** The agents seen, by author, the one seen last last. */
  private readonly seen = new Map<string, ConnectedAgent>();

  /** `onChange` hears every request observed. */
  constructor(private readonly onChange: () => void = () => undefined) {}

  /** Who sent a request: its `clientInfo` name if it has one, else what its `User-Agent` said before. */
  identify(name: string | undefined, userAgent: string | null): AgentIdentity {
    const learned = userAgent === null ? undefined : (this.names.get(userAgent) ?? productToken(userAgent));
    return agentIdentity(name ?? learned);
  }

  /**
   * Notes one HTTP request's JSON-RPC body (a message or a batch): learns the name an `initialize`
   * gives, and records that its agent was seen, and acted when it called a tool.
   */
  observe(body: unknown, userAgent: string | null, now = Date.now()): void {
    for (const message of Array.isArray(body) ? body : [body]) {
      if (!isRecord(message) || typeof message["method"] !== "string") {
        continue;
      }
      const params = isRecord(message["params"]) ? message["params"] : {};
      const initialized = message["method"] === "initialize" ? clientName(params["clientInfo"]) : undefined;
      if (initialized !== undefined && userAgent !== null) {
        remember(this.names, userAgent, initialized);
      }
      const agent = this.identify(envelopeClientName(params["_meta"]) ?? initialized, userAgent);
      const before = this.seen.get(agent.author);
      const acted = message["method"] === "tools/call" ? now : (before?.lastActedMs ?? null);
      remember(this.seen, agent.author, { author: agent.author, name: agent.name, lastSeenMs: now, lastActedMs: acted });
    }
    this.onChange();
  }

  /** The agents seen since the server started (or was last cleared), the one seen last first. */
  agents(): ConnectedAgent[] {
    return [...this.seen.values()].reverse();
  }

  /** Forgets everyone: the server stopped (which it reports itself). */
  clear(): void {
    this.names.clear();
    this.seen.clear();
  }
}
