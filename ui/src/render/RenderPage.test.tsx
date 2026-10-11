// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import type { RenderAnswer, RenderJob } from "@unfold/desktop/render-api";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, Page, Preferences } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { getOpenBoard, getPageView } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { page as pageOf, projectRow, workspaceView } from "../test/workspace";
import { installRenderJobs, useRenderJob } from "./jobs";
import { RenderApp } from "./RenderApp";

const P = "p-orders0000";
const BOARD = { project: P, id: ".unfold/local/drafts/b.json" };
const KEY = "src/routes/orders.ts#function:createOrder";
const GONE = "src/routes/orders.ts#function:gone";

const PREFERENCES: Preferences = {
  theme: "dark",
  editorCommand: null,
  backgroundUpdates: true,
  threadLimit: null,
  keyBindings: {},
  updates: { enabled: true },
  agents: { enabled: false, port: 29180 },
};

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A ResizeObserver that reports each element once it is observed, so React Flow measures the cards (jsdom sizes every element 400 × 220). */
class MeasuringResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element): void {
    queueMicrotask(() => {
      const contentRect = { x: 0, y: 0, width: 400, height: 220 };
      this.callback([{ target, contentRect } as unknown as ResizeObserverEntry], this);
    });
  }
  unobserve(): void {
    // Each element is reported once.
  }
  disconnect(): void {
    // Each element is reported once.
  }
}

const original = globalThis.ResizeObserver;
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: MeasuringResizeObserver });
});
afterAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: original });
});

let engine: FakeEngine;
let uninstallJobs: () => void;
let nextRequest = 1;

beforeEach(() => {
  resetBoards();
  queryClient.clear();
  useRenderJob.setState({ job: null, frame: null });
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === KEY) {
          return fixture("card", "card_create_order");
        }
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
      line_labels: () => [],
      get_preferences: () => PREFERENCES,
    },
  });
  uninstallJobs = installRenderJobs();
});

afterEach(() => {
  uninstallJobs();
  engine.uninstall();
  resetBoards();
  document.documentElement.removeAttribute("data-theme");
});

function cards(): Card[] {
  return [
    { id: "create-order", ref: { block: KEY }, at: [0, 0] },
    { id: "gone", ref: { block: GONE }, at: [600, 0] },
  ];
}

function job(purpose: RenderJob["purpose"], extra: Partial<RenderJob> = {}, pageExtra: Partial<Page> = {}): RenderJob {
  const page = pageOf("page-1", cards(), pageExtra);
  return { requestId: nextRequest++, purpose, board: BOARD, title: "Order placement", page: page, health: null, ...extra };
}

/** Sends a job the way the main process does, and draws the render window. */
function draw(sent: RenderJob) {
  engine.emit({ type: "render_page", ...sent });
  return render(
    <QueryClientProvider client={queryClient}>
      <RenderApp />
    </QueryClientProvider>,
  );
}

/** The window's answer to `requestId`, once it has answered. */
function answerTo(requestId: number): Promise<RenderAnswer> {
  return waitFor(
    () => {
      const call = engine.windowCalls.find((entry) => entry.command === "render_ready" && (entry.args as { requestId: number }).requestId === requestId);
      const result = (call?.args as { result: RenderAnswer } | undefined)?.result;
      if (!result) {
        throw new Error("No answer yet.");
      }
      return result;
    },
    { timeout: 5000 },
  );
}

describe("the render window", () => {
  it("draws the page it is handed, read-only, in the person's theme, and answers with its bounds once measured", async () => {
    const sent = job("image");
    const { container } = draw(sent);
    expect(getOpenBoard(BOARD)?.readOnly).toBe(true);
    expect(await answerTo(sent.requestId)).toEqual({ ok: true, bounds: { x: 0, y: 0, width: 1000, height: 220 } });
    expect(container.querySelectorAll(".react-flow__node")).toHaveLength(2);
    expect(container.querySelector(".board-topbar, .board-dock, .board-sidebar")).toBeNull();
    expect(document.documentElement.dataset["theme"]).toBe("dark");
    // Nothing of a board that holds no claim is ever opened or saved.
    expect(engine.calls.map((call) => call.command).filter((command) => command.includes("board"))).toEqual([]);
  });

  it("takes the camera it is framed with, and still saves nothing", async () => {
    const sent = job("image");
    draw(sent);
    await answerTo(sent.requestId);
    const camera = { x: 24, y: 24, zoom: 0.5 };
    engine.emit({ type: "render_frame", requestId: 99, camera, width: 400, height: 220 });
    expect(await answerTo(99)).toEqual({ ok: true });
    expect(getPageView({ board: BOARD, page: "page-1" }).camera).toEqual(camera);
    await new Promise((resolve) => setTimeout(resolve, 1100));
    expect(engine.callsOf("save_board_view")).toEqual([]);
    expect(engine.callsOf("save_board")).toEqual([]);
  });

  it("answers with the page checks of the page as drawn", async () => {
    const sent = job("check");
    draw(sent);
    const result = await answerTo(sent.requestId);
    expect(result).toEqual({
      ok: true,
      checks: [
        {
          kind: "strays",
          items: ["create-order", "gone"],
          message: "createOrder and gone have no line, connection, or frame to the rest of the page.",
          suggestion: "connect them, put them in a frame (frame_cards), or take them off the page (remove_items).",
        },
      ],
    });
  });

  it("frames one item, and refuses an item that isn't drawn", async () => {
    const card = job("image", { item: "gone" });
    draw(card);
    expect(await answerTo(card.requestId)).toEqual({ ok: true, bounds: { x: 600, y: 0, width: 400, height: 220 } });

    const frames = [{ id: "api", title: "API", members: ["gone"], look: "box" as const, collapsed: true, author: "agent:claude" }];
    const hidden = job("image", { item: "gone" }, { frames });
    engine.emit({ type: "render_page", ...hidden });
    expect(await answerTo(hidden.requestId)).toEqual({
      ok: false,
      code: "item_hidden",
      message: "`gone` is inside the collapsed frame `api`. Take the frame's image, or expand the frame first.",
    });

    const missing = job("image", { item: "nowhere" });
    engine.emit({ type: "render_page", ...missing });
    expect(await answerTo(missing.requestId)).toMatchObject({ ok: false, code: "item_not_found" });
  });
});
