/**
 * How a tool is written (design §5.3, §6.1, §6.12): its name, title, description (where it teaches,
 * at most 2,048 characters, which Claude Code keeps), its arguments as a zod schema (the SDK checks
 * them before the tool runs), its annotations, whether Claude Code loads it with the session's first
 * tools, and what it does with a {@link ToolContext}. `defineTool` turns that into a {@link Tool} the
 * server registers; the server supplies the call wrapper (who is calling, errors, the size guard).
 */

import type { McpServer, ServerContext } from "@modelcontextprotocol/server";
import type { z } from "zod";
import type { ToolContext } from "./context";
import type { ToolResult } from "./results";

/** Claude Code loads a tool marked with this at once, instead of finding it with tool search. */
const ALWAYS_LOAD = "anthropic/alwaysLoad";

/** The annotations the design sets (§5.3): read tools are read-only; creating by id is idempotent. */
interface ToolHints {
  readonly readOnlyHint: boolean;
  readonly destructiveHint?: boolean;
  readonly idempotentHint?: boolean;
}

/** One tool, as written. */
export interface ToolSpec<S extends z.ZodObject> {
  readonly name: string;
  readonly title: string;
  /** When to use it, what it expects, and the rules it enforces; what matters first. */
  readonly description: string;
  readonly input: S;
  readonly annotations: ToolHints;
  /** Loaded when the session starts (`_meta["anthropic/alwaysLoad"]`): `get_guide` and `get_current_view`. */
  readonly alwaysLoad?: boolean;
  /** Does the work. Throw a `ToolError` to refuse; anything else thrown is an internal error. */
  readonly run: (args: z.output<S>, context: ToolContext) => Promise<ToolResult>;
}

/** Runs one call: the server's wrapper around a tool's `run` (identity, context, errors, size). */
export type ToolCall = (tool: Tool, run: (context: ToolContext) => Promise<ToolResult>, request: ServerContext) => Promise<ToolResult>;

/** A tool ready to register. */
export interface Tool {
  readonly name: string;
  readonly title: string;
  readonly description: string;
  readonly annotations: ToolHints;
  readonly alwaysLoad: boolean;
  /** Puts the tool on a server; each call goes through `call`. */
  register(server: McpServer, call: ToolCall): void;
}

/** Makes a tool from its spec. */
export function defineTool<S extends z.ZodObject>(spec: ToolSpec<S>): Tool {
  const tool: Tool = {
    name: spec.name,
    title: spec.title,
    description: spec.description,
    annotations: spec.annotations,
    alwaysLoad: spec.alwaysLoad ?? false,
    register(server, call) {
      const input: z.ZodObject = spec.input;
      server.registerTool(
        spec.name,
        {
          title: spec.title,
          description: spec.description,
          inputSchema: input,
          annotations: spec.annotations,
          ...(spec.alwaysLoad === true ? { _meta: { [ALWAYS_LOAD]: true } } : {}),
        },
        // The SDK has checked `args` against `spec.input`, so they are its output.
        (args, request) => call(tool, (context) => spec.run(args as z.output<S>, context), request),
      );
    },
  };
  return tool;
}
