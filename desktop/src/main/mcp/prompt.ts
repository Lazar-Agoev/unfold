/**
 * Prompts (design §6.9): recipes the person starts from their agent (Claude Code lists them as
 * `/unfold:<name>`). Each turns its arguments into the request the agent receives as the person's
 * message: read the matching guide topic, post a diagram brief, and build.
 */

import type { McpServer } from "@modelcontextprotocol/server";
import type { z } from "zod";

/** One prompt, as written. MCP passes prompt arguments as strings, so `args` has string fields. */
export interface PromptSpec<S extends z.ZodObject> {
  readonly name: string;
  readonly title: string;
  readonly description: string;
  readonly args: S;
  /** The request, as the person's message. */
  readonly text: (args: z.output<S>) => string;
}

/** A prompt ready to register. */
export interface Prompt {
  readonly name: string;
  register(server: McpServer): void;
}

/** Makes a prompt from its spec. */
export function definePrompt<S extends z.ZodObject>(spec: PromptSpec<S>): Prompt {
  return {
    name: spec.name,
    register(server) {
      const args: z.ZodObject = spec.args;
      server.registerPrompt(spec.name, { title: spec.title, description: spec.description, argsSchema: args }, (values) => ({
        // The SDK has checked `values` against `spec.args`, so they are its output.
        messages: [{ role: "user", content: { type: "text", text: spec.text(values as z.output<S>) } }],
      }));
    },
  };
}
