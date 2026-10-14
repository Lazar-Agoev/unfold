import { describe, expect, it } from "vitest";
import type { CardView, DrawnFlow, Page } from "../ipc/generated";
import { page } from "../test/workspace";
import { blockSelection, rowTarget, selectionCode } from "./targets";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

function drawn(name: string): CardView {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return { type: "definition", flow };
}

// Keys as the goldens write them; nothing here takes them apart.
const PLACE = "shop/service.py#place_order";
const SAVE = "shop/service.py#place_order/call:save_order";
const LOAD = "shop/service.py#place_order/call:load_order";
const ENTRANCE = "entrance:server/place-an-order";

const PAGE: Page = page(
  "page-1",
  [
    { id: "place", ref: { block: PLACE }, at: [0, 0] },
    { id: "start", ref: { entrance: ENTRANCE }, at: [-400, 0] },
  ],
  {
    combined: [{ id: "save", title: "Save it and reserve stock", members: [SAVE, "shop/service.py#place_order/for[0]"], collapsed: true, author: "person:lazar", signature: "s" }],
    // The guards record what each cited key is: a block's node kind, or `entrance` for an app or entrance.
    guards: [
      { key: SAVE, kind: "call", owner: PLACE, signature: "s" },
      { key: ENTRANCE, kind: "entrance", owner: "", signature: "s" },
    ],
    resources: [
      { id: "orders", kind: "table", name: "orders", anchors: [{ file: "db/schema.sql", lines: [3, 9] }], at: [0, 0], author: "agent:claude" },
      { id: "queue", kind: "queue", name: "jobs", anchors: [], at: [0, 0], author: "agent:claude" },
      { id: "store", kind: "storage", name: "store", anchors: [{ key: SAVE }, { key: ENTRANCE }], at: [0, 0], author: "agent:claude" },
      { id: "client", kind: "actor", name: "client", anchors: [{ key: ENTRANCE }], at: [0, 0], author: "agent:claude" },
    ],
    links: [
      { id: "writes", from: "place", to: "orders", kind: "writes", step: SAVE, author: "agent:claude" },
      { id: "calls", from: "client", to: "start", kind: "http", anchors: [{ key: ENTRANCE }], author: "agent:claude" },
      { id: "reads", from: "place", to: "orders", kind: "reads", anchors: [{ file: "db/schema.sql", lines: [12, 14] }], author: "agent:claude" },
    ],
  },
);

const VIEWS: ReadonlyMap<string, CardView> = new Map([["place", drawn("place-order-shaped")]]);

describe("what the code panel opens for a selection", () => {
  it("opens a card's definition, an entrance card's lines in unfold.toml, and a block in its card", () => {
    expect(selectionCode({ kind: "card", cardId: "place", key: null }, PAGE, VIEWS)).toEqual({ target: { kind: "block", key: PLACE }, cardId: "place" });
    expect(selectionCode({ kind: "card", cardId: "start", key: null }, PAGE, VIEWS)).toEqual({ target: { kind: "start", key: ENTRANCE }, cardId: "start" });
    expect(selectionCode({ kind: "card", cardId: "place", key: LOAD }, PAGE, VIEWS)).toEqual({ target: { kind: "block", key: LOAD }, cardId: "place" });
    expect(selectionCode({ kind: "card", cardId: "gone", key: null }, PAGE, VIEWS)).toBeNull();
  });

  it("opens a combined step's lines, titled, in the file of the card drawing it", () => {
    expect(selectionCode({ kind: "combined", combinedId: "save" }, PAGE, VIEWS)).toEqual({
      target: { kind: "lines", path: "shop/service.py", lines: [25, 27], title: "Save it and reserve stock" },
      cardId: "place",
    });
    expect(selectionCode({ kind: "combined", combinedId: "save" }, PAGE, new Map())).toBeNull();
  });

  it("opens an arrow's call, and what a resource card or a connection cites first", () => {
    expect(selectionCode({ kind: "line", arrowId: "a", call: SAVE, from: "place", to: "store" }, PAGE, VIEWS)).toEqual({ target: { kind: "block", key: SAVE }, cardId: "place" });
    expect(selectionCode({ kind: "resource", resourceId: "orders" }, PAGE, VIEWS)).toEqual({ target: { kind: "lines", path: "db/schema.sql", lines: [3, 9], mark: [3, 9] }, cardId: null });
    expect(selectionCode({ kind: "link", linkId: "writes" }, PAGE, VIEWS)).toEqual({ target: { kind: "block", key: SAVE }, cardId: null });
  });

  it("opens what a citation names: a block, an app's or entrance's lines in unfold.toml, or an evidence file's lines, marked", () => {
    expect(selectionCode({ kind: "resource", resourceId: "store" }, PAGE, VIEWS)).toEqual({ target: { kind: "block", key: SAVE }, cardId: null });
    expect(selectionCode({ kind: "resource", resourceId: "client" }, PAGE, VIEWS)).toEqual({ target: { kind: "start", key: ENTRANCE }, cardId: null });
    expect(selectionCode({ kind: "link", linkId: "calls" }, PAGE, VIEWS)).toEqual({ target: { kind: "start", key: ENTRANCE }, cardId: null });
    expect(selectionCode({ kind: "link", linkId: "reads" }, PAGE, VIEWS)).toEqual({ target: { kind: "lines", path: "db/schema.sql", lines: [12, 14], mark: [12, 14] }, cardId: null });
  });

  it("opens a field of a type card as its type's written member", () => {
    expect(selectionCode({ kind: "field", cardId: "place", key: "src/cart.ts#Cart", index: 2 }, PAGE, VIEWS)).toEqual({ target: { kind: "field", key: "src/cart.ts#Cart", index: 2 }, cardId: "place" });
    expect(selectionCode({ kind: "field", cardId: "gone", key: "src/cart.ts#Cart", index: 2 }, PAGE, VIEWS)).toBeNull();
  });

  it("opens a row of other calls from its first call's statement to its last one's", () => {
    const flow = drawn("08-api");
    const step = flow.type === "definition" ? flow.flow.steps.steps.find((item) => item.kind === "thin") : undefined;
    if (step?.kind !== "thin") {
      throw new Error("08-api draws a thin row");
    }
    expect(rowTarget("shop/api.py", step)).toEqual({ kind: "lines", path: "shop/api.py", lines: [6, 15] });
  });

  it("opens nothing for what has no source", () => {
    for (const selection of [
      { kind: "none" },
      { kind: "note", noteId: "n" },
      { kind: "frame", frameId: "f" },
      { kind: "proposal", proposalId: "p" },
      { kind: "resource", resourceId: "queue" },
      { kind: "calls", arrowId: "a", from: "place", to: "store", calls: [SAVE] },
    ] as const) {
      expect(selectionCode(selection, PAGE, VIEWS)).toBeNull();
    }
  });
});

describe("what a click on a line selects", () => {
  it("selects the block in the card that draws it, the card itself for its own key", () => {
    expect(blockSelection(PAGE, VIEWS, LOAD, null)).toEqual({ kind: "card", cardId: "place", key: LOAD });
    expect(blockSelection(PAGE, VIEWS, PLACE, "place")).toEqual({ kind: "card", cardId: "place", key: null });
  });

  it("selects the collapsed combined step holding a block, and nothing for a block no card draws", () => {
    expect(blockSelection(PAGE, VIEWS, SAVE, null)).toEqual({ kind: "combined", combinedId: "save" });
    expect(blockSelection(PAGE, VIEWS, "shop/service.py#place_order/call:len", null)).toBeNull();
  });

  it("prefers the card the panel's selection is in", () => {
    const page = { ...PAGE, cards: [...PAGE.cards, { id: "again", ref: { block: PLACE }, at: [0, 900] as [number, number] }] };
    const views = new Map([...VIEWS, ["again", drawn("place-order-shaped")]]);
    expect(blockSelection(page, views, LOAD, "again")).toEqual({ kind: "card", cardId: "again", key: LOAD });
  });
});
