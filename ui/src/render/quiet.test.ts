import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { whenQuiet } from "./quiet";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("whenQuiet", () => {
  it("resolves once the signature has stayed the same for the quiet time", async () => {
    const signatures = ["a", "b", "b", "b", "b", "b", "b", "b", "b", "b"];
    const probe = vi.fn(() => signatures.shift() ?? "b");
    const settled = whenQuiet(probe, { quietMs: 100, timeoutMs: 5000, signal: new AbortController().signal });
    await vi.advanceTimersByTimeAsync(200);
    expect(await settled).toBe(true);
    // "a", then "b" from 40 ms: quiet from 40 ms to 160 ms.
    expect(probe).toHaveBeenCalledTimes(5);
  });

  it("never counts a page that isn't ready as quiet, and gives up after the timeout", async () => {
    const settled = whenQuiet(() => null, { quietMs: 0, timeoutMs: 500, signal: new AbortController().signal });
    await vi.advanceTimersByTimeAsync(600);
    expect(await settled).toBe(false);
  });

  it("stops when aborted", async () => {
    const controller = new AbortController();
    const settled = whenQuiet(() => null, { quietMs: 0, timeoutMs: 5000, signal: controller.signal });
    controller.abort();
    await vi.advanceTimersByTimeAsync(40);
    expect(await settled).toBe(false);
  });
});
