/**
 * The MCP server (design §5.3): Streamable HTTP, stateless. The SDK serves each request with a fresh
 * server from one factory, on the current revision (2026-07-28, `server/discover`, each request
 * carrying its client) and on earlier ones (the `initialize` handshake, served statelessly too). It
 * offers tools, prompts, and the instructions; no resources, sampling, roots, or logging.
 *
 * Every tool call goes through one wrapper: it works out who is calling, builds the call's context,
 * turns a `ToolError` into a refusal the agent reads, and keeps the result under about 6,000 tokens.
 */

import { createMcpHandler, McpServer } from "@modelcontextprotocol/server";
import { toolContext, type McpServices } from "./context";
import { envelopeClientName, type AgentDirectory } from "./identity";
import { INSTRUCTIONS } from "./instructions";
import { MAX_BODY_BYTES } from "./listener";
import type { Prompt } from "./prompt";
import { errorResult, fitResult, ToolError } from "./results";
import type { Tool, ToolCall } from "./tool";
import { PROMPTS, TOOLS } from "./tools";

export interface McpHandlerOptions {
  readonly services: McpServices;
  readonly directory: AgentDirectory;
  /** The app's version, which the server reports. */
  readonly version: string;
  readonly tools?: readonly Tool[];
  readonly prompts?: readonly Prompt[];
}

/** Serves one checked request (`body` already parsed, when it is JSON): the listener's `serve`. */
export type McpHandler = (request: Request, body: unknown) => Promise<Response>;

/** The MCP server's request handler. */
export function mcpHandler({ services, directory, version, tools = TOOLS, prompts = PROMPTS }: McpHandlerOptions): McpHandler {
  const handler = createMcpHandler(
    ({ requestInfo }) => {
      const server = new McpServer(
        { name: "unfold", title: "Unfold", version },
        { instructions: INSTRUCTIONS, capabilities: { tools: {}, prompts: {} } },
      );
      const userAgent = requestInfo?.headers.get("user-agent") ?? null;
      const call: ToolCall = async (tool, run, request) => {
        // On the current revision each request names its client; on earlier ones only `initialize`
        // does, so the directory remembers that by User-Agent.
        const agent = directory.identify(envelopeClientName(request.mcpReq.envelope), userAgent);
        try {
          return fitResult(await run(toolContext(services, agent, request.mcpReq.signal)));
        } catch (error) {
          if (error instanceof ToolError) {
            return errorResult(error);
          }
          console.error(`[mcp] ${tool.name} failed:`, error);
          return errorResult(new ToolError(`${tool.name} failed inside Unfold. Try again; if it fails again, tell the person.`));
        }
      };
      for (const tool of tools) {
        tool.register(server, call);
      }
      for (const prompt of prompts) {
        prompt.register(server);
      }
      return server;
    },
    {
      legacy: "stateless",
      maxRequestBodySize: MAX_BODY_BYTES,
      onerror: (error) => {
        console.warn("[mcp]", error.message);
      },
    },
  );
  return (request, body) => handler.fetch(request, body === undefined ? {} : { parsedBody: body });
}
