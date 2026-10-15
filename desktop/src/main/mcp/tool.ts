/**
 * How a tool is written: its name, title, description (where it teaches,
 * at most 2,048 characters, which Claude Code keeps), its arguments as a zod schema, its annotations,
 * whether Claude Code loads it with the session's first tools, and what it does with a
 * {@link ToolContext}. `defineTool` turns that into a {@link Tool} the server registers; the server
 * supplies the call wrapper (who is calling, errors, the size guard).
 *
 * Every object in a tool's arguments is strict (`z.strictObject`): an argument a tool doesn't take,
 * such as `name` for `names`, is refused, never dropped. The call checks the arguments itself, inside
 * the wrapper, so a refusal of them reads, and stays under the size limit, like any other.
 */

import type { McpServer, ServerContext, StandardSchemaWithJSON } from "@modelcontextprotocol/server";
import { z } from "zod";
import type { ToolContext } from "./context";
import { clip } from "./format";
import { cant, type ToolResult } from "./results";

/** Claude Code loads a tool marked with this at once, instead of finding it with tool search. */
const ALWAYS_LOAD = "anthropic/alwaysLoad";

/** The most problems an argument refusal lists, and the most unknown names one problem quotes. */
const MAX_LISTED = 5;

/** Tool annotations: read tools are read-only; creating by id is idempotent. */
interface ToolHints {
  readonly readOnlyHint: boolean;
  readonly destructiveHint?: boolean;
  readonly idempotentHint?: boolean;
}

/** A tool's arguments: a strict object, as every object in them is. */
type ToolInput = z.ZodObject<z.core.$ZodShape, z.core.$strict>;

/** One tool, as written. */
export interface ToolSpec<S extends ToolInput> {
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

/**
 * The schema the SDK lists for a tool: its arguments' JSON Schema, which agents read, and no check of
 * its own, since the call checks them ({@link parsedArgs}).
 */
function listedSchema(input: ToolInput): StandardSchemaWithJSON {
  return { "~standard": { version: 1, vendor: "unfold", validate: (value) => ({ value }), jsonSchema: input["~standard"].jsonSchema } };
}

/** `“name”`, `“name”, “other”`, and how many more past {@link MAX_LISTED}. */
function quotedNames(names: readonly string[]): string {
  const quoted = names.slice(0, MAX_LISTED).map((name) => `“${clip(name, 60)}”`);
  return names.length > MAX_LISTED ? `${quoted.join(", ")} and ${names.length - MAX_LISTED} more` : quoted.join(", ");
}

/** Words for arguments an object doesn't take: which ones, and the ones it does. */
const unknownArguments: z.core.$ZodErrorMap = (issue) => {
  if (issue.code !== "unrecognized_keys") {
    return undefined;
  }
  const taken = issue.inst instanceof z.ZodObject ? Object.keys(issue.inst.shape) : [];
  const unknown = `unknown argument${issue.keys.length === 1 ? "" : "s"} ${quotedNames(issue.keys)}`;
  return `${unknown}; ${taken.length > 0 ? `the arguments are ${taken.join(", ")}` : "it takes none"}`;
};

/** One problem with the arguments: where it is (`combine.0`), and what it is. */
function problemText(issue: z.core.$ZodIssue): string {
  const at = issue.path.map((part) => clip(String(part), 60)).join(".");
  return at === "" ? issue.message : `${at}: ${issue.message}`;
}

/** `args` as `spec.input` takes them, or a refusal that says what is wrong with them. */
function parsedArgs<S extends ToolInput>(spec: ToolSpec<S>, args: unknown): z.output<S> {
  const parsed = spec.input.safeParse(args, { error: unknownArguments });
  if (parsed.success) {
    return parsed.data;
  }
  const { issues } = parsed.error;
  const problems = issues.slice(0, MAX_LISTED).map(problemText);
  const more = issues.length > MAX_LISTED ? `; and ${issues.length - MAX_LISTED} more` : "";
  throw cant(`call ${spec.name}`, `${problems.join("; ")}${more}`, "Fix the arguments and call it again.");
}

/** Makes a tool from its spec. */
export function defineTool<S extends ToolInput>(spec: ToolSpec<S>): Tool {
  const tool: Tool = {
    name: spec.name,
    title: spec.title,
    description: spec.description,
    annotations: spec.annotations,
    alwaysLoad: spec.alwaysLoad ?? false,
    register(server, call) {
      server.registerTool(
        spec.name,
        {
          title: spec.title,
          description: spec.description,
          inputSchema: listedSchema(spec.input),
          annotations: spec.annotations,
          ...(spec.alwaysLoad === true ? { _meta: { [ALWAYS_LOAD]: true } } : {}),
        },
        (args, request) => call(tool, (context) => spec.run(parsedArgs(spec, args), context), request),
      );
    },
  };
  return tool;
}
