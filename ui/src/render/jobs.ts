/**
 * What the render window is asked to draw: the main process's `render_page` (a page, handed whole)
 * and `render_frame` (the camera to capture it with), and the answers (`render_ready`). Each page is
 * adopted as a read-only board session, so the canvas and the cards draw it exactly as the board view
 * would, and nothing of it is ever saved.
 */

import type { RenderAnswer, RenderFrame, RenderJob } from "@unfold/desktop/render-api";
import { create } from "zustand";
import { onShellEvent, windowCommand } from "../ipc";
import type { BoardState, Health, Page } from "../ipc/generated";
import { adoptBoardState, forgetBoard, setCamera } from "../state/boards/store";
import type { Camera } from "../state/boards";

interface RenderState {
  /** The page being drawn, if any. */
  readonly job: RenderJob | null;
  /** The camera to capture it with, once the main process sized the window for it. */
  readonly frame: RenderFrame | null;
}

export const useRenderJob = create<RenderState>(() => ({ job: null, frame: null }));

const NO_HEALTH: Health = { items: [], missing: 0, outdated: 0, review: 0, brokenCombined: 0, brokenLinks: 0 };

/** The lowest zoom the page is measured at: everything on a large page is drawn, so everything is measured. */
const MEASURE_MIN_ZOOM = 0.05;
/** Room left around the page's items while it is measured, in canvas units. */
const MEASURE_MARGIN = 400;

/**
 * The camera a page is measured with: its items' positions in view of a window `width` × `height`,
 * at zoom 1 when they fit. (Cards are measured unscaled; only a page of over 40 cards draws just the
 * cards in view.)
 */
export function measureCamera(page: Page, width: number, height: number): Camera {
  const points = [...page.cards, ...(page.resources ?? []), ...(page.notes ?? [])].map((item) => item.at);
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  const left = Math.min(0, ...xs) - MEASURE_MARGIN;
  const top = Math.min(0, ...ys) - MEASURE_MARGIN;
  const spanX = Math.max(0, ...xs) + MEASURE_MARGIN - left;
  const spanY = Math.max(0, ...ys) + MEASURE_MARGIN - top;
  const zoom = Math.max(MEASURE_MIN_ZOOM, Math.min(1, width / spanX, height / spanY));
  return { x: -left * zoom, y: -top * zoom, zoom };
}

/** The board session a job is drawn from: its one page and its health. */
export function jobState(job: RenderJob): BoardState {
  const page = job.page as unknown as Page;
  return {
    project: job.board.project,
    id: job.board.id,
    name: "",
    document: { format: "unfold-board", version: 1, title: job.title, pages: [page] },
    revision: "",
    health: (job.health as Health | null) ?? NO_HEALTH,
    label: null,
  };
}

/** Takes a job: the page before it is forgotten, and this one adopted read-only, with the measuring camera. */
function receiveJob(job: RenderJob): void {
  const previous = useRenderJob.getState().job;
  if (previous) {
    forgetBoard(previous.board);
  }
  const board = adoptBoardState(jobState(job), { readOnly: true });
  const page = job.page as unknown as Page;
  setCamera({ board, page: page.id }, measureCamera(page, window.innerWidth, window.innerHeight));
  useRenderJob.setState({ job, frame: null });
}

/** Follows the main process's requests. Returns an unsubscribe function. */
export function installRenderJobs(): () => void {
  const offPage = onShellEvent("render_page", ({ type: _, ...job }) => {
    receiveJob(job);
  });
  const offFrame = onShellEvent("render_frame", ({ type: _, ...frame }) => {
    useRenderJob.setState({ frame });
  });
  return () => {
    offPage();
    offFrame();
  };
}

/** Answers a `render_page` or `render_frame`. */
export function answer(requestId: number, result: RenderAnswer): void {
  windowCommand("render_ready", { requestId, result }).catch((error: unknown) => {
    console.warn("Answering the main process failed:", error);
  });
}
