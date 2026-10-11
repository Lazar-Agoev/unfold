/**
 * Agents (docs/ipc.md): turning the app's MCP server on in Preferences ›
 * Agents, and an MCP client that talks to it over HTTP the way Claude Code does (the official SDK's
 * client, named `claude-code`, so the app signs its edits as Claude). Raw requests check the
 * server's guards, which a client never trips.
 */

import { request } from "node:http";
import { createServer } from "node:net";
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { dialog, sidebar } from "./home";

/** A port nothing on this machine listens on now (the server takes it once Preferences are saved). */
export function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address !== null && typeof address === "object") {
          resolve(address.port);
        } else {
          reject(new Error("no port"));
        }
      });
    });
  });
}

/** Opens Preferences from the sidebar's gear and returns the dialog. */
export async function openPreferences(page: Page): Promise<Locator> {
  await sidebar(page).getByRole("button", { name: "Preferences…" }).click();
  const preferences = dialog(page, "Preferences");
  await expect(preferences).toBeVisible();
  return preferences;
}

/** The status of a POST to the server with `headers` added to a well-formed MCP request. */
export function rawStatus(address: string, headers: Readonly<Record<string, string>>): Promise<number> {
  const url = new URL(address);
  const body = JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list", params: {} });
  return new Promise((resolve, reject) => {
    const sent = request(
      {
        host: url.hostname,
        port: url.port,
        path: url.pathname,
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/json, text/event-stream", "content-length": Buffer.byteLength(body), ...headers },
      },
      (response) => {
        response.resume();
        resolve(response.statusCode ?? 0);
      },
    );
    sent.once("error", reject);
    sent.end(body);
  });
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

  async prompts(): Promise<string[]> {
    return (await this.client.listPrompts()).prompts.map((prompt) => prompt.name);
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
