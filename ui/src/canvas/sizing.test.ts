import { describe, expect, it } from "vitest";
import type { CardView, Description, TypeDefinition, TypeMember } from "../ipc/generated";
import { fixture } from "../test/engine";
import { fallbackSize } from "./layout";
import { LOCATION_FONT, TITLE_FONT, TYPE_KIND_FONT, estimateText } from "./measure";
import {
  FIELDS_FOLDED,
  NODE_MAX_WIDTH,
  NODE_MIN_WIDTH,
  TYPE_MAX_WIDTH,
  blockCardWidth,
  descriptionHeight,
  estimatedBlockSize,
  estimatedCardSize,
  estimatedFieldOffset,
  fieldTexts,
  listedFields,
  nodeWidth,
  typeCardHeight,
  typeCardWidth,
} from "./sizing";

const measure = estimateText;
const KEY = "src/routes/orders.ts#function:createOrder";
const CART = "src/cart.ts#interface:Cart";

const member = (name: string, type: string, extra: Partial<TypeMember> = {}): TypeMember => ({
  name,
  kind: "property",
  optional: false,
  readonly: false,
  default: false,
  type: { text: type, spans: [] },
  ...extra,
});

function definition(members: TypeMember[], inherited: TypeDefinition["inherited"] = []): TypeDefinition {
  return { key: CART, kind: "interface", name: "Cart", path: "src/cart.ts", line: 12, language: "typescript", members, inherited, uses: 0 };
}

const description = (text: string): Description => ({ text, author: "person:lazar", signature: "" });

describe("Diagram nodes", () => {
  it("size to their header between 240 and 560", () => {
    const short = nodeWidth({ mark: "ƒ", name: "a", location: "a.ts:1", code: true }, measure);
    expect(short).toBe(NODE_MIN_WIDTH);
    const long = nodeWidth({ mark: "ƒ", name: "x".repeat(80), location: "routes/orders.ts:25", code: true }, measure);
    expect(long).toBe(NODE_MAX_WIDTH);
    // The chevron, the `ƒ`, a method's qualified name, and its location fit whole (padding, border, and the gaps between them).
    const method = { mark: "ƒ", name: "StripeProvider.ChargeWithRetries", location: "services/payment.ts:13", code: true };
    const parts = 14 + 8 + measure(method.name, TITLE_FONT) + measure(method.location, LOCATION_FONT);
    expect(nodeWidth(method, measure)).toBeGreaterThanOrEqual(Math.ceil(26 + parts + 3 * 8));
    expect(nodeWidth(method, measure)).toBeLessThan(NODE_MAX_WIDTH);
    // An entrance's mark is its kind's word.
    const entrance = { ...method, mark: "COMMAND", code: false };
    expect(nodeWidth(entrance, measure)).toBeGreaterThan(nodeWidth(method, measure));
  });

  it("make room for the chevron in an expanded block card's header", () => {
    const header = { name: "StripeProvider.ChargeWithRetries", location: "services/payment.ts", body: null };
    expect(blockCardWidth({ ...header, chevron: true }, measure) - blockCardWidth(header, measure)).toBe(14 + 8);
  });

  it("wrap the whole description, line by line; other cards show one line", () => {
    expect(descriptionHeight(undefined, 300, measure)).toBe(0);
    const one = descriptionHeight(description("Places the order."), 300, measure);
    const two = descriptionHeight(description("Places the order.\nOr queues it."), 300, measure);
    expect(two - one).toBe(18);
    expect(descriptionHeight(description("word ".repeat(60)), 300, measure)).toBeGreaterThan(two);
    expect(descriptionHeight(description("word ".repeat(60)), null, measure)).toBe(one);
  });

  it("are estimated before they are drawn: compact, whatever the function's length", () => {
    const model = fixture("card", "card_create_order");
    const code = estimatedCardSize({ key: KEY, model, view: "code" }, measure);
    expect(code).toEqual(estimatedBlockSize(model, measure));
    const node = estimatedCardSize({ key: KEY, model, view: "diagram" }, measure);
    expect(node.height).toBe(44);
    expect(node.width).toBeLessThanOrEqual(NODE_MAX_WIDTH);
    expect(estimatedCardSize({ key: KEY, model, view: "diagram", description: description("Takes an order.") }, measure).height).toBeGreaterThan(44);
    expect(estimatedCardSize({ key: KEY, model: undefined, view: "diagram" }, measure)).toEqual(fallbackSize(false, "node"));
  });
});

describe("type cards", () => {
  const cart = definition(
    [member("owner", "User", { optional: true }), member("createdAt", "Date", { readonly: true }), member("coupon", "string", { default: true })],
    [{ from: { text: "Draft", spans: [] }, members: [member("lines", "CartItem[]")] }, { from: { text: "BaseModel", spans: [] }, members: [] }],
  );

  it("list their rows as written, inherited ones after their own", () => {
    expect(fieldTexts(cart)).toEqual([
      { name: "owner?", type: "User" },
      { name: "readonly createdAt", type: "Date" },
      { name: "coupon", type: "string = …" },
      { name: "lines", type: "CartItem[]" },
    ]);
    expect(fieldTexts({ ...cart, members: [], inherited: [], aliased: { text: "Cart | null", spans: [] } })).toEqual([{ name: "=", type: "Cart | null" }]);
  });

  it("size to their header and widest field, up to 560", () => {
    const header = { mark: "interface", name: "Cart", location: "cart.ts:12" };
    expect(typeCardWidth(header, fieldTexts(cart), measure)).toBeGreaterThanOrEqual(280);
    expect(typeCardWidth(header, [{ name: "x", type: "Record<string, ".repeat(20) }], measure)).toBe(TYPE_MAX_WIDTH);
    // The whole header fits (padding, border, kind, name, location, and the gaps between them; a type card
    // has no chevron): the kind is measured as it is drawn, in 13px code.
    const job = { mark: "interface", name: "OrderJobWithAVeryLongName", location: "types/order-jobs.ts:22" };
    const parts = measure("interface", TYPE_KIND_FONT) + measure(job.name, TITLE_FONT) + measure(job.location, LOCATION_FONT);
    expect(typeCardWidth(job, [{ name: "order", type: "OrderDraft" }], measure)).toBe(Math.ceil(26 + parts + 2 * 8));
  });

  it("show a base's heading before its members, and a base without members last", () => {
    expect(listedFields(cart, false)).toEqual({
      entries: [
        { kind: "row", index: 0 },
        { kind: "row", index: 1 },
        { kind: "row", index: 2 },
        { kind: "heading", base: 0 },
        { kind: "row", index: 3 },
        { kind: "heading", base: 1 },
      ],
      hidden: 0,
    });
  });

  it("fold 12 fields or more behind “N more”, and estimate their rows before they are drawn", () => {
    const wide = definition(Array.from({ length: 12 }, (_, index) => member(`f${index}`, "string")));
    const folded = listedFields(wide, false);
    expect(folded.entries).toHaveLength(FIELDS_FOLDED);
    expect(folded.hidden).toBe(12 - FIELDS_FOLDED);
    expect(listedFields(wide, true).hidden).toBe(0);
    expect(listedFields(definition(Array.from({ length: 11 }, (_, index) => member(`f${index}`, "string"))), false).hidden).toBe(0);
    // The header, the grid's padding, 8 rows, and "N more".
    expect(typeCardHeight(wide, 0)).toBe(44 + 18 + 8 * 20 + 24);
    // Rows: the first row's center is under the header and the grid's top padding; a hidden one is at "N more".
    expect(estimatedFieldOffset(wide, "0", 0)).toBe(44 + 8 + 10);
    expect(estimatedFieldOffset(wide, "1", 35)).toBe(44 + 35 + 8 + 30);
    expect(estimatedFieldOffset(wide, "11", 0)).toBe(44 + 8 + 8 * 20 + 12);
    expect(estimatedFieldOffset(wide, "from0/0", 0)).toBeUndefined();
  });

  it("are estimated from their definition in Diagram view, and in Flow for a type declaration", () => {
    const model: CardView = {
      type: "block",
      header: { key: CART, kind: "interface", name: "Cart", qualifiedName: "Cart", path: "src/cart.ts", startLine: 12, endLine: 18, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
      code: null,
      flow: null,
    };
    const size = estimatedCardSize({ key: CART, model, view: "diagram", definition: cart }, measure);
    expect(size.height).toBe(typeCardHeight(cart, 0));
    expect(estimatedCardSize({ key: CART, model, view: "flow", definition: cart }, measure)).toEqual(size);
    expect(estimatedCardSize({ key: CART, model, view: "diagram" }, measure).height).toBe(fallbackSize(false, "fields").height);
  });
});
