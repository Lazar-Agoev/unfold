import type { RenderJob } from "@unfold/desktop/render-api";
import { describe, expect, it } from "vitest";
import type { Health } from "../ipc/generated";
import { page } from "../test/workspace";
import { jobState, measureCamera } from "./jobs";

describe("measureCamera", () => {
  it("keeps a small page at zoom 1, with room around it", () => {
    expect(measureCamera(page("p", [{ id: "a", ref: { block: "k" }, at: [100, 50] }]), 1600, 1000)).toEqual({ x: 400, y: 400, zoom: 1 });
  });

  it("zooms out until a large page's positions are in view, never below 5%", () => {
    const wide = page("p", [
      { id: "a", ref: { block: "k" }, at: [-1000, 0] },
      { id: "b", ref: { block: "k" }, at: [9000, 200] },
    ]);
    const camera = measureCamera(wide, 1200, 800);
    expect(camera.zoom).toBeCloseTo(1200 / 10800);
    expect(camera.x).toBeCloseTo(1400 * camera.zoom);
    expect(measureCamera(page("p", [{ id: "far", ref: { block: "k" }, at: [1e7, 0] }]), 1200, 800).zoom).toBe(0.05);
  });
});

describe("jobState", () => {
  const job: RenderJob = {
    requestId: 1,
    purpose: "image",
    board: { project: "p-orders0000", id: ".unfold/boards/b.json" },
    title: "Order placement",
    page: page("page-2", [{ id: "a", ref: { block: "k" }, at: [0, 0] }]),
    health: null,
  };

  it("is a board of the one page, with no health", () => {
    const state = jobState(job);
    expect(state.document).toEqual({ format: "unfold-board", version: 1, title: "Order placement", pages: [job.page] });
    expect(state.health.items).toEqual([]);
  });

  it("carries the board's health for the review banners", () => {
    const health: Health = { items: [], missing: 1, outdated: 0, brokenGroups: 0, review: 0, brokenLinks: 0 };
    expect(jobState({ ...job, health }).health).toBe(health);
  });
});
