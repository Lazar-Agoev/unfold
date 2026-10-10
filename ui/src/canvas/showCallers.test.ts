// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Callers } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { FOLLOW_GAP } from "./layout";
import { showCallers } from "./showCallers";

const P = "p-orders0000";
const REF = { project: P, id: ".unfold/local/drafts/b.json" };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#function:createOrder";
const CALLER = "src/routes/index.ts#function:routes";
const ON_PAGE = "src/server.ts#function:start";

let engine: FakeEngine;
let callers: Callers;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  callers = {
    project: P,
    key: KEY,
    total: 2,
    callers: [
      { key: CALLER, name: "routes", qualifiedName: "routes", path: "src/routes/index.ts", line: 3, sites: 1 },
      { key: ON_PAGE, name: "start", qualifiedName: "start", path: "src/server.ts", line: 9, sites: 2 },
    ],
  };
  engine = installFakeEngine({
    handlers: {
      callers: () => callers,
      card: () => fixture("card", "card_create_order"),
      save_board: () => boardState(P, REF.id),
    },
  });
  store.adoptBoardState(
    boardState(
      P,
      REF.id,
      boardDocument([
        page("page-1", [
          { id: "create-order", ref: { block: KEY }, at: [1000, 200] },
          { id: "start", ref: { block: ON_PAGE }, at: [0, 900] },
        ]),
      ]),
    ),
  );
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("Show callers", () => {
  it("adds the callers not on the page to the card's left, as one undo step", async () => {
    await showCallers(PAGE, "create-order");
    const cards = store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
    expect(cards.map((card) => card.id)).toEqual(["create-order", "start", "routes"]);
    const added = cards[2];
    expect(added?.ref).toEqual({ block: CALLER });
    expect(added?.at[1]).toBe(200);
    expect(added?.at[0]).toBeLessThan(1000 - FOLLOW_GAP);
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(1);
    expect(engine.callsOf("callers")).toEqual([{ project: P, key: KEY, limit: 20 }]);
  });

  it("adds nothing when nothing calls the card", async () => {
    callers = { ...callers, total: 0, callers: [] };
    await showCallers(PAGE, "create-order");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards).toHaveLength(2);
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(0);
  });
});
