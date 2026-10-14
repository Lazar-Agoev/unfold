// @vitest-environment jsdom
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView, DrawnFlow, Page } from "../ipc/generated";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { adoptBoardState, editPage, getOpenBoard, resetBoards } from "./boards/store";
import { toggleOpen } from "./boards/document";
import { cardArgs, cardArgsOf, cardName, combinableSteps, eachSequence, fetchCard, findBlock, flowBlocks, heldBlocks, openPathOf, overlayIds, pageOverlay, useCard } from "./cards";
import { setShowAllCalls } from "./showAllCalls";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const PAGE = { board: { project: P, id: ID }, page: "page-1" };
const CREATE = "src/routes/orders.ts#createOrder";
const BRANCH = "src/routes/orders.ts#createOrder/if[0]/then";
const CARD: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0] };

const OVERLAID: Partial<Page> = {
  names: [{ key: BRANCH, text: "Wants an async reply?", author: "person:lazar", signature: "sha256:a" }],
  combined: [{ id: "reply", title: "Reply", members: ["k1", "k2"], collapsed: true, author: "agent:claude", signature: "sha256:b", description: { text: "Sends it.", author: "agent:claude", signature: "" } }],
  proposals: [
    { id: "log", after: "k2", title: "Log it", reason: "For support.", author: "agent:claude" },
    { id: "check", start: CREATE, title: "Check the cart", author: "person:lazar" },
  ],
};

let engine: FakeEngine;

beforeEach(() => {
  resetBoards();
  queryClient.clear();
  setShowAllCalls(false);
  engine = installFakeEngine({ handlers: { card: ({ key }) => fixture("card", key === CREATE ? "card_create_order" : "card_place_an_order") } });
});

afterEach(() => {
  engine.uninstall();
});

function wrapper({ children }: { readonly children: ReactNode }) {
  return createElement(QueryClientProvider, { client: queryClient }, children);
}

describe("a card's arguments", () => {
  it("carry the page's overlay: its names, combined steps, and proposed steps, as the card command takes them", () => {
    expect(pageOverlay(page("page-1", [], OVERLAID))).toEqual({
      names: [{ key: BRANCH, text: "Wants an async reply?" }],
      combined: [{ id: "reply", title: "Reply", members: ["k1", "k2"], collapsed: true }],
      proposed: [
        { id: "log", after: "k2", title: "Log it" },
        { id: "check", start: CREATE, title: "Check the cart" },
      ],
    });
    expect(pageOverlay(page("page-1"))).toEqual({});
  });

  it("carry the card's key, the calls it opens in place, and Show all calls, leaving out what is empty or off", () => {
    expect(cardArgs(P, CARD, {}, false)).toEqual({ project: P, key: CREATE });
    const open = { ...CARD, open: [["call-a"], ["call-a", "call-b"]] };
    expect(cardArgs(P, open, { names: [{ key: BRANCH, text: "x" }] }, true)).toEqual({
      project: P,
      key: CREATE,
      overlay: { names: [{ key: BRANCH, text: "x" }] },
      open: [["call-a"], ["call-a", "call-b"]],
      showAllCalls: true,
    });
    expect(cardArgs(P, { ref: { entrance: "entrance:server/place-an-order" } }, {}, false)).toEqual({ project: P, key: "entrance:server/place-an-order" });
  });

  it("are read from the open page and the switch for a card on it", () => {
    adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD], OVERLAID)])));
    setShowAllCalls(true);
    expect(cardArgsOf(PAGE, "create-order")).toEqual({ project: P, key: CREATE, overlay: pageOverlay(page("page-1", [], OVERLAID)), showAllCalls: true });
    expect(cardArgsOf(PAGE, "gone")).toBeUndefined();
  });
});

describe("useCard", () => {
  it("asks for the card again when the page's names, its open calls, or Show all calls change", async () => {
    adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
    const { result, rerender } = renderHook(() => useCard(PAGE, adoptedCard()), { wrapper });
    await waitFor(() => {
      expect(result.current.data?.type).toBe("definition");
    });
    act(() => {
      editPage(PAGE, (draft) => {
        draft.names.push({ key: BRANCH, text: "Async?", author: "person:lazar", signature: "" });
        toggleOpen(draft, "create-order", ["call-a"]);
      });
      setShowAllCalls(true);
    });
    rerender();
    await waitFor(() => {
      expect(engine.callsOf("card")).toEqual([
        { project: P, key: CREATE },
        { project: P, key: CREATE, overlay: { names: [{ key: BRANCH, text: "Async?" }] }, open: [["call-a"]], showAllCalls: true },
      ]);
    });
    // The card as last drawn stays while it is asked for again.
    expect(result.current.data?.type).toBe("definition");
  });

  it("reads a card on an open page through the cache", async () => {
    adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [CARD])])));
    const view = await fetchCard(PAGE, "create-order");
    expect(view && cardName(view)).toBe("createOrder");
    await fetchCard(PAGE, "create-order");
    expect(engine.callsOf("card")).toHaveLength(1);
  });
});

/** The card on the open page now, as the canvas reads its cards from the store. */
function adoptedCard(): Card {
  return getOpenBoard(PAGE.board)?.document.pages[0]?.cards[0] ?? CARD;
}

describe("a drawn flow's blocks", () => {
  const view = fixture("card", "card_create_order");
  const flow = (view as Extract<CardView, { type: "definition" }>).flow;

  it("are read from its parts, its definition first, then its steps in the order they run", () => {
    const blocks = flowBlocks(flow);
    expect(blocks[0]).toMatchObject({ key: CREATE, form: "function", label: "createOrder" });
    expect(findBlock(flow, BRANCH)).toMatchObject({ form: "then", label: 'req.header("prefer") === "respond-async"' });
    expect(findBlock(flow, "src/routes/orders.ts#createOrder/call:orderService.place")).toMatchObject({ form: "call", label: "orderService.place" });
    expect(findBlock(flow, "nowhere")).toBeUndefined();
  });

  it("name a card by the page's name for its definition, else its name from the code; a start by its name", () => {
    expect(cardName(view)).toBe("createOrder");
    expect(cardName({ type: "definition", flow: { ...flow, header: { ...flow.header, name: "Take an order" } } })).toBe("Take an order");
    expect(cardName(fixture("card", "card_place_an_order"))).toBe("Place an order");
  });

  it("list the combined and proposed steps a card draws", () => {
    const steps = flow.steps.steps;
    const drawn: CardView = {
      type: "definition",
      flow: {
        ...flow,
        steps: {
          steps: [
            ...steps,
            { id: 90, state: "reached", kind: "combined", item: "reply", title: "Reply", members: [], collapsed: true, lines: [1, 2], conditions: 0, loops: 0, calls: [], library: 0, unknown: 0 },
            { id: 91, state: "reached", kind: "proposed", item: "log", title: "Log it" },
          ],
        },
      },
    };
    expect(overlayIds(drawn)).toEqual(["reply", "log"]);
    expect(overlayIds(fixture("card", "card_place_an_order"))).toEqual([]);
  });
});

describe("where a block opens and combines", () => {
  const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });
  const cli = GOLDENS["../../../testdata/goldens/drawn/07-cli.json"];
  if (!cli) {
    throw new Error("no golden 07-cli");
  }
  const MAIN_CALL = "shop/cli.py#/if[0]/then/call:main";
  const PLACE_CALL = "shop/cli.py#main/call:place_order";
  const LOAD_CALL = "shop/service.py#place_order/call:load_order";

  it("opens a call along the calls opened around it, outermost first", () => {
    expect(openPathOf(cli, MAIN_CALL)).toEqual([MAIN_CALL]);
    expect(openPathOf(cli, PLACE_CALL)).toEqual([MAIN_CALL, PLACE_CALL]);
    expect(openPathOf(cli, LOAD_CALL)).toEqual([MAIN_CALL, PLACE_CALL, LOAD_CALL]);
    expect(openPathOf(cli, "nowhere")).toBeUndefined();
  });

  it("finds the steps of a body drawn anywhere in the flow: a definition's own, a part's, an opened callee's", () => {
    expect(combinableSteps(cli, "shop/cli.py#main")).toEqual([PLACE_CALL]);
    expect(combinableSteps(cli, "shop/service.py#place_order")?.slice(0, 2)).toEqual([LOAD_CALL, "shop/service.py#place_order/if[0]"]);
    expect(combinableSteps(cli, "shop/service.py#place_order/try[0]/body")).toEqual(["shop/service.py#place_order/try[0]/body/call:charge_card"]);
    expect(combinableSteps(cli, "nowhere")).toBeUndefined();
  });

  it("names a part by the text that opens it, with the lines it marks, as the reader labels it", () => {
    expect(findBlock(cli, "shop/service.py#place_order/try[0]/except[0]")).toMatchObject({ form: "except", label: "except PaymentDeclined as error", lines: [20, 22] });
    expect(findBlock(cli, "shop/service.py#place_order/if[0]/elif[0]")).toMatchObject({ form: "elif", label: 'elif order["paid"]', lines: [14, 15] });
    expect(findBlock(cli, LOAD_CALL)?.lines).toBeUndefined();
  });

  it("walks every sequence it draws, with its body and its file, and lists what collapsed combined steps hold", () => {
    const bodies: string[] = [];
    eachSequence(cli, (_sequence, body, path) => {
      bodies.push(`${path} ${body}`);
    });
    expect(bodies.slice(0, 3)).toEqual(["shop/cli.py shop/cli.py#", "shop/cli.py shop/cli.py#/if[0]/then", "shop/cli.py shop/cli.py#main"]);
    expect(bodies).toContain("shop/service.py shop/service.py#place_order/try[0]/except[0]");
    const shaped = GOLDENS["../../../testdata/goldens/drawn/place-order-shaped.json"];
    if (!shaped) {
      throw new Error("no golden place-order-shaped");
    }
    expect(heldBlocks(shaped)).toContain("shop/service.py#place_order/if[0]/then/raise[0]/call:ValueError");
    expect(heldBlocks(cli)).toEqual([]);
  });
});
