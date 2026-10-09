import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { debouncedSave, placeOnScreen, savedWindowList } from "./window-placement";

const primary = { x: 0, y: 25, width: 1440, height: 875 };
const right = { x: 1440, y: 0, width: 1920, height: 1080 };

describe("placeOnScreen", () => {
  it("keeps bounds that a screen shows", () => {
    const bounds = { x: 100, y: 100, width: 1280, height: 820 };
    expect(placeOnScreen(bounds, [primary])).toEqual(bounds);
    const onSecond = { x: 2000, y: 200, width: 1000, height: 700 };
    expect(placeOnScreen(onSecond, [primary, right])).toEqual(onSecond);
  });

  it("keeps a window whose title bar is partly on a screen", () => {
    const bounds = { x: 1300, y: 300, width: 1000, height: 700 };
    expect(placeOnScreen(bounds, [primary])).toEqual(bounds);
  });

  it("moves a window from a screen that is gone onto the primary screen", () => {
    const onSecond = { x: 2000, y: 200, width: 1000, height: 700 };
    expect(placeOnScreen(onSecond, [primary])).toEqual({ x: 220, y: 113, width: 1000, height: 700 });
  });

  it("moves a window whose title bar is above every screen, shrinking it to fit", () => {
    const bounds = { x: 100, y: -2000, width: 3000, height: 2000 };
    expect(placeOnScreen(bounds, [primary])).toEqual({ x: 0, y: 25, width: 1440, height: 875 });
  });

  it("leaves bounds alone when no screens are known", () => {
    const bounds = { x: 5000, y: 5000, width: 800, height: 600 };
    expect(placeOnScreen(bounds, [])).toEqual(bounds);
  });
});

describe("savedWindowList", () => {
  it("reads saved windows and skips malformed ones", () => {
    const result = {
      windows: [
        { key: "a", bounds: { x: 10.4, y: 20, width: 800, height: 600 }, maximized: true, selectedProject: null, tabs: [], activeTab: null, layout: {} },
        { key: "b", bounds: null, maximized: false },
        { key: "c", bounds: { x: 0, y: 0, width: 0, height: 600 } },
        { key: "", bounds: null },
        { bounds: null },
        "nope",
      ],
    };
    expect(savedWindowList(result)).toEqual([
      { key: "a", bounds: { x: 10, y: 20, width: 800, height: 600 }, maximized: true },
      { key: "b", bounds: null, maximized: false },
      { key: "c", bounds: null, maximized: false },
    ]);
    expect(savedWindowList(null)).toEqual([]);
    expect(savedWindowList({ windows: "x" })).toEqual([]);
  });
});

describe("debouncedSave", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("saves once, after the last change settles", () => {
    const save = vi.fn();
    const placement = debouncedSave(save, 500);
    placement.schedule();
    vi.advanceTimersByTime(300);
    placement.schedule();
    vi.advanceTimersByTime(300);
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(200);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("flushes a waiting save immediately, so a window closing keeps its last resize", () => {
    const save = vi.fn();
    const placement = debouncedSave(save, 500);
    placement.schedule();
    placement.flush();
    expect(save).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1_000);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("flushes nothing when no save waits, and drops a cancelled one", () => {
    const save = vi.fn();
    const placement = debouncedSave(save, 500);
    placement.flush();
    placement.schedule();
    vi.advanceTimersByTime(600);
    placement.flush();
    expect(save).toHaveBeenCalledTimes(1);
    placement.schedule();
    placement.cancel();
    placement.flush();
    vi.advanceTimersByTime(600);
    expect(save).toHaveBeenCalledTimes(1);
  });
});
