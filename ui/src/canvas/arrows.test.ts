import { describe, expect, it } from "vitest";
import type { CallLink, CardView, TypeDefinition, TypeMember, TypeText } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";
import { NODE_PORT, computeArrows, fieldPort, fieldRows, portsByCard } from "./arrows";

function link(source: string, target: string | undefined, kind: CallLink["kind"] = "calls", line = 1): CallLink {
  return {
    source,
    owner: "src/a.ts#function:a",
    kind,
    resolution: target ? "resolved" : "unresolved",
    ...(target ? { target } : { reason: "dynamic" as const }),
    text: source,
    line,
  };
}

function block(key: string, links: CallLink[]): CardView {
  return {
    type: "block",
    header: { key, kind: "function", name: key, qualifiedName: key, path: "src/a.ts", startLine: 1, endLine: 9, language: "typescript", fileState: "current", source: "current", owner: "src/a.ts#" },
    code: {
      key,
      path: "src/a.ts",
      language: "typescript",
      kind: "function",
      startLine: 1,
      endLine: 9,
      source: { state: "current", text: "", dedent: 0, lines: [], spans: [], outline: [], links },
    },
    flow: null,
  };
}

const A = "src/a.ts#function:a";
const B = "src/b.ts#function:b";
const C = "src/c.ts#function:c";
const P = "p-orders0000";
const Q = "p-payments00";
/** A model map keyed as the canvas keys it: by project and key. */
const inP = (entries: readonly (readonly [string, CardView])[]) => new Map(entries.map(([key, view]) => [cardRefKey(P, key), view]));

describe("arrows", () => {
  it("draws one arrow per opened occurrence, even when several calls go to one card", () => {
    const models = inP([
      [A, block(A, [link(`${A}/call[0]`, B, "calls", 2), link(`${A}/call[1]`, B, "calls", 5), link(`${A}/call[2]`, C, "imports", 1), link(`${A}/call[3]`, undefined), link(`${A}/call[4]`, A)])],
      [B, block(B, [])],
    ]);
    const cards = [
      { id: "a", key: A, project: P, entrance: false, body: "code" as const },
      { id: "b", key: B, project: P, entrance: false, body: "code" as const },
    ];
    const arrows = computeArrows(cards, models);
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target])).toEqual([
      [`${A}/call[0]`, "b"],
      [`${A}/call[1]`, "b"],
    ]);
    expect(portsByCard(arrows).get("a")).toEqual([`${A}/call[0]`, `${A}/call[1]`]);
    // A Code link is its occurrence's own edge; the arrow names it for its label and contract.
    expect(arrows.map((arrow) => [arrow.project, arrow.seq])).toEqual([
      [P, 0],
      [P, 0],
    ]);

    const withC = computeArrows([...cards, { id: "c", key: C, project: P, entrance: false, body: "code" as const }], models);
    const imported = withC.find((arrow) => arrow.target === "c");
    expect(imported?.dashed).toBe(true);
    expect(withC.filter((arrow) => arrow.target === "b").every((arrow) => !arrow.dashed)).toBe(true);
  });

  it("points an entrance at each handler on the board, from its target row", () => {
    const entrance: CardView = {
      type: "entrance",
      header: { key: "entrance:e", kind: "http", title: "POST /orders", label: null, method: "POST", trigger: "/orders", triggerStatus: "exact", detector: "express.route", language: "typescript" },
      evidence: { path: "src/server.ts", startLine: 1, endLine: 1, pointer: null, source: "current", code: null, text: null },
      targets: [
        { position: 0, role: "handler", text: "a", resolution: "resolved", target: A, name: "a", path: "src/a.ts", line: 1, note: null, reason: null, module: null },
        { position: 1, role: "handler", text: "c", resolution: "resolved", target: C, name: "c", path: "src/c.ts", line: 1, note: null, reason: null, module: null },
      ],
      applications: [],
    };
    const arrows = computeArrows(
      [
        { id: "e", key: "entrance:e", project: P, entrance: true, body: "handlers" as const },
        { id: "a", key: A, project: P, entrance: false, body: "code" as const },
      ],
      inP([["entrance:e", entrance]]),
    );
    expect(arrows).toEqual([expect.objectContaining({ source: "e", sourceHandle: "target:0", target: "a", kind: "handler", dashed: false })]);
  });

  it("joins only cards of the same project, and none to a card that isn't connected", () => {
    const models = inP([[A, block(A, [link(`${A}/call[0]`, B, "calls", 2)])]]);
    const cards = [
      { id: "a", key: A, project: P, entrance: false, body: "code" as const },
      // The same key in another project, and a card whose project isn't connected.
      { id: "b-elsewhere", key: B, project: Q, entrance: false, body: "code" as const },
      { id: "b-gone", key: B, project: null, entrance: false, body: "code" as const },
    ];
    expect(computeArrows(cards, models)).toEqual([]);
    const arrows = computeArrows([...cards, { id: "b", key: B, project: P, entrance: false, body: "code" as const }], models);
    expect(arrows.map((arrow) => arrow.target)).toEqual(["b"]);
  });
});

describe("arrows from a Diagram node", () => {
  const models = inP([
    [A, block(A, [link(`${A}/call[0]`, B, "calls", 2), link(`${A}/call[1]`, B, "calls", 5), link(`${A}/call[2]`, C, "calls", 7)])],
    [B, block(B, [])],
  ]);
  const cards = [
    { id: "a", key: A, project: P, entrance: false, body: "node" as const },
    { id: "b", key: B, project: P, entrance: false, body: "node" as const },
    { id: "c", key: C, project: P, entrance: false, body: "code" as const },
  ];

  it("leave from the node's edge, one per card they reach: several calls as one arrow, one call as itself", () => {
    const arrows = computeArrows(cards, models);
    expect(arrows.map((arrow) => [arrow.id, arrow.sourceHandle, arrow.target])).toEqual([
      [`a|${NODE_PORT}|b`, NODE_PORT, "b"],
      [`a|${NODE_PORT}|c`, NODE_PORT, "c"],
    ]);
    const [toB, toC] = arrows;
    expect(toB?.calls).toEqual([
      { source: `${A}/call[0]`, seq: 0, line: 2, kind: "calls" },
      { source: `${A}/call[1]`, seq: 0, line: 5, kind: "calls" },
    ]);
    // One call keeps its line: its label and contract are that call's.
    expect(toC?.calls).toBeUndefined();
    expect([toC?.occurrence, toC?.seq, toC?.line]).toEqual([`${A}/call[2]`, 0, 7]);
    expect(portsByCard(arrows).get("a")).toEqual([NODE_PORT]);
  });

  it("point an entrance node at each handler from its edge", () => {
    const entrance: CardView = {
      type: "entrance",
      header: { key: "entrance:e", kind: "http", title: "POST /orders", label: null, method: "POST", trigger: "/orders", triggerStatus: "exact", detector: "express.route", language: "typescript" },
      evidence: { path: "src/server.ts", startLine: 1, endLine: 1, pointer: null, source: "current", code: null, text: null },
      targets: [{ position: 0, role: "handler", text: "a", resolution: "resolved", target: A, name: "a", path: "src/a.ts", line: 1, note: null, reason: null, module: null }],
      applications: [],
    };
    const arrows = computeArrows([{ id: "e", key: "entrance:e", project: P, entrance: true, body: "node" }, ...cards], inP([["entrance:e", entrance]]));
    expect(arrows).toEqual([expect.objectContaining({ source: "e", sourceHandle: NODE_PORT, target: "a", kind: "handler", occurrence: "target:0" })]);
  });
});

const CART = "src/cart.ts#interface:Cart";
const ITEM = "src/cart.ts#interface:CartItem";
const USER = "src/user.ts#interface:User";
const DRAFT = "src/cart.ts#interface:Draft";

/** A written type naming `targets` (declared) and `builtin` names, in order. */
function written(text: string, spans: readonly (readonly [string, string | null])[]): TypeText {
  return {
    text,
    spans: spans.map(([name, target]) => {
      const start = text.indexOf(name);
      return target === null ? { start, end: start + name.length, style: "builtin" as const, module: "global" } : { start, end: start + name.length, style: "declared" as const, target };
    }),
  };
}

const field = (name: string, type: TypeText): TypeMember => ({ name, kind: "property", optional: false, readonly: false, default: false, type });

const cart: TypeDefinition = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 12,
  language: "typescript",
  members: [
    field("owner", written("User", [["User", USER]])),
    field("items", written("CartItem[]", [["CartItem", ITEM]])),
    field("pair", written("Map<CartItem, CartItem>", [["Map", null], ["CartItem", ITEM]])),
    field("createdAt", written("Date", [["Date", null]])),
    field("self", written("Cart", [["Cart", CART]])),
  ],
  inherited: [{ from: written("Draft", [["Draft", DRAFT]]), members: [field("lines", written("CartItem[]", [["CartItem", ITEM]]))] }],
  uses: 3,
};

describe("type arrows", () => {
  const models = inP([
    [CART, block(CART, [])],
    [ITEM, block(ITEM, [])],
  ]);
  const definitions = new Map([[cardRefKey(P, CART), cart]]);
  const typeCards = (body: "fields" | "code") => [
    { id: "cart", key: CART, project: P, entrance: false, body },
    { id: "item", key: ITEM, project: P, entrance: false, body: "fields" as const },
    // The same type in another project: type links never cross projects.
    { id: "user-elsewhere", key: USER, project: Q, entrance: false, body: "fields" as const },
  ];

  it("leave from each field's row to each card its written type names, labeled with the type, once per field and card", () => {
    const arrows = computeArrows(typeCards("fields"), models, undefined, definitions);
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target, arrow.kind, arrow.type?.text])).toEqual([
      [fieldPort("1"), "item", "type", "CartItem[]"],
      [fieldPort("2"), "item", "type", "Map<CartItem, CartItem>"],
      [fieldPort("from0/0"), "item", "type", "CartItem[]"],
    ]);
    expect(arrows.every((arrow) => !arrow.dashed)).toBe(true);
  });

  it("leave an inherited field to the base's card when the base is on the page", () => {
    const withDraft = [...typeCards("fields"), { id: "draft", key: DRAFT, project: P, entrance: false, body: "fields" as const }];
    const arrows = computeArrows(withDraft, models, undefined, definitions);
    expect(arrows.map((arrow) => arrow.sourceHandle)).toEqual([fieldPort("1"), fieldPort("2")]);
  });

  it("are drawn only while the card shows its fields", () => {
    expect(computeArrows(typeCards("code"), models, undefined, definitions)).toEqual([]);
  });

  it("name rows as the card lists them: an alias's type, its members, then each base's", () => {
    const alias: TypeDefinition = { ...cart, kind: "type_alias", members: [], inherited: [], aliased: written("Cart | null", [["Cart", CART]]) };
    expect(fieldRows(alias).map((row) => row.row)).toEqual(["="]);
    expect(fieldRows(cart).map((row) => row.row)).toEqual(["0", "1", "2", "3", "4", "from0/0"]);
  });
});
