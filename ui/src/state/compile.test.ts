// @vitest-environment jsdom
import "../test/dom";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { cancelCompile, installCompileEvents, resetCompile, startCompile, useCompileProgress, useCompileStartedHere } from "./compile";

const P = "p-orders0000";
const Q = "p-payments00";

let engine: FakeEngine;

beforeEach(() => {
  resetCompile();
});

afterEach(() => {
  engine.uninstall();
});

function hooks(project: string) {
  return renderHook(() => ({ progress: useCompileProgress(project), here: useCompileStartedHere(project) }));
}

describe("compile per project", () => {
  it("streams progress for its project, runs once per project, and treats cancelling as no error", async () => {
    let send: (message: unknown) => void = () => undefined;
    engine = installFakeEngine({
      handlers: { cancel_compile: () => ({ cancelled: true }) },
      streams: {
        compile: (_args, message, signal) => {
          send = message;
          return new Promise((_resolve, reject) => {
            signal.addEventListener("abort", () => {
              // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- the transport rejects with plain data
              reject({ code: "cancelled", message: "Cancelled." });
            });
          });
        },
      },
    });
    const p = hooks(P);
    const q = hooks(Q);
    let run: Promise<void> = Promise.resolve();
    act(() => {
      run = startCompile(P);
    });
    void startCompile(P);
    expect(engine.callsOf("compile")).toEqual([{ project: P, mode: "check" }]);
    act(() => {
      send({ stage: "extract", done: 3, total: 10 });
    });
    expect(p.result.current).toEqual({ progress: { stage: "extract", done: 3, total: 10 }, here: true });
    expect(q.result.current).toEqual({ progress: null, here: false });
    await act(async () => {
      await cancelCompile(P);
      await run;
    });
    expect(engine.callsOf("cancel_compile")).toEqual([{ project: P }]);
    expect(p.result.current.here).toBe(false);
  });

  it("follows progress from any window until the project settles", () => {
    engine = installFakeEngine();
    const stop = installCompileEvents();
    const q = hooks(Q);
    // Nothing is left over from the test before (its Compile of P reported progress).
    expect(hooks(P).result.current).toEqual({ progress: null, here: false });
    act(() => {
      engine.emit({ type: "compile_progress", project: Q, stage: "snapshot", done: 1, total: 4 });
    });
    expect(q.result.current.progress).toEqual({ stage: "snapshot", done: 1, total: 4 });
    act(() => {
      engine.emit({ type: "project_state", project: Q, state: { kind: "updating", files: 2 } });
    });
    expect(q.result.current.progress).not.toBeNull();
    act(() => {
      engine.emit({ type: "project_state", project: Q, state: { kind: "current" } });
    });
    expect(q.result.current.progress).toBeNull();
    stop();
  });
});
