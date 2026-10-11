/**
 * The agents' MCP server as the app runs it (design §5.2, §5.7): it listens while Preferences › Agents
 * has it on, on the port it names, and moves or stops when that changes. Preferences › Agents reads
 * its status (`agent_server_status`), and every window hears when it changes (`agent_server_changed`):
 * at once when it starts, stops, or fails, and at most once a second while agents act.
 */

import type { AgentServerStatus } from "../../shared/agent-api";
import type { McpServices } from "./context";
import { AgentDirectory } from "./identity";
import { McpListener, type ListenerSettings } from "./listener";
import type { Prompt } from "./prompt";
import { mcpHandler } from "./server";
import type { Tool } from "./tool";

/** The port agents connect to unless Preferences › Agents names another (D7). */
const DEFAULT_PORT = 29180;
/** How often, at most, agents' activity is reported to the windows. */
const ACTIVITY_INTERVAL_MS = 1_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Preferences › Agents, from the preferences (`get_preferences`, `preferences_changed`); off when unreadable. */
export function agentSettings(preferences: unknown): ListenerSettings {
  const agents = isRecord(preferences) ? preferences["agents"] : undefined;
  const enabled = isRecord(agents) && agents["enabled"] === true;
  const port = isRecord(agents) ? agents["port"] : undefined;
  return {
    enabled,
    port: typeof port === "number" && Number.isInteger(port) && port > 0 && port <= 65_535 ? port : DEFAULT_PORT,
  };
}

export interface McpServiceOptions {
  readonly services: McpServices;
  /** The app's version, which the server reports. */
  readonly version: string;
  /** Hears the status whenever it changes. */
  readonly onStatus: (status: AgentServerStatus) => void;
  /** The tools and prompts, when not the app's own (tests). */
  readonly tools?: readonly Tool[];
  readonly prompts?: readonly Prompt[];
}

export class McpService {
  private readonly directory: AgentDirectory;
  private readonly listener: McpListener;
  private port = DEFAULT_PORT;
  private activity: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly options: McpServiceOptions) {
    this.directory = new AgentDirectory(() => {
      this.activity ??= setTimeout(() => {
        this.notify();
      }, ACTIVITY_INTERVAL_MS);
    });
    const serve = mcpHandler({
      services: options.services,
      directory: this.directory,
      version: options.version,
      ...(options.tools ? { tools: options.tools } : {}),
      ...(options.prompts ? { prompts: options.prompts } : {}),
    });
    this.listener = new McpListener({
      serve: (request, body) => {
        this.directory.observe(body, request.headers.get("user-agent"));
        return serve(request, body);
      },
      onState: (state) => {
        if (state.state !== "listening") {
          // Stopped, moving, or failed: whoever was connected is disconnected.
          this.directory.clear();
        }
        this.notify();
      },
    });
  }

  /** Follows Preferences › Agents: listens on `port` while `enabled`, else stops. */
  configure(settings: ListenerSettings): Promise<void> {
    this.port = settings.port;
    return this.listener.configure(settings);
  }

  /** What Preferences › Agents shows. */
  status(): AgentServerStatus {
    const state = this.listener.state;
    const port = state.state === "off" ? this.port : state.port;
    return {
      state: state.state,
      port,
      address: `http://127.0.0.1:${port}/mcp`,
      ...(state.state === "failed" ? { message: state.message } : {}),
      agents: this.directory.agents(),
    };
  }

  /** Stops listening (the app is quitting). */
  close(): Promise<void> {
    return this.listener.close();
  }

  private notify(): void {
    if (this.activity !== null) {
      clearTimeout(this.activity);
      this.activity = null;
    }
    this.options.onStatus(this.status());
  }
}
