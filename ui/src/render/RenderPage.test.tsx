// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import type { RenderAnswer, RenderJob } from "@unfold/desktop/render-api";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, Page } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { getOpenBoard, getPageView } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { getShowAllCalls, setShowAllCalls } from "../state/showAllCalls";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { page as pageOf, projectRow, workspaceView } from "../test/workspace";
import { installRenderJobs, useRenderJob } from "./jobs";
import { RenderApp } from "./RenderApp";

const P = "p-orders0000";
const BOARD = { project: P, id: ".unfold/boards/b.json" };
const KEY = "src/routes/orders.ts#createOrder";
const GONE = 'src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]';
const START = "entrance:orders-api/place-an-order";

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
        if (key === START) {
          const code = [{ key: KEY, state: "found" as const, label: "createOrder", path: "src/routes/orders.ts", line: 3 }];
          return { type: "start", key: START, kind: "entrance", app: "Orders API", name: "Place an order", on: "POST /orders", code };
        }
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
    },
  });
  uninstallJobs = installRenderJobs();
});

afterEach(() => {
  uninstallJobs();
  engine.uninstall();
  resetBoards();
});

function cards(): Card[] {
  return [
    { id: "create-order", ref: { block: KEY }, at: [0, 0] },
    { id: "gone", ref: { block: GONE }, at: [600, 0] },
  ];
}

function job(purpose: RenderJob["purpose"], extra: Partial<RenderJob> = {}, pageExtra: Partial<Page> = {}): RenderJob {
  const page = pageOf("page-1", cards(), pageExtra);
  return { requestId: nextRequest++, purpose, board: BOARD, title: "Order placement", page: page, health: null, showAllCalls: false, ...extra };
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
  it("draws with Show all calls as the person's window shows it: builtin calls too when it is on", async () => {
    setShowAllCalls(false);
    const sent = job("image", { showAllCalls: true });
    draw(sent);
    expect(getShowAllCalls()).toBe(true);
    await answerTo(sent.requestId);
    expect(engine.callsOf("card").every((args) => args["showAllCalls"] === true)).toBe(true);
    const off = job("check", { showAllCalls: false });
    engine.emit({ type: "render_page", ...off });
    expect(getShowAllCalls()).toBe(false);
  });

  it("draws the page it is handed, read-only, and answers with its bounds once measured", async () => {
    const sent = job("image");
    const { container } = draw(sent);
    expect(getOpenBoard(BOARD)?.readOnly).toBe(true);
    expect(await answerTo(sent.requestId)).toEqual({ ok: true, bounds: { x: 0, y: 0, width: 1000, height: 220 } });
    expect(container.querySelectorAll(".react-flow__node")).toHaveLength(2);
    expect(container.querySelector(".board-topbar, .board-dock, .board-sidebar")).toBeNull();
    // Nothing of a board the engine doesn't hold open is ever opened or saved.
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
          message: "createOrder and gone have no line to the rest of the page.",
          suggestion:
            "Add the code that joins it (add_cards with follow: the code an entrance starts, a caller, or a callee), connect a store or an outside service with a cited connection, or take it off the page (remove_items).",
        },
      ],
    });
  });

  it("checks the resource cards and connections it draws: strays, evidence, and missing words", async () => {
    const sent = job("check", {}, {
      resources: [
        { id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 400], author: "agent:claude" },
        { id: "sessions", kind: "cache", name: "sessions", anchors: [{ key: KEY }], at: [1200, 400], author: "agent:claude", description: { text: "Who is signed in.", author: "agent:claude", signature: "" } },
      ],
      links: [{ id: "writes", from: "create-order", to: "orders", kind: "writes", author: "agent:claude" }],
    });
    const { container } = draw(sent);
    const result = await answerTo(sent.requestId);
    expect(container.querySelector('[data-resource-id="orders"]')).not.toBeNull();
    expect(container.querySelector('.react-flow__edge[data-id="writes"]')).not.toBeNull();
    expect(result.ok && "checks" in result ? result.checks.map((check) => [check.kind, check.items]) : result).toEqual([
      ["missingWords", ["orders"]],
      ["evidence", ["orders", "writes"]],
      ["strays", ["gone", "sessions"]],
    ]);
  });

  it("counts the start arrow it draws from an entrance's card to its code: neither card is a stray", async () => {
    const sent = job("check", {}, { cards: [...cards(), { id: "place-an-order", ref: { entrance: START }, at: [0, 400] }] });
    const { container } = draw(sent);
    const result = await answerTo(sent.requestId);
    expect(container.querySelector('.react-flow__edge[data-id^="place-an-order|"]')).not.toBeNull();
    expect(result.ok && "checks" in result ? result.checks.map((check) => [check.kind, check.items]) : result).toEqual([["strays", ["gone"]]]);
  });

  it("answers for a page whose collapsed frame holds every card, drawn as one node", async () => {
    const frames = [{ id: "api", title: "API", members: ["create-order", "gone"], look: "box" as const, collapsed: true, author: "agent:claude" }];
    const sent = job("check", {}, { frames });
    const { container } = draw(sent);
    expect(await answerTo(sent.requestId)).toMatchObject({ ok: true });
    expect(container.querySelectorAll(".react-flow__node")).toHaveLength(1);
  });

  it("frames a resource card or a note as it frames a card", async () => {
    const sent = job("image", { item: "orders" }, { resources: [{ id: "orders", kind: "queue", name: "orders", anchors: [], at: [0, 600], author: "agent:claude" }] });
    draw(sent);
    expect(await answerTo(sent.requestId)).toEqual({ ok: true, bounds: { x: 0, y: 600, width: 400, height: 220 } });
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
