// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView } from "../ipc/generated";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { useBoardArrows } from "./useBoardArrows";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const CREATE = "src/routes/orders.ts#createOrder";
const PLACE = "src/services/order-service.ts#OrderService.place";

const createOrder = fixture("card", "card_create_order");
const cards: Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "place", ref: { block: PLACE }, at: [800, 0] },
];

/** A callee's card: createOrder's header under another key, drawing no steps. */
function leaf(key: string): CardView {
  if (createOrder.type !== "definition") {
    throw new Error("createOrder draws a definition");
  }
  return { type: "definition", flow: { ...createOrder.flow, header: { ...createOrder.flow.header, key, label: "place" }, steps: { steps: [] }, routes: [] } };
}

let engine: FakeEngine;
let gone: boolean;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  gone = false;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === CREATE && gone) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such definition." };
        }
        return key === CREATE ? createOrder : leaf(key);
      },
    },
  });
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards)])));
});

afterEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine.uninstall();
});

const wrapper = ({ children }: { readonly children: ReactNode }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;

describe("a page's arrows", () => {
  it("draw none from a card whose definition is gone, though its query keeps what it drew before", async () => {
    const { result } = renderHook(() => useBoardArrows(PAGE, cards), { wrapper });
    await waitFor(() => {
      expect(result.current.arrows.filter((arrow) => arrow.source === "create-order" && arrow.target === "place")).not.toEqual([]);
    });
    gone = true;
    await act(async () => {
      await queryClient.invalidateQueries();
    });
    await waitFor(() => {
      expect(result.current.views.has("create-order")).toBe(false);
    });
    expect(result.current.arrows.filter((arrow) => arrow.source === "create-order")).toEqual([]);
  });
});
