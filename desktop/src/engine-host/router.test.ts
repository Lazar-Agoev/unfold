import { describe, expect, it } from "vitest";
import type { HostToRenderer } from "../shared/transport";
import type { EngineBinding } from "./binding";
import { asEngineBinding } from "./binding";
import type { WindowPort } from "./router";
import { EngineRouter, errorFromRejection } from "./router";

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

/** An in-memory addon that records calls and lets the test settle each dispatch. */
class FakeBinding implements EngineBinding {
  readonly calls: string[] = [];
  readonly dispatches = new Map<number, { windowId: number; command: unknown; result: Deferred<string> }>();
  readonly cancelled: number[] = [];
  /** Dispatches and closes in call order, to check what reached Rust before a close. */
  readonly order: string[] = [];
  sink: ((message: string) => void) | null = null;
  openResult: Deferred<string> | null = null;

  init(options: string): void {
    this.calls.push(`init ${options}`);
  }
  subscribe(sink: (message: string) => void): void {
    this.sink = sink;
  }
  openWorkspace(windowId: number, root: string): Promise<string> {
    this.calls.push(`open ${windowId} ${root}`);
    return this.openResult?.promise ?? Promise.resolve("{}");
  }
  closeWorkspace(windowId: number): Promise<void> {
    this.calls.push(`close ${windowId}`);
    this.order.push(`close ${windowId}`);
    return Promise.resolve();
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
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup() {
  const binding = new FakeBinding();
  const logs: string[] = [];
  const router = new EngineRouter({ binding }, (message) => logs.push(message));
  return { binding, router, logs };
}

describe("EngineRouter", () => {
  it("reports connected and opens the workspace on attach", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    await flush();
    expect(port.sent).toEqual([{ kind: "status", state: "connected" }]);
    expect(binding.calls).toEqual(["open 1 /repo"]);
  });

  it("dispatches invokes with host-wide request ids and returns results to the right window", async () => {
    const { binding, router } = setup();
    const a = new FakePort();
    const b = new FakePort();
    router.attach(1, "/repo", a);
    router.attach(2, "/repo", b);
    a.send({ kind: "invoke", id: 1, command: "graph_status", args: {} });
    b.send({ kind: "invoke", id: 1, command: "list_boards" });
    await flush();

    expect([...binding.dispatches.entries()].map(([id, d]) => [id, d.windowId, d.command])).toEqual([
      [1, 1, { command: "graph_status", args: {} }],
      [2, 2, { command: "list_boards", args: {} }],
    ]);
    binding.dispatches.get(2)?.result.resolve('{"boards":[]}');
    binding.dispatches.get(1)?.result.reject(new Error('{"code":"not_compiled","message":"Compile first."}'));
    await flush();

    expect(a.sent.at(-1)).toEqual({
      kind: "result",
      id: 1,
      ok: false,
      error: { code: "not_compiled", message: "Compile first." },
    });
    expect(b.sent.at(-1)).toEqual({ kind: "result", id: 1, ok: true, value: { boards: [] } });
  });

  it("waits for the workspace to open before dispatching", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "graph_status", args: {} });
    await flush();
    expect(binding.dispatches.size).toBe(0);
    binding.openResult.resolve("{}");
    await flush();
    expect(binding.dispatches.size).toBe(1);
  });

  it("forwards a failed workspace open as an event and still dispatches", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    binding.openResult.reject(new Error('{"code":"folder_locked","message":"Another copy has it."}'));
    await flush();
    expect(port.sent.at(-1)).toEqual({
      kind: "event",
      event: {
        type: "workspace_open_failed",
        folder: "/repo",
        error: { code: "folder_locked", message: "Another copy has it." },
      },
    });
  });

  it("routes events by window list and stream messages by window and request", async () => {
    const { router } = setup();
    const a = new FakePort();
    const b = new FakePort();
    const c = new FakePort();
    router.attach(1, "/repo", a);
    router.attach(2, "/repo", b);
    router.attach(3, "/other", c);
    b.send({ kind: "invoke", id: 9, command: "compile", args: { mode: "check" } });
    await flush();

    router.handleSinkMessage(JSON.stringify({ kind: "event", windows: [1, 2], event: { type: "graph_updated", files: [] } }));
    router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 2, requestId: 1, message: { done: 3 } }));
    router.handleSinkMessage(JSON.stringify({ kind: "stream", windowId: 1, requestId: 1, message: { done: 4 } }));

    const graphUpdated = { kind: "event", event: { type: "graph_updated", files: [] } };
    expect(a.sent.slice(1)).toEqual([graphUpdated]);
    expect(b.sent.slice(1)).toEqual([graphUpdated, { kind: "stream", id: 9, message: { done: 3 } }]);
    expect(c.sent.slice(1)).toEqual([]);
  });

  it("delivers broadcasts to every window, including windows that show no folder", async () => {
    const { router } = setup();
    const folder = new FakePort();
    const other = new FakePort();
    const welcome = new FakePort();
    const closed = new FakePort();
    router.attach(1, "/repo", folder);
    router.attach(2, "/other", other);
    router.attach(3, null, welcome);
    router.attach(4, null, closed);
    router.detach(4);
    await flush();

    const preferences = { theme: "dark", keyBindings: { "palette.open": "Mod+Shift+K" } };
    router.handleSinkMessage(
      JSON.stringify({ kind: "broadcast", event: { type: "preferences_changed", preferences } }),
    );

    const changed = { kind: "event", event: { type: "preferences_changed", preferences } };
    for (const port of [folder, other, welcome]) {
      expect(port.sent.slice(1)).toEqual([changed]);
    }
    expect(closed.sent.slice(1)).toEqual([]);
  });

  it("cancels dispatched requests and requests cancelled before dispatch", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "search_text", args: { query: "x" } });
    port.send({ kind: "cancel", id: 1 });
    binding.openResult.resolve("{}");
    await flush();
    expect(binding.cancelled).toEqual([1]);

    port.send({ kind: "invoke", id: 2, command: "search_text", args: {} });
    await flush();
    port.send({ kind: "cancel", id: 2 });
    expect(binding.cancelled).toEqual([1, 2]);
  });

  it("rejects malformed and duplicate requests without dispatching", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, null, port);
    port.send({ kind: "invoke", id: 1, command: "Bad Name", args: {} });
    port.send({ kind: "invoke", id: 2, command: "graph_status", args: [] });
    port.send({ kind: "invoke", id: 3, command: "graph_status", args: {} });
    port.send({ kind: "invoke", id: 3, command: "graph_status", args: {} });
    port.send({ kind: "invoke", command: "graph_status" });
    await flush();

    const failures = port.sent.flatMap((m) => (m.kind === "result" && !m.ok ? [m.id] : []));
    expect(failures).toEqual([1, 2, 3]);
    expect(binding.dispatches.size).toBe(1);
    expect(binding.calls).toEqual([]);
  });

  it("detach cancels in-flight work, closes the port and the workspace, and drops late results", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "graph_status", args: {} });
    await flush();
    router.detach(1);
    binding.dispatches.get(1)?.result.resolve("{}");
    await flush();

    expect(port.closed).toBe(true);
    expect(binding.cancelled).toEqual([1]);
    expect(binding.calls).toEqual(["open 1 /repo", "close 1"]);
    expect(port.sent).toEqual([{ kind: "status", state: "connected" }]);
  });

  it("detach keeps a board save in flight and closes the workspace after it reached Rust", async () => {
    const { binding, router, logs } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "save_board", args: { board: "b" } });
    port.send({ kind: "invoke", id: 2, command: "graph_status", args: {} });
    await flush();
    router.detach(1);
    await flush();

    expect(binding.cancelled).toEqual([2]);
    expect(binding.order).toEqual(["dispatch 1", "dispatch 2", "close 1"]);
    binding.dispatches.get(1)?.result.resolve("{}");
    await flush();
    expect(port.sent).toEqual([{ kind: "status", state: "connected" }]);
    expect(logs).toEqual([]);
  });

  it("detach keeps the window's last state save in flight, like a board save (the app quitting)", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "save_window_state", args: { state: { key: "w1" } } });
    await flush();
    router.detach(1);
    await flush();

    expect(binding.cancelled).toEqual([]);
    expect(binding.order).toEqual(["dispatch 1", "close 1"]);
  });

  it("detach still dispatches a board save queued behind the workspace open, then closes", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "graph_status", args: {} });
    port.send({ kind: "invoke", id: 2, command: "save_board", args: { board: "b" } });
    router.detach(1);
    await flush();
    expect(binding.order).toEqual([]);

    binding.openResult.resolve("{}");
    await flush();
    expect(binding.order).toEqual(["dispatch 2", "close 1"]);
    expect([...binding.dispatches.values()].map((d) => d.command)).toEqual([
      { command: "save_board", args: { board: "b" } },
    ]);
  });

  it("detach waits for the main process's bounds save queued behind the workspace open, then closes", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    router.attach(1, "/repo", new FakePort());
    const bounds = { key: "k", bounds: { x: 0, y: 0, width: 1100, height: 760 }, maximized: false };
    const saved = router.trusted(1, "save_window_bounds", bounds);
    router.detach(1);
    await flush();
    expect(binding.order).toEqual([]);

    binding.openResult.resolve("{}");
    await flush();
    expect(binding.order).toEqual(["dispatch 1", "close 1"]);
    expect(binding.dispatches.get(1)?.command).toEqual({ command: "save_window_bounds", args: bounds, trusted: true });
    binding.dispatches.get(1)?.result.resolve("null");
    expect(await saved).toEqual({ ok: true, value: null });
  });

  it("logs a board write that fails after its window closed", async () => {
    const { binding, router, logs } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "publish_board", args: { board: "b" } });
    await flush();
    router.detach(1);
    binding.dispatches.get(1)?.result.reject(new Error('{"code":"io","message":"disk full"}'));
    await flush();
    expect(logs).toEqual(["publish_board for closed window 1 failed: disk full"]);
    expect(port.sent).toEqual([{ kind: "status", state: "connected" }]);
  });

  it("re-attaching with the same folder swaps the port and keeps the workspace", async () => {
    const { binding, router } = setup();
    const first = new FakePort();
    const second = new FakePort();
    router.attach(1, "/repo", first);
    router.attach(1, "/repo", second);
    await flush();
    expect(first.closed).toBe(true);
    expect(binding.calls).toEqual(["open 1 /repo"]);

    const third = new FakePort();
    router.attach(1, "/elsewhere", third);
    await flush();
    expect(binding.calls).toEqual(["open 1 /repo", "close 1", "open 1 /elsewhere"]);
  });

  it("Close Folder (re-attaching with no folder) lands a final board save, then closes the workspace", async () => {
    const { binding, router } = setup();
    const folderPort = new FakePort();
    const otherWindow = new FakePort();
    router.attach(1, "/repo", folderPort);
    router.attach(2, "/repo", otherWindow);
    folderPort.send({ kind: "invoke", id: 1, command: "save_board", args: { board: "b" } });
    await flush();

    const welcomePort = new FakePort();
    router.attach(1, null, welcomePort);
    await flush();
    expect(folderPort.closed).toBe(true);
    expect(binding.order).toEqual(["dispatch 1", "close 1"]);
    expect(binding.calls).toEqual(["open 1 /repo", "open 2 /repo", "close 1"]);
    expect(welcomePort.sent).toEqual([{ kind: "status", state: "connected" }]);
    expect(otherWindow.closed).toBe(false);

    // The welcome screen still reaches the engine (recent folders) without a workspace.
    welcomePort.send({ kind: "invoke", id: 1, command: "recent_folders", args: {} });
    await flush();
    expect([...binding.dispatches.values()].at(-1)).toMatchObject({ windowId: 1, command: { command: "recent_folders" } });
  });

  it("shutdown closes every workspace", async () => {
    const { binding, router } = setup();
    router.attach(1, "/repo", new FakePort());
    router.attach(2, "/other", new FakePort());
    router.attach(3, null, new FakePort());
    await router.shutdown();
    expect(binding.calls.filter((c) => c.startsWith("close"))).toEqual(["close 1", "close 2"]);
  });

  it("shutdown awaits queued board writes of every window before their closes", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    const a = new FakePort();
    const b = new FakePort();
    router.attach(1, "/repo", a);
    router.attach(2, "/other", b);
    a.send({ kind: "invoke", id: 1, command: "save_board", args: { board: "x" } });
    b.send({ kind: "invoke", id: 1, command: "rename_board", args: { board: "y", name: "Y" } });
    b.send({ kind: "invoke", id: 2, command: "search_text", args: { query: "q" } });
    let finished = false;
    const done = router.shutdown().then(() => {
      finished = true;
    });
    await flush();
    expect(finished).toBe(false);

    binding.openResult.resolve("{}");
    await done;
    expect(binding.order).toEqual(["dispatch 1", "dispatch 2", "close 1", "close 2"]);
    expect([...binding.dispatches.values()].map((d) => (d.command as { command: string }).command)).toEqual([
      "save_board",
      "rename_board",
    ]);
  });

  it("answers every invoke with engine_unavailable when the addon is missing", async () => {
    const unavailable = { code: "engine_unavailable", message: "The Unfold engine could not be loaded." };
    const router = new EngineRouter({ unavailable }, () => undefined);
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "graph_status", args: {} });
    await flush();
    expect(port.sent).toEqual([
      { kind: "status", state: "unavailable", error: unavailable },
      { kind: "result", id: 1, ok: false, error: unavailable },
    ]);
  });

  it("refuses main-process-only commands and anything carrying trusted from a window", async () => {
    const { binding, router } = setup();
    const port = new FakePort();
    router.attach(1, "/repo", port);
    port.send({ kind: "invoke", id: 1, command: "export_mermaid", args: { board: "b", path: "/tmp/x.mmd" } });
    port.send({ kind: "invoke", id: 2, command: "folder_windows", args: { root: "/elsewhere" } });
    port.send({ kind: "invoke", id: 3, command: "graph_status", args: {}, trusted: true });
    port.send({ kind: "invoke", id: 4, command: "graph_status", args: { trusted: true } });
    port.send({ kind: "invoke", id: 5, command: "export_mermaid_text", args: { board: "b" } });
    await flush();

    const refused = port.sent.flatMap((m) => (m.kind === "result" && !m.ok ? [[m.id, m.error.code]] : []));
    expect(refused).toEqual([
      [1, "untrusted"],
      [2, "untrusted"],
      [3, "untrusted"],
      [4, "untrusted"],
    ]);
    expect([...binding.dispatches.values()].map((d) => d.command)).toEqual([
      { command: "export_mermaid_text", args: { board: "b" } },
    ]);
  });

  it("dispatches the main process's requests with trusted, after the window's workspace opens", async () => {
    const { binding, router } = setup();
    binding.openResult = deferred<string>();
    router.attach(1, "/repo", new FakePort());
    const exported = router.trusted(1, "export_mermaid", { board: "b", path: "/out/b.mmd" });
    await flush();
    expect(binding.dispatches.size).toBe(0);
    binding.openResult.resolve("{}");
    await flush();

    const [[requestId, dispatched]] = [...binding.dispatches.entries()] as [[number, { windowId: number; command: unknown; result: Deferred<string> }]];
    expect(dispatched.windowId).toBe(1);
    expect(dispatched.command).toEqual({ command: "export_mermaid", args: { board: "b", path: "/out/b.mmd" }, trusted: true });
    dispatched.result.resolve('{"path":"/out/b.mmd","bytes":12}');
    expect(await exported).toEqual({ ok: true, value: { path: "/out/b.mmd", bytes: 12 } });

    const windows = router.trusted(0, "folder_windows", { root: "/other" });
    await flush();
    const next = binding.dispatches.get(requestId + 1);
    expect(next?.windowId).toBe(0);
    next?.result.reject(new Error('{"code":"not_a_folder","message":"/other: gone"}'));
    expect(await windows).toEqual({ ok: false, error: { code: "not_a_folder", message: "/other: gone" } });
  });

  it("answers trusted requests with engine_unavailable when the addon is missing", async () => {
    const unavailable = { code: "engine_unavailable", message: "The Unfold engine could not be loaded." };
    const router = new EngineRouter({ unavailable }, () => undefined);
    expect(await router.trusted(0, "folder_windows", { root: "/repo" })).toEqual({ ok: false, error: unavailable });
  });

  it("reports when a window's attach settles", async () => {
    const binding = new FakeBinding();
    const attached: unknown[] = [];
    const router = new EngineRouter({ binding }, () => undefined, {
      attached: (windowId, folder, error) => attached.push([windowId, folder, error]),
    });
    binding.openResult = deferred<string>();
    router.attach(1, "/repo", new FakePort());
    router.attach(2, null, new FakePort());
    await flush();
    expect(attached).toEqual([[2, null, null]]);
    binding.openResult.reject(new Error('{"code":"folder_locked","message":"Another copy has it."}'));
    await flush();
    expect(attached).toEqual([
      [2, null, null],
      [1, "/repo", { code: "folder_locked", message: "Another copy has it." }],
    ]);
  });

  it("logs and drops malformed sink messages", () => {
    const { router, logs } = setup();
    router.handleSinkMessage("not json");
    router.handleSinkMessage(JSON.stringify({ kind: "event", windows: 1, event: {} }));
    router.handleSinkMessage(JSON.stringify({ kind: "mystery" }));
    router.handleSinkMessage(JSON.stringify({ kind: "broadcast", event: "preferences_changed" }));
    expect(logs).toHaveLength(4);
  });
});

describe("errorFromRejection", () => {
  it("keeps Rust's code and message, and wraps anything else as internal", () => {
    expect(errorFromRejection(new Error('{"code":"bad_path","message":"Outside the folder."}'))).toEqual({
      code: "bad_path",
      message: "Outside the folder.",
    });
    expect(errorFromRejection(new Error("boom"))).toEqual({ code: "internal", message: "boom" });
  });
});

describe("asEngineBinding", () => {
  it("names the functions a module is missing", () => {
    expect(() => asEngineBinding({ init: () => undefined })).toThrow(
      "the addon module is missing: subscribe, openWorkspace, closeWorkspace, dispatch, cancel",
    );
    expect(() => asEngineBinding(null)).toThrow("did not export an object");
  });
});
