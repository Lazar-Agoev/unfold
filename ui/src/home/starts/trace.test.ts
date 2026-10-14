// @vitest-environment jsdom
import "../../test/dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type * as Components from "../../components";
import { queryClient } from "../../ipc/client";
import type { CardView, StartCard } from "../../ipc/generated";
import { FOLLOW_GAP } from "../../canvas/layout";
import { getOpenBoard, resetBoards } from "../../state/boards/store";
import { useShownBoardStore } from "../../state/shownBoard";
import { fixture, installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardState } from "../../test/workspace";
import { traceStart } from "./trace";

const toasts = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../../components", async (original) => ({ ...(await original<typeof Components>()), showToast: toasts.showToast }));

const P = "p-orders0000";
/** What the reader calls each definition. */
const LABELS: Readonly<Record<string, string>> = {
  "src/routes/middleware.ts#requireUser": "requireUser",
  "src/routes/middleware.ts#validateBody": "validateBody",
  "src/routes/orders.ts#createOrder": "createOrder",
  "src/worker.ts#": "src/worker.ts",
};
const BOARD = { project: P, id: ".unfold/boards/traced.json" };

let engine: FakeEngine;

/** A definition's card named `label`. */
function definition(key: string, label: string): CardView {
  const card = fixture("card", "card_create_order");
  if (card.type !== "definition") {
    throw new Error("card_create_order draws a definition");
  }
  return { ...card, flow: { ...card.flow, header: { ...card.flow.header, key, label } } };
}

function install(starts: Readonly<Record<string, StartCard>>) {
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        const start = starts[key];
        if (start) {
          return { type: "start", ...start };
        }
        return definition(key, LABELS[key] ?? "unknown");
      },
      new_board: ({ project }) => boardState(project, BOARD.id),
      save_board: ({ project, id }) => boardState(project, id),
    },
  });
}

const cards = () => getOpenBoard(BOARD)?.document.pages[0]?.cards ?? [];

beforeEach(() => {
  vi.clearAllMocks();
  resetBoards();
  queryClient.clear();
  useShownBoardStore.setState({ board: null });
});

afterEach(() => {
  engine.uninstall();
});

describe("tracing", () => {
  it("puts an entrance and the cards for its code, expanded and in order, on a new board named after it", async () => {
    const start = fixture("card", "card_place_an_order");
    if (start.type !== "start") {
      throw new Error("card_place_an_order draws a start");
    }
    install({ [start.key]: start });
    await traceStart(P, start.key, "Place an order");
    expect(engine.callsOf("new_board")).toEqual([{ project: P, title: "Place an order" }]);
    expect(useShownBoardStore.getState().board).toEqual(BOARD);
    const placed = cards();
    expect(placed.map((card) => [card.ref, card.view])).toEqual([
      [{ entrance: "entrance:server/place-an-order" }, undefined],
      [{ block: "src/routes/middleware.ts#requireUser" }, "expanded"],
      [{ block: "src/routes/middleware.ts#validateBody" }, "expanded"],
      [{ block: "src/routes/orders.ts#createOrder" }, "expanded"],
    ]);
    // The code goes in one column to the entrance's right, top to bottom in the order it runs.
    const [entrance, ...code] = placed;
    expect(entrance?.at).toEqual([0, 0]);
    const xs = new Set(code.map((card) => card.at[0]));
    expect(xs.size).toBe(1);
    expect([...xs][0]).toBeGreaterThanOrEqual(FOLLOW_GAP);
    const ys = code.map((card) => card.at[1]);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(new Set(ys).size).toBe(3);
    expect(code.map((card) => card.id)).toEqual(["require-user", "validate-body", "create-order"]);
  });

  it("traces an app's code, and leaves code that isn't found on the start card", async () => {
    install({
      "entrance:worker": {
        key: "entrance:worker",
        kind: "app",
        app: "Worker",
        name: "Worker",
        on: "npm run worker",
        code: [
          { key: "src/worker.ts#", state: "found", label: "src/worker.ts", path: "src/worker.ts", line: 1 },
          { key: "src/gone.ts#run", state: "missing" },
        ],
      },
    });
    await traceStart(P, "entrance:worker", "Worker");
    expect(cards().map((card) => card.ref)).toEqual([{ entrance: "entrance:worker" }, { block: "src/worker.ts#" }]);
  });

  it("says why it couldn't trace, and makes no board", async () => {
    engine = installFakeEngine({
      handlers: {
        card: () => {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No app or entrance has that key" };
        },
      },
    });
    await traceStart(P, "entrance:gone", "Gone");
    expect(engine.callsOf("new_board")).toEqual([]);
    expect(toasts.showToast).toHaveBeenCalledWith({ title: "Couldn't trace Gone", body: "No app or entrance has that key", tone: "danger" });
  });
});
