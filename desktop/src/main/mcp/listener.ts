/**
 * The MCP server's HTTP listener (design §5.3, §5.7): `127.0.0.1:<port>`, path `/mcp`, only while
 * Preferences › Agents has it on. It serves this machine's programs and nothing else:
 *
 * - a request with an `Origin` header comes from a web page, and one whose `Host` isn't
 *   `127.0.0.1:<port>` or `localhost:<port>` could be a web page reaching it by DNS rebinding: both
 *   are refused with 403 and no body;
 * - a body over {@link MAX_BODY_BYTES} is refused with 413 before it is parsed;
 * - every other path is 404.
 *
 * What passes goes to `serve` (the MCP SDK's handler) as a web `Request`, with its JSON body parsed
 * once here, and the `Response` streams back. Stopping closes every connection, so turning agents
 * off disconnects everyone at once.
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
  | { readonly state: "off" }
  | { readonly state: "starting" | "listening" | "port_taken"; readonly port: number }
  | { readonly state: "failed"; readonly port: number; readonly message: string };

/** Where to listen: `port` while `enabled`, nowhere otherwise. */
export interface ListenerSettings {
  readonly enabled: boolean;
  readonly port: number;
}

export interface McpListenerOptions {
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
  private current: ListenerState = { state: "off" };
  private wanted: ListenerSettings = { enabled: false, port: 0 };
  /** Changes run one at a time, in order. */
  private changes: Promise<void> = Promise.resolve();

  constructor(private readonly options: McpListenerOptions) {}

  get state(): ListenerState {
    return this.current;
  }

  /**
   * Listens on `port` while `enabled`, else stops. A change of port restarts it; the same settings
   * again try once more when the port was taken or listening failed, and otherwise change nothing.
   */
  configure(settings: ListenerSettings): Promise<void> {
    this.changes = this.changes.then(() => this.apply(settings));
    return this.changes;
  }

  /** Stops listening and closes every connection. */
  close(): Promise<void> {
    return this.configure({ enabled: false, port: this.wanted.port });
  }

  private async apply(settings: ListenerSettings): Promise<void> {
    const same = settings.enabled === this.wanted.enabled && settings.port === this.wanted.port;
    this.wanted = settings;
    if (same && (this.current.state === "listening" || this.current.state === "off")) {
      return;
    }
    await this.stop();
    if (settings.enabled) {
      await this.start(settings.port);
    }
  }

  private async start(port: number): Promise<void> {
    this.setState({ state: "starting", port });
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

  private async stop(): Promise<void> {
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
    if (this.current.state !== "off") {
      this.setState({ state: "off" });
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
    if (new URL(req.url ?? "/", `http://${HOST}:${port}`).pathname !== PATH) {
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
