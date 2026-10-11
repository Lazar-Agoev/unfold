import { createServer, request as httpRequest, type Server } from "node:http";
import { connect, type AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { MAX_BODY_BYTES, McpListener, type ListenerState } from "./listener";

interface Reply {
  readonly status: number;
  readonly body: string;
}

/** Sends a raw HTTP request, with exactly the headers given (Node adds no Origin). */
function send(port: number, options: { path?: string; method?: string; headers?: Record<string, string>; body?: string | Buffer }): Promise<Reply> {
  return new Promise((resolve, reject) => {
    const req = httpRequest(
      { host: "127.0.0.1", port, path: options.path ?? "/mcp", method: options.method ?? "POST", headers: { host: `127.0.0.1:${port}`, ...options.headers } },
      (res) => {
        const chunks: Buffer[] = [];
        res.on("data", (chunk: Buffer) => chunks.push(chunk));
        res.on("end", () => {
          resolve({ status: res.statusCode ?? 0, body: Buffer.concat(chunks).toString("utf8") });
        });
      },
    );
    req.on("error", reject);
    req.end(options.body);
  });
}

/** Sends `request` as raw bytes and resolves with the reply's status line. */
function sendRaw(port: number, request: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const socket = connect(port, "127.0.0.1", () => {
      socket.end(request);
    });
    const chunks: Buffer[] = [];
    socket.on("data", (chunk: Buffer) => chunks.push(chunk));
    socket.on("end", () => {
      resolve(Buffer.concat(chunks).toString("utf8").split("\r\n")[0] ?? "");
    });
    socket.on("error", reject);
  });
}

const listeners: McpListener[] = [];
const blockers: Server[] = [];

afterEach(async () => {
  await Promise.all(listeners.splice(0).map((listener) => listener.close()));
  await Promise.all(blockers.splice(0).map((server) => new Promise((resolve) => server.close(resolve))));
});

/** A listener whose `serve` echoes what reached it. */
function setup(serve?: (request: Request, body: unknown) => Promise<Response>) {
  const states: ListenerState[] = [];
  const served: { request: Request; body: unknown }[] = [];
  const listener = new McpListener({
    serve:
      serve ??
      ((request, body) => {
        served.push({ request, body });
        return Promise.resolve(Response.json({ method: request.method, body }));
      }),
    onState: (state) => states.push(state),
  });
  listeners.push(listener);
  return { listener, states, served };
}

async function listening(listener: McpListener): Promise<number> {
  await listener.configure({ enabled: true, port: 0 });
  const state = listener.state;
  if (state.state !== "listening") {
    throw new Error(`not listening: ${state.state}`);
  }
  return state.port;
}

/** A port another program holds. */
async function takenPort(): Promise<number> {
  const blocker = createServer();
  blockers.push(blocker);
  await new Promise<void>((resolve) => blocker.listen(0, "127.0.0.1", resolve));
  return (blocker.address() as AddressInfo).port;
}

describe("the listener's checks", () => {
  it("serves POST /mcp from this machine, with the body parsed once", async () => {
    const { listener, served } = setup();
    const port = await listening(listener);
    const reply = await send(port, { headers: { "content-type": "application/json", "user-agent": "claude-code/2.0" }, body: '{"jsonrpc":"2.0"}' });
    expect(reply).toEqual({ status: 200, body: '{"method":"POST","body":{"jsonrpc":"2.0"}}' });
    expect(served[0]?.request.headers.get("user-agent")).toBe("claude-code/2.0");
    expect(await served[0]?.request.text()).toBe('{"jsonrpc":"2.0"}');
    // localhost at the port is this machine too.
    expect((await send(port, { headers: { host: `localhost:${port}` }, body: "{}" })).status).toBe(200);
    // A body that isn't JSON still reaches the SDK, which answers it.
    expect(await send(port, { body: "not json" })).toEqual({ status: 200, body: '{"method":"POST"}' });
  });

  it("refuses requests from web pages, and Host names that aren't its own, with a bare 403", async () => {
    const { listener, served } = setup();
    const port = await listening(listener);
    for (const headers of [
      { origin: "https://example.com" },
      { origin: "null" },
      { origin: `http://127.0.0.1:${port}` },
      { host: `evil.example:${port}` },
      { host: "127.0.0.1" },
      { host: `localhost:${port + 1}` },
      { host: `127.0.0.1.nip.io:${port}` },
    ]) {
      expect(await send(port, { headers, body: "{}" })).toEqual({ status: 403, body: "" });
    }
    expect(served).toEqual([]);
  });

  it("serves only /mcp", async () => {
    const { listener } = setup();
    const port = await listening(listener);
    expect(await send(port, { path: "/", method: "GET" })).toEqual({ status: 404, body: "" });
    expect(await send(port, { path: "/mcp/../admin" })).toEqual({ status: 404, body: "" });
    expect((await send(port, { path: "/mcp?x=1", body: "{}" })).status).toBe(200);
    // A request target that isn't a URL is answered too, not left hanging.
    for (const target of ["http://[", "//[x", "http://%/mcp"]) {
      expect(await sendRaw(port, `GET ${target} HTTP/1.1\r\nHost: 127.0.0.1:${port}\r\nConnection: close\r\n\r\n`)).toBe("HTTP/1.1 404 Not Found");
    }
  });

  it("refuses a body over the cap with 413, before parsing it", async () => {
    const { listener, served } = setup();
    const port = await listening(listener);
    const big = Buffer.alloc(MAX_BODY_BYTES + 1, "a");
    expect(await send(port, { body: big })).toEqual({ status: 413, body: "" });
    // Without a Content-Length too (chunked), reading stops at the cap.
    expect(await send(port, { headers: { "transfer-encoding": "chunked" }, body: big })).toEqual({ status: 413, body: "" });
    expect((await send(port, { body: Buffer.alloc(MAX_BODY_BYTES, " ") })).status).toBe(200);
    expect(served).toHaveLength(1);
  });

  it("answers 500 when serving fails", async () => {
    const { listener } = setup(() => Promise.reject(new Error("broken")));
    const port = await listening(listener);
    const error = console.error;
    console.error = () => undefined;
    try {
      expect(await send(port, { body: "{}" })).toEqual({ status: 500, body: "" });
    } finally {
      console.error = error;
    }
  });
});

describe("starting and stopping", () => {
  it("listens while enabled, moves with the port, and stops when turned off", async () => {
    const { listener, states } = setup();
    const first = await listening(listener);
    await listener.configure({ enabled: true, port: 0 });
    // The same settings again change nothing.
    expect(states).toEqual([
      { state: "starting", port: 0 },
      { state: "listening", port: first },
    ]);
    const second = await takenPort();
    await new Promise<void>((resolve) => {
      blockers.pop()?.close(() => {
        resolve();
      });
    });
    await listener.configure({ enabled: true, port: second });
    expect(listener.state).toEqual({ state: "listening", port: second });
    expect((await send(second, { body: "{}" })).status).toBe(200);
    await expect(send(first, { body: "{}" })).rejects.toThrow();

    await listener.configure({ enabled: false, port: second });
    expect(listener.state).toEqual({ state: "off" });
    await expect(send(second, { body: "{}" })).rejects.toThrow();
  });

  it("says when the port is taken, and tries again when asked", async () => {
    const { listener, states } = setup();
    const port = await takenPort();
    await listener.configure({ enabled: true, port });
    expect(listener.state).toEqual({ state: "port_taken", port });
    expect(states.at(-1)).toEqual({ state: "port_taken", port });

    await new Promise<void>((resolve) => {
      blockers.pop()?.close(() => {
        resolve();
      });
    });
    await listener.configure({ enabled: true, port });
    expect(listener.state).toEqual({ state: "listening", port });
  });

  it("disconnects everyone when it stops", async () => {
    let reached!: () => void;
    const waiting = new Promise<void>((resolve) => {
      reached = resolve;
    });
    // A request the server never answers, like a client waiting on a long tool call.
    const { listener } = setup(() => {
      reached();
      return new Promise<Response>(() => undefined);
    });
    const port = await listening(listener);
    const pending = send(port, { body: "{}" });
    await waiting;
    await listener.close();
    await expect(pending).rejects.toThrow();
  });
});
