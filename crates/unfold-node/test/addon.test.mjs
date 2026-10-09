// Tests of the built addon through its JavaScript surface (contract §3.1): the dispatch round
// trip, broadcast events and stream messages from Rust threads reaching the sink, trusted
// commands, cancelling, errors, detaching windows, and shutting down. Build the addon first
// (`just addon`); the tests run in one process because the addon holds one process-wide host.

import assert from "node:assert/strict";
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
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
  let project;
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

  async function trusted(command, args = {}) {
    return JSON.parse(await addon.dispatch(0, nextRequest++, JSON.stringify({ command, args, trusted: true })));
  }

  /** Whether `message` is a broadcast event of `type` about the test's project. */
  const broadcast = (message, type) =>
    message.kind === "broadcast" && message.event.type === type && message.event.project === project;

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
    assert.deepEqual(functions.sort(), ["cancel", "detachWindow", "dispatch", "init", "shutdown", "subscribe"]);
  });

  it("throws the error JSON when initialized twice", () => {
    const options = JSON.stringify({ appDataDir: scratch, logsDir: scratch, fallbackCacheDir: scratch });
    assert.throws(() => addon.init(options), (error) => JSON.parse(error.message).code === "already_initialized");
  });

  it("rejects malformed commands and unknown projects", async () => {
    assert.equal((await rejection(addon.dispatch(9, nextRequest++, "not json"))).code, "invalid_command");
    assert.equal((await rejection(addon.dispatch(9, nextRequest++, '{"command":"nope"}'))).code, "invalid_command");
    assert.equal((await rejection(addon.dispatch(9, -1, '{"command":"workspace"}'))).code, "invalid_args");
    const unknown = await rejection(dispatch(9, "project_summary", { project: "p-nonenonen" }));
    assert.equal(unknown.code, "unknown_project");
    assert.equal(typeof unknown.message, "string");
  });

  it("refuses trusted-only commands without trusted, and runs them with it", async () => {
    assert.equal((await rejection(dispatch(1, "add_project", { path: folder }))).code, "untrusted");
    const added = await trusted("add_project", { path: folder });
    assert.equal(added.status, "added");
    assert.equal(added.project.name, "ts-orders");
    project = added.project.id;
    const changed = await sinkMessage((m) => m.kind === "broadcast" && m.event.type === "workspace_changed");
    assert.equal(changed.event.workspace.projects[0].id, project);
    assert.deepEqual((await trusted("saved_windows")).windows, []);
  });

  it("wakes a selected project, and its events reach the sink from Rust threads", async () => {
    assert.equal(await dispatch(1, "select_project", { project }), null);
    const state = await sinkMessage((m) => broadcast(m, "project_state"));
    assert.equal(state.event.state.kind, "not_compiled");
    const workspace = await dispatch(2, "workspace");
    assert.equal(workspace.projects[0].live, true);
  });

  it("compiles with streamed progress and a graph_updated broadcast, then serves entrances and cards", async () => {
    const requestId = nextRequest++;
    const outcome = JSON.parse(
      await addon.dispatch(1, requestId, JSON.stringify({ command: "compile", args: { project } })),
    );
    assert.equal(typeof outcome, "object");
    await sinkMessage((m) => m.kind === "stream" && m.windowId === 1 && m.requestId === requestId);
    await sinkMessage((m) => broadcast(m, "graph_updated"));

    const map = await dispatch(2, "entrances", { project });
    const entrance = [...map.applications.flatMap((app) => app.entrances), ...map.unreached.entrances][0];
    assert.ok(entrance, "ts-orders has entrances");
    const card = await dispatch(2, "card", { project, key: entrance.key });
    assert.equal(typeof card.type, "string");
  });

  it("cancels a running command", async () => {
    const requestId = nextRequest++;
    const running = addon.dispatch(
      1,
      requestId,
      JSON.stringify({ command: "compile", args: { project, mode: "from_scratch" } }),
    );
    addon.cancel(requestId);
    assert.equal((await rejection(running)).code, "cancelled");
    addon.cancel(requestId); // Finished: a no-op.
    addon.cancel(-5); // Not a request id: ignored.
    assert.equal(typeof (await dispatch(1, "project_summary", { project })), "object");
  });

  it("detaches windows, waiting for their closing writes", async () => {
    const state = { key: "w1", selectedProject: project, tabs: [], activeTab: null, layout: {} };
    const saving = dispatch(1, "save_window_state", { state });
    assert.equal(await addon.detachWindow(1), undefined);
    assert.equal(await saving, null);
    assert.equal((await dispatch(3, "window_state", { key: "w1" })).selectedProject, project);
    assert.equal(await addon.detachWindow(2), undefined);
    assert.equal(await addon.detachWindow(42), undefined, "an unknown window is fine");
  });

  it("shuts down, remembering each project's state", async () => {
    assert.equal(await addon.shutdown(), undefined);
    const saved = JSON.parse(readFileSync(join(scratch, "app-data", "workspace.json"), "utf8"));
    assert.equal(saved.projects[0].lastKnown.state.kind, "current");
    assert.equal((await rejection(dispatch(1, "project_summary", { project }))).code, "closed");
  });
});
