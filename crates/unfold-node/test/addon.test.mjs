// Tests of the built addon through its JavaScript surface (brief §6): the dispatch round trip,
// events and stream messages from Rust threads reaching the sink, cancelling, errors, and opening
// and closing workspaces. Build the addon first (`just addon`); the tests run in one process
// because the addon holds one process-wide host.

import assert from "node:assert/strict";
import { cpSync, existsSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const crateDir = dirname(dirname(fileURLToPath(import.meta.url)));
const playground = join(crateDir, "..", "..", "testdata", "playgrounds", "ts-orders");
const built = readdirSync(crateDir).some((name) => name.endsWith(".node")) && existsSync(join(crateDir, "index.js"));

/** Rejections carry JSON {code, message} as the Error's message. */
async function rejection(promise) {
  try {
    await promise;
  } catch (error) {
    assert.ok(error instanceof Error, "rejects with an Error");
    return JSON.parse(error.message);
  }
  assert.fail("expected a rejection");
}

describe("unfold-node addon", { skip: built ? false : "the addon is not built: run `just addon`" }, () => {
  const addon = built ? createRequire(import.meta.url)(crateDir) : null;
  const messages = [];
  const waiters = [];
  let scratch;
  let folder;
  let nextRequest = 1;

  /** Resolves with the first sink message (past or future) matching `predicate`. */
  function sinkMessage(predicate, timeoutMs = 30_000) {
    const seen = messages.find(predicate);
    if (seen) {
      return Promise.resolve(seen);
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("no matching sink message")), timeoutMs);
      timer.unref();
      waiters.push({
        predicate,
        resolve: (message) => {
          clearTimeout(timer);
          resolve(message);
        },
      });
    });
  }

  async function dispatch(windowId, command, args = {}) {
    return JSON.parse(await addon.dispatch(windowId, nextRequest++, JSON.stringify({ command, args })));
  }

  before(() => {
    scratch = mkdtempSync(join(tmpdir(), "unfold-node-test-"));
    folder = join(scratch, "ts-orders");
    cpSync(playground, folder, { recursive: true });
    addon.init(
      JSON.stringify({
        appDataDir: join(scratch, "app-data"),
        logsDir: join(scratch, "logs"),
        fallbackCacheDir: join(scratch, "caches"),
      }),
    );
    addon.subscribe((text) => {
      const message = JSON.parse(text);
      messages.push(message);
      for (const waiter of waiters.splice(0)) {
        if (waiter.predicate(message)) {
          waiter.resolve(message);
        } else {
          waiters.push(waiter);
        }
      }
    });
  });

  after(() => {
    rmSync(scratch, { recursive: true, force: true });
  });

  it("exports exactly the transport functions", () => {
    const functions = Object.keys(addon).filter((name) => typeof addon[name] === "function");
    assert.deepEqual(functions.sort(), [
      "cancel",
      "closeWorkspace",
      "dispatch",
      "init",
      "openWorkspace",
      "subscribe",
    ]);
  });

  it("throws the error JSON when initialized twice", () => {
    const options = JSON.stringify({ appDataDir: scratch, logsDir: scratch, fallbackCacheDir: scratch });
    assert.throws(() => addon.init(options), (error) => JSON.parse(error.message).code === "already_initialized");
  });

  it("rejects malformed commands and commands for windows without a folder", async () => {
    assert.equal((await rejection(addon.dispatch(9, nextRequest++, "not json"))).code, "invalid_command");
    assert.equal((await rejection(addon.dispatch(9, nextRequest++, '{"command":"nope"}'))).code, "invalid_command");
    assert.equal((await rejection(addon.dispatch(9, -1, '{"command":"graph_status"}'))).code, "invalid_args");
    const notAttached = await rejection(dispatch(9, "graph_status"));
    assert.equal(notAttached.code, "not_attached");
    assert.equal(typeof notAttached.message, "string");
  });

  it("refuses trusted-only commands without trusted, and runs them with it", async () => {
    assert.equal((await rejection(dispatch(0, "folder_windows", { root: folder }))).code, "untrusted");
    const windows = JSON.parse(
      await addon.dispatch(0, nextRequest++, JSON.stringify({ command: "folder_windows", args: { root: folder }, trusted: true })),
    );
    assert.deepEqual(windows.windows, []);
  });

  it("opens a workspace, and its event reaches the sink from a Rust thread", async () => {
    const opened = JSON.parse(await addon.openWorkspace(1, folder));
    assert.equal(opened.name, "ts-orders");
    const event = await sinkMessage((m) => m.kind === "event" && m.event.type === "folder_opened");
    assert.deepEqual(event.windows, [1]);
  });

  it("compiles with streamed progress and a graph_updated event, then serves entrances and cards", async () => {
    const requestId = nextRequest++;
    const outcome = JSON.parse(await addon.dispatch(1, requestId, JSON.stringify({ command: "compile", args: {} })));
    assert.equal(typeof outcome, "object");
    await sinkMessage((m) => m.kind === "stream" && m.windowId === 1 && m.requestId === requestId);
    const updated = await sinkMessage((m) => m.kind === "event" && m.event.type === "graph_updated");
    assert.deepEqual(updated.windows, [1]);

    const tree = await dispatch(1, "entrance_tree");
    const entrance = [...tree.applications.flatMap((app) => app.entrances), ...tree.unreached][0];
    assert.ok(entrance, "ts-orders has entrances");
    const card = await dispatch(1, "card", { key: entrance.key });
    assert.equal(typeof card.type, "string");
  });

  it("sends a folder's events to every window attached to it", async () => {
    await addon.openWorkspace(2, folder);
    const before = messages.length;
    await dispatch(2, "compile", { mode: "from_scratch" });
    const updated = await sinkMessage((m) => messages.indexOf(m) >= before && m.kind === "event" && m.event.type === "graph_updated");
    assert.deepEqual([...updated.windows].sort(), [1, 2]);
  });

  it("cancels a running command", async () => {
    const requestId = nextRequest++;
    const running = addon.dispatch(1, requestId, JSON.stringify({ command: "compile", args: { mode: "from_scratch" } }));
    addon.cancel(requestId);
    assert.equal((await rejection(running)).code, "cancelled");
    addon.cancel(requestId); // Finished: a no-op.
    addon.cancel(-5); // Not a request id: ignored.
    assert.equal(typeof (await dispatch(1, "graph_status")), "object");
  });

  it("closes workspaces", async () => {
    assert.equal(await addon.closeWorkspace(2), undefined);
    assert.equal(await addon.closeWorkspace(1), undefined);
    assert.equal((await rejection(dispatch(1, "graph_status"))).code, "not_attached");
  });
});
