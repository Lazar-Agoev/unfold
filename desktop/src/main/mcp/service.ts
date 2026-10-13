/**
 * The agents' MCP server as the app runs it (design §5.2, §5.7): it listens on `127.0.0.1:29180`
 * whenever the app is open. Settings reads its status (`agent_server_status`), and the window hears
 * when it changes (`agent_server_changed`): at once when it starts listening or can't, and at most
 * once a second while agents act.
 */

import type { AgentServerStatus } from "../../shared/agent-api";
import type { McpServices } from "./context";
import { AgentDirectory } from "./identity";
import { McpListener } from "./listener";
import type { Prompt } from "./prompt";
import { mcpHandler } from "./server";
import type { Tool } from "./tool";

/** The port agents connect to. It is fixed, so the commands people paste stay the same. */
const AGENT_PORT = 29180;
/** How often, at most, agents' activity is reported to the window. */
const ACTIVITY_INTERVAL_MS = 1_000;

export interface McpServiceOptions {
  readonly services: McpServices;
  /** The app's version, which the server reports. */
  readonly version: string;
  /** Hears the status whenever it changes. */
  readonly onStatus: (status: AgentServerStatus) => void;
  /** The tools and prompts, when not the app's own (tests). */
  readonly tools?: readonly Tool[];
  readonly prompts?: readonly Prompt[];
  /** The port, when not the agents' port (tests: 0 picks a free one). */
  readonly port?: number;
}

export class McpService {
  private readonly directory: AgentDirectory;
  private readonly listener: McpListener;
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
      port: options.port ?? AGENT_PORT,
      serve: (request, body) => {
        this.directory.observe(body, request.headers.get("user-agent"));
        return serve(request, body);
      },
      onState: () => {
        this.notify();
      },
    });
  }

  /** Starts listening; when it can't, the status says why. Never rejects. */
  start(): Promise<void> {
    return this.listener.start();
  }

  /** What Settings shows. */
  status(): AgentServerStatus {
    const state = this.listener.state;
    return {
      state: state.state,
      address: `http://127.0.0.1:${state.port}/mcp`,
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
