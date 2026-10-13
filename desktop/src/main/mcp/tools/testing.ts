/**
 * Test support for the tools: a fake engine that answers by command and records each request (for no
 * window, or as the window holding a board), one fake window that answers the agent channel's requests
 * as a test says, a fake render window, and a call that runs one tool as the server does (its
 * arguments checked by its schema, a refusal as an error result, the size guard). Engine answers
 * recorded from real commands live in `testdata/workspace/commands/`.
 */

import type { McpServer, ServerContext } from "@modelcontextprotocol/server";
import type { z } from "zod";
import type { AgentIdentity, AgentShellEvent, ViewReport } from "../../../shared/agent-api";
import type { AddedProject } from "../../../shared/unfold-api";
import { AgentBridge, type AgentWindows } from "../../agents/bridge";
import { ViewTracker } from "../../agents/views";
import { EngineRequestError } from "../../engine-host";
import type { WindowEntry } from "../../windows";
import { toolContext, type PageImages, type ToolContext } from "../context";
import type { ProjectRow, WorkspaceView } from "../engine-types";
import { errorResult, fitResult, ToolError, type ToolResult } from "../results";
import type { Tool } from "../tool";

/** The project the recorded answers name (`<project>`), as the fake workspace lists it. */
export const PROJECT = "<project>";

/** A project row of the fake workspace. */
export function projectRow(id: string, name: string, overrides: Partial<ProjectRow> = {}): ProjectRow {
  return { id, name, path: `/code/${name}`, expanded: true, live: true, state: { kind: "current" }, ...overrides };
}

/** A recorded answer, as a fresh copy of the type the command returns. */
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- a JSON import can't carry the command's result type
export function recorded<T>(answer: unknown): T {
  return structuredClone(answer) as T;
}

/** An engine refusal, as the engine host rejects with it. */
export function refusal(code: string, message: string): EngineRequestError {
  return new EngineRequestError({ code, message });
}

type Answer = (args: Readonly<Record<string, unknown>>) => unknown;

export interface HarnessOptions {
  /** The engine's answers by command (`workspace` defaults to {@link projects}); a thrown error rejects. */
  readonly engine?: Readonly<Record<string, Answer>>;
  /** The workspace's projects (default: ts-orders as {@link PROJECT}). */
  readonly projects?: readonly ProjectRow[];
  /** The window's answer to an agent request; undefined leaves it unanswered (it times out). */
  readonly window?: (request: AgentShellEvent) => unknown;
  /** The render window's answers (default: every request fails as `render_failed`). */
  readonly renderer?: Partial<PageImages>;
  /** Whether the person agrees to add a folder, and what adding it gives (an error: the engine refuses it). */
  readonly confirm?: boolean;
  readonly added?: AddedProject | EngineRequestError;
  readonly agent?: AgentIdentity;
}

/** What a tool call answered: its text blocks joined, and whether it is a refusal. */
export interface Called {
  readonly text: string;
  readonly isError: boolean;
  readonly result: ToolResult;
}

const KINDS: Readonly<Record<string, "edit" | "read" | "open" | "create" | "board" | "show" | "repair">> = {
  agent_edit: "edit",
  agent_read: "read",
  agent_open_board: "open",
  agent_create_board: "create",
  agent_edit_board: "board",
  agent_show: "show",
  agent_repair: "repair",
};

const NO_RENDER = { ok: false, code: "render_failed", message: "No fake render window answers in this test." } as const;

/** A render window that fails every request (`render_failed`), for tests that draw nothing. */
export const NO_RENDERER: PageImages = {
  screenshot: () => Promise.resolve(NO_RENDER),
  check: () => Promise.resolve(NO_RENDER),
  checkBoard: () => Promise.resolve(NO_RENDER),
};

/** A fake app for tools: engine, window, views, and the requests each received. */
export function harness(options: HarnessOptions = {}) {
  const projects = options.projects ?? [projectRow(PROJECT, "ts-orders")];
  const workspace: WorkspaceView = { projects: [...projects], selected: projects[0]?.id ?? null };
  const engineCalls: [string, Readonly<Record<string, unknown>>][] = [];
  /** Engine requests sent as a window: its id, the command, and the arguments. */
  const windowCalls: [number, string, Readonly<Record<string, unknown>>][] = [];
  const requests: AgentShellEvent[] = [];
  const focused: number[] = [];
  const confirmations: string[] = [];
  const views = new ViewTracker();
  const entry = { id: 1, loaded: true } as WindowEntry;
  const windows: AgentWindows = {
    get: (id) => (id === 1 ? entry : undefined),
    current: () => entry,
    open: () => entry,
    focus: () => {
      focused.push(1);
    },
    deliver: (_entry, event) => {
      const request = event as AgentShellEvent;
      requests.push(request);
      const kind = KINDS[request.type];
      const answer = options.window?.(request);
      if (kind !== undefined && answer !== undefined && "requestId" in request) {
        queueMicrotask(() => {
          bridge.settle(1, kind, request.requestId, answer);
        });
      }
    },
  };
  const bridge = new AgentBridge({ windows, views, timeoutMs: 500 });
  const answers: Readonly<Record<string, Answer>> = { workspace: () => workspace, ...options.engine };
  const agent = options.agent ?? { author: "agent:claude", name: "Claude" };

  const renderer: PageImages = { ...NO_RENDERER, ...options.renderer };

  function answer(command: string, commandArgs: Readonly<Record<string, unknown>>): Promise<unknown> {
    const answered = answers[command];
    if (!answered) {
      return Promise.reject(refusal("not_found", `no fake answer for ${command}`));
    }
    try {
      return Promise.resolve(answered(commandArgs));
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /** A tool call's context, as the server makes one per call. */
  function context(): ToolContext {
    return toolContext(
      {
        bridge,
        renderer,
        engine: (command, commandArgs) => {
          engineCalls.push([command, commandArgs]);
          return answer(command, commandArgs);
        },
        engineAs: (windowId, command, commandArgs) => {
          windowCalls.push([windowId, command, commandArgs]);
          return answer(command, commandArgs);
        },
        confirmAddProject: (_agent, path) => {
          confirmations.push(path);
          return Promise.resolve(options.confirm ?? false);
        },
        addProject: () =>
          options.added instanceof EngineRequestError ? Promise.reject(options.added) : Promise.resolve(options.added ?? { status: "added", project: PROJECT }),
      },
      agent,
      new AbortController().signal,
    );
  }

  async function call(tool: Tool, args: Readonly<Record<string, unknown>> = {}): Promise<Called> {
    let input: z.ZodType | undefined;
    let handler: ((args: unknown, request: ServerContext) => Promise<ToolResult>) | undefined;
    const server = {
      registerTool: (_name: string, config: { inputSchema: z.ZodType }, registered: typeof handler) => {
        input = config.inputSchema;
        handler = registered;
      },
    } as unknown as McpServer;
    tool.register(server, async (_tool, run) => {
      try {
        return fitResult(await run(context()));
      } catch (error) {
        if (error instanceof ToolError) {
          return errorResult(error);
        }
        throw error;
      }
    });
    const parsed = input?.safeParse(args);
    if (!handler || !parsed) {
      throw new Error(`${tool.name} registered nothing`);
    }
    if (!parsed.success) {
      return { text: parsed.error.issues.map((issue) => issue.message).join("\n"), isError: true, result: { content: [], isError: true } };
    }
    const result = await handler(parsed.data, {} as ServerContext);
    const text = result.content.map((block) => (block.type === "text" ? block.text : "")).join("\n");
    return { text, isError: result.isError === true, result };
  }

  return {
    call,
    context,
    views,
    bridge,
    engineCalls,
    windowCalls,
    requests,
    focused,
    confirmations,
    /** The engine commands called, by name, in order. */
    commands: () => engineCalls.map(([command]) => command),
    /** Window 1 reports a view. */
    report: (report: ViewReport) => {
      views.report(1, report);
    },
  };
}
