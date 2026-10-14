/**
 * Agents (docs/ipc.md): the app's MCP server, which listens on a fixed local port while the app runs,
 * Settings (where an agent's command is copied and its connection shown), and an MCP client that
 * talks to the server over HTTP the way Claude Code does (the official SDK's client, named
 * `claude-code`, so the app signs its edits as Claude).
 */

import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { dialog, sidebar } from "./home";

/** Where the app's MCP server listens while the app runs (one app runs at a time). */
export const AGENT_ADDRESS = "http://127.0.0.1:29180/mcp";

/** Opens Settings from Home's sidebar gear and returns the dialog. */
export async function openSettings(page: Page): Promise<Locator> {
  await sidebar(page).getByRole("button", { name: "Settings", exact: true }).click();
  const settings = dialog(page, "Settings");
  await expect(settings).toBeVisible();
  return settings;
}

/** What a tool answered: its text blocks joined, its images, and whether it is an error. */
export interface ToolAnswer {
  readonly text: string;
  readonly images: readonly { readonly data: string; readonly mimeType: string }[];
  readonly isError: boolean;
}

/** An MCP client connected to the app's server. */
export class AgentClient {
  private readonly client: Client;

  private constructor(client: Client) {
    this.client = client;
  }

  /** Connects as `name` (Claude Code's client is `claude-code`). */
  static async connect(address: string, name = "claude-code"): Promise<AgentClient> {
    const client = new Client({ name, version: "2.0.0" });
    await client.connect(new StreamableHTTPClientTransport(new URL(address)));
    return new AgentClient(client);
  }

  async tools(): Promise<string[]> {
    return (await this.client.listTools()).tools.map((tool) => tool.name);
  }

  async call(tool: string, args: Readonly<Record<string, unknown>> = {}): Promise<ToolAnswer> {
    const result = await this.client.callTool({ name: tool, arguments: { ...args } });
    const text: string[] = [];
    const images: { data: string; mimeType: string }[] = [];
    for (const block of result.content) {
      if (block.type === "text") {
        text.push(block.text);
      } else if (block.type === "image") {
        images.push({ data: block.data, mimeType: block.mimeType });
      }
    }
    return { text: text.join("\n"), images, isError: result.isError === true };
  }

  /** Calls a tool that must succeed and returns its text. */
  async ok(tool: string, args: Readonly<Record<string, unknown>> = {}): Promise<string> {
    const answer = await this.call(tool, args);
    expect(answer.isError, `${tool} answered: ${answer.text}`).toBe(false);
    return answer.text;
  }

  /** Calls a tool that must be refused and returns the refusal's text. */
  async refused(tool: string, args: Readonly<Record<string, unknown>> = {}): Promise<string> {
    const answer = await this.call(tool, args);
    expect(answer.isError, `${tool} answered: ${answer.text}`).toBe(true);
    return answer.text;
  }

  close(): Promise<void> {
    return this.client.close();
  }
}

/** The page revision in a result's header (`· rev 1a2b3c4d ·`). */
export function revisionOf(text: string): string {
  const found = /· rev ([0-9a-f]{8})\b/.exec(text);
  if (!found?.[1]) {
    throw new Error(`no revision in: ${text}`);
  }
  return found[1];
}
