// @vitest-environment jsdom
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { TextSearchArgs } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { useTextSearch } from "./textSearch";

let engine: FakeEngine;
let cancelled: string[];

beforeEach(() => {
  cancelled = [];
  engine = installFakeEngine({
    streams: {
      search_text: (args, send, signal) => {
        const query = String(args["query"]);
        send({ project: "p-a", path: `src/${query}.ts`, matches: [{ line: 1, text: query, column: 0, ranges: [[0, query.length]] }] });
        if (query === "slow") {
          // Ends only when cancelled, after one more (late) message.
          return new Promise((_resolve, reject) => {
            signal.addEventListener("abort", () => {
              cancelled.push(query);
              send({ project: "p-a", path: "src/late.ts", matches: [] });
              reject(Object.assign(new Error("cancelled"), { code: "cancelled" }));
            });
          });
        }
        return Promise.resolve({ files: 1, matches: 1, searched: 3, truncated: false, filesTruncated: false, projects: [] });
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
});

describe("useTextSearch", () => {
  it("streams one search, and a new query cancels the running one and drops its late messages", async () => {
    const initialProps: { args: TextSearchArgs | null } = { args: { query: "slow" } };
    const { result, rerender } = renderHook(({ args }) => useTextSearch(args), { initialProps });
    await waitFor(() => {
      expect(result.current.files.map((file) => file.path)).toEqual(["src/slow.ts"]);
    });
    expect(result.current.running).toBe(true);

    rerender({ args: { query: "refund", caseSensitive: true } });
    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });
    expect(cancelled).toEqual(["slow"]);
    expect(result.current.files.map((file) => file.path)).toEqual(["src/refund.ts"]);
    expect(result.current.matches).toBe(1);
    expect(engine.callsOf("search_text")).toEqual([{ query: "slow" }, { query: "refund", caseSensitive: true }]);

    rerender({ args: null });
    expect(result.current).toMatchObject({ files: [], running: false });
  });

  it("runs again when a project's graph changes", async () => {
    const { result } = renderHook(() => useTextSearch({ query: "refund" }));
    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });
    act(() => {
      engine.emit({ type: "graph_updated", project: "p-a", everything: true, changedFiles: [], removedFiles: [], changedKeys: [], changedEntrances: [], inputsChanged: false });
    });
    await waitFor(() => {
      expect(engine.callsOf("search_text")).toHaveLength(2);
    });
  });
});
