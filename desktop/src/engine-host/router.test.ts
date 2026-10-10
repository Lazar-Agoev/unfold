import { describe, expect, it } from "vitest";
import type { HostToRenderer } from "../shared/transport";
import type { EngineBinding } from "./binding";
import { asEngineBinding } from "./binding";
import type { WindowPort } from "./router";
import { CLOSING_WRITES, EngineRouter, errorFromRejection, MAIN_PROCESS_ONLY } from "./router";

interface Deferred<T> {
  promise: Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

interface Dispatch {
  readonly windowId: number;
  readonly command: unknown;
  readonly result: Deferred<string>;
}

/** An in-memory addon that records calls and lets the test settle each dispatch and detach. */
class FakeBinding implements EngineBinding {
  readonly dispatches = new Map<number, Dispatch>();
  readonly cancelled: number[] = [];
  /** Dispatches, detaches (started and finished), and shutdowns in call order. */
  readonly order: string[] = [];
  sink: ((message: string) => void) | null = null;
  /** When set, `detachWindow` waits for the test to settle it. */
  detachResult: Deferred<undefined> | null = null;

  init(): void {
    this.order.push("init");
  }
  subscribe(sink: (message: string) => void): void {
    this.sink = sink;
  }
  dispatch(windowId: number, requestId: number, command: string): Promise<string> {
    const result = deferred<string>();
    this.order.push(`dispatch ${requestId}`);
    this.dispatches.set(requestId, { windowId, command: JSON.parse(command) as unknown, result });
    return result.promise;
  }
  cancel(requestId: number): void {
    this.cancelled.push(requestId);
  }
  async detachWindow(windowId: number): Promise<void> {
    this.order.push(`detach ${windowId}`);
    await this.detachResult?.promise;
    this.order.push(`detached ${windowId}`);
  }
  shutdown(): Promise<void> {
    this.order.push("shutdown");
    return Promise.resolve();
  }

  /** The commands Rust received, in dispatch order. */
  commands(): unknown[] {
    return [...this.dispatches.values()].map((d) => d.command);
  }
}

class FakePort implements WindowPort {
  readonly sent: HostToRenderer[] = [];
  closed = false;
  private handler: ((data: unknown) => void) | null = null;

  post(message: HostToRenderer): void {
    this.sent.push(message);
  }
  onMessage(handler: (data: unknown) => void): void {
    this.handler = handler;
  }
  close(): void {
    this.closed = true;
  }
  /** Simulates the renderer sending a message. */
  send(data: unknown): void {
    this.handler?.(data);
  }
  /** What the window received after its `status` message. */
  received(): HostToRenderer[] {
    return this.sent.slice(1);
  }
}

/** Waits for pending promise callbacks. */
const flush = () =>
  new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
const CONNECTED: HostToRenderer = { kind: "status", state: "connected" };
const UNAVAILABLE = { code: "engine_unavailable", message: "The Unfold engine could not be loaded." };

function setup() {
  const binding = new FakeBinding();
  const logs: string[] = [];
  const router = new EngineRouter({ binding }, (message) => logs.push(message));
  return { binding, router, logs };
}

describe("EngineRouter: attach and dispatch", () => {
  it("reports connected on attach and opens nothing", () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, port);
    expect(port.sent).toEqual([CONNECTED]);
    expect(binding.order).toEqual([]);
  });

  it("dispatches at once, with host-wide request ids, and answers the right window", async () => {
    const { binding, router } = setup();
    const a = new FakePort();
    const b = new FakePort();
    router.attach(1, a);
    router.attach(2, b);
    a.send({ kind: "invoke", id: 1, command: "project_summary", args: { project: "p-aaaaaaaaaa" } });
    b.send({ kind: "invoke", id: 1, command: "workspace" });

    expect([...binding.dispatches.entries()].map(([id, d]) => [id, d.windowId, d.command])).toEqual([
      [1, 1, { command: "project_summary", args: { project: "p-aaaaaaaaaa" } }],
      [2, 2, { command: "workspace", args: {} }],
    ]);
    binding.dispatches.get(2)?.result.resolve('{"projects":[]}');
    binding.dispatches.get(1)?.result.reject(new Error('{"code":"unknown_project","message":"No such project."}'));
    await flush();

    expect(a.received()).toEqual([
      { kind: "result", id: 1, ok: false, error: { code: "unknown_project", message: "No such project." } },
    ]);
    expect(b.received()).toEqual([{ kind: "result", id: 1, ok: true, value: { projects: [] } }]);
  });

  it("answers a result that is not JSON, or a dispatch that throws, with internal", async () => {
    const binding = new FakeBinding();
    const router = new EngineRouter({ binding }, () => undefined);
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 1, command: "workspace", args: {} });
    binding.dispatches.get(1)?.result.resolve("not json");
    binding.dispatch = () => {
      throw new Error("boom");
    };
    port.send({ kind: "invoke", id: 2, command: "workspace", args: {} });
    await flush();
    expect(port.received().map((m) => (m.kind === "result" && !m.ok ? [m.id, m.error.code] : null))).toEqual([
      [1, "internal"],
      [2, "internal"],
    ]);
  });

  it("cancels a running request on the window's cancel", () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 7, command: "search_text", args: { query: "x" } });
    port.send({ kind: "cancel", id: 7 });
    port.send({ kind: "cancel", id: 8 });
    expect(binding.cancelled).toEqual([1]);
  });

  it("rejects malformed and duplicate requests without dispatching", () => {
    const { binding, router, logs } = setup();
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 1, command: "Bad Name", args: {} });
    port.send({ kind: "invoke", id: 2, command: "workspace", args: [] });
    port.send({ kind: "invoke", id: 3, command: "workspace", args: {} });
    port.send({ kind: "invoke", id: 3, command: "workspace", args: {} });
    port.send({ kind: "teleport", id: 4 });
    port.send({ kind: "invoke", command: "workspace" });

    const failures = port.sent.flatMap((m) => (m.kind === "result" && !m.ok ? [[m.id, m.error.code]] : []));
    expect(failures).toEqual([
      [1, "invalid_message"],
      [2, "invalid_message"],
      [3, "invalid_message"],
      [4, "invalid_message"],
    ]);
    expect(binding.commands()).toEqual([{ command: "workspace", args: {} }]);
    expect(logs).toEqual(["window 1 sent a message without a valid id"]);
  });

  it("answers every invoke with engine_unavailable when the addon is missing", () => {
    const router = new EngineRouter({ unavailable: UNAVAILABLE }, () => undefined);
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 1, command: "workspace", args: {} });
    expect(port.sent).toEqual([
      { kind: "status", state: "unavailable", error: UNAVAILABLE },
      { kind: "result", id: 1, ok: false, error: UNAVAILABLE },
    ]);
  });
});

describe("EngineRouter: sink fan-out", () => {
  it("sends events to the listed windows only", () => {
    const { router } = setup();
    const [a, b, c] = [new FakePort(), new FakePort(), new FakePort()];
    router.attach(1, a);
    router.attach(2, b);
    router.attach(3, c);
    const event = { type: "board_conflict", project: "p-aaaaaaaaaa", board: ".unfold/boards/b.json" };
    router.handleSinkMessage(JSON.stringify({ kind: "event", windows: [2, 9], event }));
    expect(a.received()).toEqual([]);
    expect(b.received()).toEqual([{ kind: "event", event }]);
    expect(c.received()).toEqual([]);
  });

  it("broadcasts to every attached window, and not to detached ones", () => {
    const { router } = setup();
    const [a, b, gone] = [new FakePort(), new FakePort(), new FakePort()];
    router.attach(1, a);
    router.attach(2, b);
    router.attach(3, gone);
    router.detach(3);
    const event = { type: "graph_updated", project: "p-aaaaaaaaaa", everything: false };
    router.handleSinkMessage(JSON.stringify({ kind: "broadcast", event }));
    expect(a.received()).toEqual([{ kind: "event", event }]);
    expect(b.received()).toEqual([{ kind: "event", event }]);
    expect(gone.received()).toEqual([]);
  });

  it("routes stream messages by request id and window, and drops late ones", async () => {
    const { binding, router } = setup();
    const [a, b] = [new FakePort(), new FakePort()];
    router.attach(1, a);
    router.attach(2, b);
    b.send({ kind: "invoke", id: 9, command: "compile", args: { project: "p-aaaaaaaaaa" } });

    router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 2, requestId: 1, message: { done: 3 } }));
    router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 1, requestId: 1, message: { done: 4 } }));
    binding.dispatches.get(1)?.result.resolve("{}");
    await flush();
    router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 2, requestId: 1, message: { done: 5 } }));

    expect(a.received()).toEqual([]);
    expect(b.received()).toEqual([
      { kind: "stream", id: 9, message: { done: 3 } },
      { kind: "result", id: 9, ok: true, value: {} },
    ]);
  });

  it("posts a request's stream messages before its result, at once and in the addon's order", async () => {
    // The addon settles a request only after its stream messages reached the sink (its own tests
    // prove that boundary); the router keeps that order and holds nothing back.
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 4, command: "search_text", args: { query: "retry" } });

    for (const path of ["a.ts", "b.ts"]) {
      router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 1, requestId: 1, message: { path } }));
    }
    const result = binding.dispatches.get(1)?.result;
    result?.resolve("{}");
    await result?.promise;

    expect(port.received()).toEqual([
      { kind: "stream", id: 4, message: { path: "a.ts" } },
      { kind: "stream", id: 4, message: { path: "b.ts" } },
      { kind: "result", id: 4, ok: true, value: {} },
    ]);
  });

  it("logs and drops malformed sink messages", () => {
    const { router, logs } = setup();
    router.handleSinkMessage("not json");
    router.handleSinkMessage("[]");
    router.handleSinkMessage(JSON.stringify({ kind: "event", windows: 1, event: {} }));
    router.handleSinkMessage(JSON.stringify({ kind: "broadcast", event: "preferences_changed" }));
    router.handleSinkMessage(JSON.stringify({ kind: "mystery" }));
    expect(logs).toEqual([
      "dropped a sink message that is not JSON",
      "dropped a sink message that is not an object",
      "dropped a malformed event message",
      "dropped a malformed broadcast message",
      "dropped a sink message of unknown kind",
    ]);
  });
});

describe("EngineRouter: trusted commands", () => {
  it("lists exactly the main-process-only commands and closing writes", () => {
    expect([...MAIN_PROCESS_ONLY].sort()).toEqual(
      ["add_project", "export_mermaid", "forget_window", "save_window_bounds", "saved_windows"].sort(),
    );
    expect([...CLOSING_WRITES].sort()).toEqual(
      [
        "save_board",
        "repair_board",
        "publish_board",
        "discard_changes",
        "unpublish_board",
        "delete_board",
        "duplicate_board",
        "set_board_folder",
        "create_folder",
        "rename_folder",
        "delete_folder",
        "save_board_view",
        "save_window_state",
        "save_window_bounds",
      ].sort(),
    );
  });

  it("refuses main-process-only commands and anything carrying trusted from a window", () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, port);
    let id = 0;
    for (const command of MAIN_PROCESS_ONLY) {
      port.send({ kind: "invoke", id: ++id, command, args: { path: "/elsewhere" } });
    }
    port.send({ kind: "invoke", id: ++id, command: "workspace", args: {}, trusted: true });
    port.send({ kind: "invoke", id: ++id, command: "workspace", args: { trusted: true } });
    // Untrusted siblings of trusted commands go through: they name no outside path.
    port.send({ kind: "invoke", id: ++id, command: "export_mermaid_text", args: { project: "p-aaaaaaaaaa" } });
    port.send({ kind: "invoke", id: id + 1, command: "readd_project", args: { path: "/removed" } });

    const refused = port.received().flatMap((m) => (m.kind === "result" && !m.ok ? [m.error.code] : []));
    expect(refused).toEqual(Array<string>(MAIN_PROCESS_ONLY.size + 2).fill("untrusted"));
    expect(binding.commands()).toEqual([
      { command: "export_mermaid_text", args: { project: "p-aaaaaaaaaa" } },
      { command: "readd_project", args: { path: "/removed" } },
    ]);
  });

  it("dispatches the main process's requests with trusted, for any window id", async () => {
    const { binding, router } = setup();
    const added = router.trusted(0, "add_project", { path: "/code/shop" });
    const [dispatched] = [...binding.dispatches.values()];
    expect(dispatched?.windowId).toBe(0);
    expect(dispatched?.command).toEqual({ command: "add_project", args: { path: "/code/shop" }, trusted: true });
    dispatched?.result.resolve('{"status":"added","project":{"id":"p-aaaaaaaaaa"}}');
    expect(await added).toEqual({ ok: true, value: { status: "added", project: { id: "p-aaaaaaaaaa" } } });

    const forgotten = router.trusted(4, "forget_window", { key: "k" });
    binding.dispatches.get(2)?.result.reject(new Error('{"code":"invalid_args","message":"bad key"}'));
    expect(await forgotten).toEqual({ ok: false, error: { code: "invalid_args", message: "bad key" } });

    expect(await router.trusted(0, "Bad Name", {})).toMatchObject({ ok: false, error: { code: "invalid_message" } });
    const garbled = router.trusted(0, "saved_windows", {});
    binding.dispatches.get(3)?.result.resolve("{");
    expect(await garbled).toMatchObject({ ok: false, error: { code: "internal" } });
  });

  it("answers trusted requests with engine_unavailable when the addon is missing", async () => {
    const router = new EngineRouter({ unavailable: UNAVAILABLE }, () => undefined);
    expect(await router.trusted(0, "saved_windows", {})).toEqual({ ok: false, error: UNAVAILABLE });
  });
});

describe("EngineRouter: detach", () => {
  it("cancels in-flight work, closes the port, detaches the window in Rust, and drops late results", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 1, command: "project_summary", args: {} });
    router.detach(1);
    binding.dispatches.get(1)?.result.resolve("{}");
    await flush();

    expect(port.closed).toBe(true);
    expect(binding.cancelled).toEqual([1]);
    expect(binding.order).toEqual(["dispatch 1", "detach 1", "detached 1"]);
    expect(port.sent).toEqual([CONNECTED]);
  });

  it("keeps every closing write in flight and detaches only after they reached Rust", async () => {
    const { binding, router, logs } = setup();
    const port = new FakePort();
    router.attach(1, port);
    // save_window_bounds is a closing write only the main process sends (tested below).
    const windowWrites = [...CLOSING_WRITES].filter((command) => !MAIN_PROCESS_ONLY.has(command));
    windowWrites.forEach((command, i) => {
      port.send({ kind: "invoke", id: i + 1, command, args: { project: "p-aaaaaaaaaa" } });
    });
    port.send({ kind: "invoke", id: 100, command: "search_text", args: {} });
    const search = binding.dispatches.size;
    router.detach(1);
    await flush();

    expect(search).toBe(windowWrites.length + 1);
    expect(binding.cancelled).toEqual([search]);
    expect(binding.order.slice(-3)).toEqual([`dispatch ${search}`, "detach 1", "detached 1"]);
    for (const dispatched of binding.dispatches.values()) {
      dispatched.result.resolve("null");
    }
    await flush();
    expect(port.sent).toEqual([CONNECTED]);
    expect(logs).toEqual([]);
  });

  it("detaches after the main process's trusted bounds save for the window reached Rust", async () => {
    const { binding, router } = setup();
    router.attach(1, new FakePort());
    const bounds = { key: "k", bounds: { x: 0, y: 0, width: 1100, height: 760 }, maximized: false };
    const saved = router.trusted(1, "save_window_bounds", bounds);
    router.detach(1);
    await flush();

    expect(binding.order).toEqual(["dispatch 1", "detach 1", "detached 1"]);
    expect(binding.cancelled).toEqual([]);
    binding.dispatches.get(1)?.result.resolve("null");
    expect(await saved).toEqual({ ok: true, value: null });
  });

  it("logs a closing write that fails after its window closed", async () => {
    const { binding, router, logs } = setup();
    const port = new FakePort();
    router.attach(1, port);
    port.send({ kind: "invoke", id: 1, command: "publish_board", args: { project: "p-aaaaaaaaaa" } });
    router.detach(1);
    binding.dispatches.get(1)?.result.reject(new Error('{"code":"io","message":"disk full"}'));
    await flush();
    expect(logs).toEqual(["publish_board for closed window 1 failed: disk full"]);
    expect(port.sent).toEqual([CONNECTED]);
  });

  it("runs one window's detaches in order", async () => {
    const { binding, router } = setup();
    binding.detachResult = deferred();
    router.attach(1, new FakePort());
    router.detach(1);
    router.detach(1);
    await flush();
    expect(binding.order).toEqual(["detach 1"]);

    binding.detachResult.resolve(undefined);
    await flush();
    expect(binding.order).toEqual(["detach 1", "detached 1", "detach 1", "detached 1"]);
  });

  it("detaches a window that closed before it attached (its trusted writes may have reached Rust)", async () => {
    const { binding, router } = setup();
    router.detach(5);
    await flush();
    expect(binding.order).toEqual(["detach 5", "detached 5"]);
  });

  it("logs a failed detach", async () => {
    const { binding, router, logs } = setup();
    binding.detachWindow = () => Promise.reject(new Error('{"code":"internal","message":"poisoned"}'));
    router.attach(1, new FakePort());
    router.detach(1);
    await flush();
    expect(logs).toEqual(["detaching window 1 failed: poisoned"]);
  });

  it("only releases the port when the addon is missing", () => {
    const router = new EngineRouter({ unavailable: UNAVAILABLE }, () => undefined);
    const port = new FakePort();
    router.attach(1, port);
    router.detach(1);
    expect(port.closed).toBe(true);
  });
});

describe("EngineRouter: re-attach (reload)", () => {
  it("swaps the port, cancels the old page's reads, keeps its closing writes, and keeps Rust's window state", async () => {
    const { binding, router } = setup();
    const first = new FakePort();
    const second = new FakePort();
    router.attach(1, first);
    first.send({ kind: "invoke", id: 1, command: "save_board", args: { project: "p-aaaaaaaaaa" } });
    first.send({ kind: "invoke", id: 2, command: "list_boards", args: { project: "p-aaaaaaaaaa" } });
    router.attach(1, second);

    expect(first.closed).toBe(true);
    expect(binding.cancelled).toEqual([2]);
    expect(binding.order).toEqual(["dispatch 1", "dispatch 2"]);
    expect(second.sent).toEqual([CONNECTED]);

    // The new page reuses renderer ids from 1; the old page's results never reach it.
    second.send({ kind: "invoke", id: 1, command: "workspace", args: {} });
    binding.dispatches.get(1)?.result.resolve("{}");
    binding.dispatches.get(3)?.result.resolve('{"projects":[]}');
    await flush();
    expect(second.received()).toEqual([{ kind: "result", id: 1, ok: true, value: { projects: [] } }]);

    // Messages on the replaced port are ignored.
    first.send({ kind: "invoke", id: 3, command: "workspace", args: {} });
    expect(binding.dispatches.size).toBe(3);
  });
});

describe("EngineRouter: shutdown", () => {
  it("detaches every window, waits for every detach, then shuts the engine down", async () => {
    const { binding, router } = setup();
    binding.detachResult = deferred();
    const a = new FakePort();
    router.attach(1, a);
    router.attach(2, new FakePort());
    a.send({ kind: "invoke", id: 1, command: "save_board", args: { project: "p-aaaaaaaaaa" } });
    router.detach(3); // A window that closed just before the app quit.

    let finished = false;
    const done = router.shutdown().then(() => {
      finished = true;
    });
    await flush();
    expect(finished).toBe(false);
    expect(binding.order).toEqual(["dispatch 1", "detach 3", "detach 1", "detach 2"]);

    binding.detachResult.resolve(undefined);
    await done;
    expect(binding.order).toEqual([
      "dispatch 1",
      "detach 3",
      "detach 1",
      "detach 2",
      "detached 3",
      "detached 1",
      "detached 2",
      "shutdown",
    ]);
    expect(binding.cancelled).toEqual([]);
  });

  it("logs a failed engine shutdown and still resolves", async () => {
    const { binding, router, logs } = setup();
    binding.shutdown = () => Promise.reject(new Error("gone"));
    await router.shutdown();
    expect(logs).toEqual(["shutting the engine down failed: gone"]);
  });

  it("resolves at once when the addon is missing", async () => {
    const router = new EngineRouter({ unavailable: UNAVAILABLE }, () => undefined);
    router.attach(1, new FakePort());
    await expect(router.shutdown()).resolves.toBeUndefined();
  });
});

describe("errorFromRejection", () => {
  it("keeps Rust's code and message, and wraps anything else as internal", () => {
    expect(errorFromRejection(new Error('{"code":"bad_path","message":"Outside the project."}'))).toEqual({
      code: "bad_path",
      message: "Outside the project.",
    });
    expect(errorFromRejection(new Error("boom"))).toEqual({ code: "internal", message: "boom" });
    expect(errorFromRejection("text")).toEqual({ code: "internal", message: "text" });
  });
});

describe("asEngineBinding", () => {
  it("names the functions a module is missing", () => {
    expect(() => asEngineBinding({ init: () => undefined, dispatch: () => undefined })).toThrow(
      "the addon module is missing: subscribe, cancel, detachWindow, shutdown",
    );
    expect(() => asEngineBinding(null)).toThrow("did not export an object");
  });
});
