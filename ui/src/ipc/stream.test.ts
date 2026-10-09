// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import type { TextSearchFile } from "./generated";
import { createLatestStream } from "./stream";

let engine: FakeEngine;

afterEach(() => {
  engine.uninstall();
});

const summary = { files: 1, matches: 1, searched: 1, truncated: false, filesTruncated: false, projects: [] };

function file(path: string): TextSearchFile {
  return { project: "p-orders0000", path, matches: [] };
}

describe("createLatestStream", () => {
  it("cancels the running request when a new one starts, and drops its late messages and result", async () => {
    const sends: ((message: unknown) => void)[] = [];
    const finishes: (() => void)[] = [];
    const cancelled: boolean[] = [];
    engine = installFakeEngine({
      streams: {
        search_text: (_args, send, signal) => {
          const index = sends.length;
          sends.push(send);
          cancelled.push(false);
          signal.addEventListener("abort", () => {
            cancelled[index] = true;
          });
          return new Promise((resolve) => {
            finishes.push(() => {
              resolve(summary);
            });
          });
        },
      },
    });
    const search = createLatestStream("search_text");
    const first = vi.fn();
    const second = vi.fn();
    const firstRun = search.run({ query: "or" }, first);
    const secondRun = search.run({ query: "ord" }, second);
    expect(cancelled).toEqual([true, false]);
    expect(engine.callsOf("search_text")).toEqual([{ query: "or" }, { query: "ord" }]);

    // The first request answers late anyway: nothing of it gets through.
    sends[0]?.(file("late.ts"));
    finishes[0]?.();
    await expect(firstRun).rejects.toMatchObject({ code: "cancelled" });
    expect(first).not.toHaveBeenCalled();

    sends[1]?.(file("src/orders.ts"));
    finishes[1]?.();
    await expect(secondRun).resolves.toEqual(summary);
    expect(second).toHaveBeenCalledWith(file("src/orders.ts"));
  });

  it("cancel stops the running request and drops what follows", async () => {
    let send: (message: unknown) => void = () => undefined;
    let finish: () => void = () => undefined;
    engine = installFakeEngine({
      streams: {
        search_text: (_args, sendMessage) => {
          send = sendMessage;
          return new Promise((resolve) => {
            finish = () => {
              resolve(summary);
            };
          });
        },
      },
    });
    const search = createLatestStream("search_text");
    const onMessage = vi.fn();
    const run = search.run({ query: "order" }, onMessage);
    search.cancel();
    send(file("late.ts"));
    finish();
    await expect(run).rejects.toMatchObject({ code: "cancelled" });
    expect(onMessage).not.toHaveBeenCalled();
  });
});
