/**
 * The MCP server's HTTP listener (design §5.3, §5.7): `127.0.0.1:<port>`, path `/mcp`, from launch
 * until the app quits. It serves this machine's programs and nothing else:
 *
 * - a request with an `Origin` header comes from a web page, and one whose `Host` isn't
 *   `127.0.0.1:<port>` or `localhost:<port>` could be a web page reaching it by DNS rebinding: both
 *   are refused with 403 and no body;
 * - a body over {@link MAX_BODY_BYTES} is refused with 413 before it is parsed;
 * - every other path is 404.
 *
 * What passes goes to `serve` (the MCP SDK's handler) as a web `Request`, with its JSON body parsed
 * once here, and the `Response` streams back. Closing it closes every connection.
 */

import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { Readable } from "node:stream";
import type { ReadableStream as WebReadableStream } from "node:stream/web";

/** The interface it listens on: this machine only. */
const HOST = "127.0.0.1";
/** The one path it serves. */
const PATH = "/mcp";
/** The most a request body may hold. Tool arguments are a page's worth of ids and words at most. */
export const MAX_BODY_BYTES = 1024 * 1024;

/** Whether it listens, and on which port (the one asked for, or the one it got when asked for 0). */
export type ListenerState =
  | { readonly state: "starting" | "listening" | "port_taken"; readonly port: number }
  | { readonly state: "failed"; readonly port: number; readonly message: string };

export interface McpListenerOptions {
  /** The port to listen on. There is no fallback: when another program holds it, the state says so. */
  readonly port: number;
  /** Serves one checked request; `body` is the parsed JSON body (undefined when absent or not JSON). */
  readonly serve: (request: Request, body: unknown) => Promise<Response>;
  /** Hears every change of state. */
  readonly onState: (state: ListenerState) => void;
}

/** Whether `host` (a `Host` header) names this listener: `127.0.0.1` or `localhost` at its port. */
function isOwnHost(host: string | undefined, port: number): boolean {
  const lowered = host?.toLowerCase();
  return lowered === `${HOST}:${port}` || lowered === `localhost:${port}`;
}

/** Whether a request target (`req.url`) is the one path it serves, with any query; a target that isn't a URL isn't. */
function isOwnPath(target: string | undefined, port: number): boolean {
  try {
    return new URL(target ?? "/", `http://${HOST}:${port}`).pathname === PATH;
  } catch {
    return false;
  }
}

/** Answers with a bare status: no body, no detail. Node discards whatever of the request is unread. */
function refuse(res: ServerResponse, status: number): void {
  res.writeHead(status, { "content-length": "0" }).end();
}

/**
 * The request's body, or null once it passes `limit` bytes: from then on the rest is read and
 * discarded, never kept, so the client can finish sending and read the refusal.
 */
function readBody(req: IncomingMessage, limit: number): Promise<Buffer | null> {
  if (Number(req.headers["content-length"] ?? 0) > limit) {
    req.resume();
    return Promise.resolve(null);
  }
  return new Promise((resolve, reject) => {
    let chunks: Buffer[] | null = [];
    let size = 0;
    req.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (chunks !== null && size > limit) {
        chunks = null;
        resolve(null);
      }
      chunks?.push(chunk);
    });
    req.on("end", () => {
      resolve(chunks === null ? null : Buffer.concat(chunks));
    });
    req.on("error", reject);
  });
}

/** The request as a web `Request` for the SDK, aborted when the client goes away first. */
function webRequest(req: IncomingMessage, res: ServerResponse, port: number, body: Buffer): Request {
  const headers = new Headers();
  for (let index = 0; index + 1 < req.rawHeaders.length; index += 2) {
    headers.append(req.rawHeaders[index] ?? "", req.rawHeaders[index + 1] ?? "");
  }
  const aborted = new AbortController();
  res.on("close", () => {
    if (!res.writableFinished) {
      aborted.abort();
    }
  });
  const method = req.method ?? "GET";
  return new Request(`http://${HOST}:${port}${PATH}`, {
    method,
    headers,
    signal: aborted.signal,
    ...(method === "GET" || method === "HEAD" ? {} : { body: new Uint8Array(body) }),
  });
}

/** The body as JSON, or undefined when it is empty or isn't JSON (the SDK answers that). */
function parseJson(body: Buffer): unknown {
  try {
    return body.length === 0 ? undefined : (JSON.parse(body.toString("utf8")) as unknown);
  } catch {
    return undefined;
  }
}

/** Writes the SDK's response, streaming its body (a JSON body, or server-sent events). */
async function send(res: ServerResponse, response: Response): Promise<void> {
  const headers: Record<string, string> = {};
  response.headers.forEach((value, name) => {
    headers[name] = value;
  });
  res.writeHead(response.status, headers);
  if (!response.body) {
    res.end();
    return;
  }
  const stream = Readable.fromWeb(response.body as WebReadableStream<Uint8Array>);
  await new Promise<void>((resolve) => {
    stream.on("error", () => {
      res.destroy();
      resolve();
    });
    res.on("close", () => {
      stream.destroy();
      resolve();
    });
    stream.pipe(res);
  });
}

export class McpListener {
  private server: Server | null = null;
  private current: ListenerState;

  constructor(private readonly options: McpListenerOptions) {
    this.current = { state: "starting", port: options.port };
  }

  get state(): ListenerState {
    return this.current;
  }

  /** Starts listening; when it can't, the state says why. Never rejects. */
  async start(): Promise<void> {
    const port = this.options.port;
    const server = createServer((req, res) => {
      void this.handle(server, req, res);
    });
    try {
      await new Promise<void>((resolve, reject) => {
        server.once("error", reject);
        server.listen({ host: HOST, port, exclusive: true }, () => {
          server.off("error", reject);
          resolve();
        });
      });
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      this.setState(
        code === "EADDRINUSE"
          ? { state: "port_taken", port }
          : { state: "failed", port, message: error instanceof Error ? error.message : String(error) },
      );
      return;
    }
    server.on("error", (error) => {
      console.error("[mcp] the listener failed:", error);
    });
    this.server = server;
    this.setState({ state: "listening", port: (server.address() as AddressInfo).port });
  }

  /** Stops listening and closes every connection (the app is quitting). */
  async close(): Promise<void> {
    const server = this.server;
    this.server = null;
    if (server) {
      await new Promise<void>((resolve) => {
        server.close(() => {
          resolve();
        });
        server.closeAllConnections();
      });
    }
  }

  private setState(state: ListenerState): void {
    this.current = state;
    this.options.onState(state);
  }

  private async handle(server: Server, req: IncomingMessage, res: ServerResponse): Promise<void> {
    const port = (server.address() as AddressInfo | null)?.port ?? 0;
    if (req.headers.origin !== undefined || !isOwnHost(req.headers.host, port)) {
      refuse(res, 403);
      return;
    }
    if (!isOwnPath(req.url, port)) {
      refuse(res, 404);
      return;
    }
    try {
      const body = await readBody(req, MAX_BODY_BYTES);
      if (body === null) {
        refuse(res, 413);
        return;
      }
      await send(res, await this.options.serve(webRequest(req, res, port, body), parseJson(body)));
    } catch (error) {
      console.error("[mcp] a request failed:", error);
      if (!res.headersSent) {
        refuse(res, 500);
      } else {
        res.destroy();
      }
    }
  }
}
