// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SETTLE_MS, WRITING_IDLE_MS, finishWorking, markWorking, resetWorking, useItemWriting, useWorkingAgents, useWorkingItems } from "./working";

const REF = { project: "p-orders0000", id: ".unfold/boards/b.json" };
const TARGET = { board: REF, page: "page-1" };
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

beforeEach(() => {
  vi.useFakeTimers();
  resetWorking();
});

afterEach(() => {
  resetWorking();
  vi.useRealTimers();
});

describe("working marks", () => {
  it("hold what each write wrote, keeping an item's earlier fields, and when it landed", () => {
    const { result } = renderHook(() => useItemWriting(TARGET, "create"));
    act(() => {
      markWorking(TARGET, CLAUDE, [{ id: "create", fields: ["description"] }], 1000);
      markWorking(TARGET, CLAUDE, [{ id: "create", fields: ["title"] }], 2000);
    });
    expect(result.current).toEqual({ agent: "Claude", author: "agent:claude", fields: ["description", "title"], at: 2000, settling: false });
  });

  it("settle two minutes after an item's last write: the item fades for 300 ms, while the agent still works", () => {
    const { result } = renderHook(() => ({ writing: useItemWriting(TARGET, "create"), items: useWorkingItems(TARGET), agents: useWorkingAgents(TARGET) }));
    act(() => {
      markWorking(TARGET, CLAUDE, [{ id: "create", fields: ["description"] }]);
    });
    act(() => {
      vi.advanceTimersByTime(WRITING_IDLE_MS - 1);
    });
    expect(result.current.writing?.settling).toBe(false);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.writing?.settling).toBe(true);
    expect([...result.current.items]).toEqual([]);
    act(() => {
      vi.advanceTimersByTime(SETTLE_MS);
    });
    expect(result.current.writing).toBeUndefined();
    expect(result.current.agents).toEqual(["Claude"]);
  });

  it("settle when the agent finishes: it stops working, and what it wrote fades out", () => {
    const { result } = renderHook(() => ({ writing: useItemWriting(TARGET, "note"), agents: useWorkingAgents(TARGET) }));
    act(() => {
      markWorking(TARGET, CLAUDE, [{ id: "note", fields: ["text"] }]);
      finishWorking(REF, "agent:claude");
    });
    expect(result.current.agents).toEqual([]);
    expect(result.current.writing?.settling).toBe(true);
    act(() => {
      vi.advanceTimersByTime(SETTLE_MS);
    });
    expect(result.current.writing).toBeUndefined();
  });
});
