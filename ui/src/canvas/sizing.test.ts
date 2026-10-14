import { describe, expect, it } from "vitest";
import { layoutFlow } from "../flow/layout";
import type { CardView, Description, DrawnFlow, StartCard, TypeDefinition, TypeMember } from "../ipc/generated";
import { fixture } from "../test/engine";
import { CARD_MAX_WIDTH, fallbackSize } from "./layout";
import { LOCATION_FONT, TITLE_FONT, TYPE_KIND_FONT, estimateText } from "./measure";
import {
  COMPACT_MAX_WIDTH,
  COMPACT_MIN_WIDTH,
  FIELDS_FOLDED,
  TYPE_MAX_WIDTH,
  compactCardWidth,
  descriptionHeight,
  estimatedCardSize,
  estimatedFieldOffset,
  expandedCardSize,
  expandedCardWidth,
  fieldTexts,
  headerText,
  listedFields,
  startCardSize,
  startCardWidth,
  typeCardHeight,
  typeCardWidth,
} from "./sizing";

const measure = estimateText;
const CART = "src/cart.ts#Cart";
const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });
/** A drawn flow of the nine examples, as the IPC sends it. */
function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

const place = drawn("01-place-order");
const cli = drawn("07-cli");
const createOrder = fixture("card", "card_create_order");

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

describe("definition cards", () => {
  it("size a compact card to its header between 240 and 560, with room for the chevron, the mark, and `</>`", () => {
    const short = { mark: { text: "ƒ", word: false }, name: "a", location: "a.ts:1", chevron: true };
    expect(compactCardWidth(short, measure)).toBe(COMPACT_MIN_WIDTH);
    expect(compactCardWidth({ ...short, name: "x".repeat(80) }, measure)).toBe(COMPACT_MAX_WIDTH);
    const method = { ...short, name: "StripeProvider.ChargeWithRetries", location: "services/payment.ts:13" };
    const parts = 14 + 8 + measure(method.name, TITLE_FONT) + measure(method.location, LOCATION_FONT);
    // `</>` at the end is 30px wide.
    expect(compactCardWidth(method, measure)).toBe(Math.ceil(26 + parts + 4 * 8 + 30));
    // A class's mark is its word, measured as it is drawn.
    expect(compactCardWidth({ ...method, mark: { text: "class", word: true } }, measure)).toBe(Math.ceil(26 + parts - 8 + measure("class", TYPE_KIND_FONT) + 4 * 8 + 30));
  });

  it("read their header from the drawn flow: the page's name over the code name, and where it is", () => {
    expect(headerText(place.header, true)).toEqual({ mark: { text: "ƒ", word: false }, name: "place_order", location: "shop/service.py:9", chevron: true });
    expect(headerText({ ...place.header, name: "Place the order" }, false).name).toBe("Place the order");
    expect(headerText(cli.header, true).mark).toEqual({ text: "M", word: false });
  });

  it("wrap the whole description, line by line; an expanded card shows one line", () => {
    expect(descriptionHeight(undefined, 300, measure)).toBe(0);
    const one = descriptionHeight(description("Places the order."), 300, measure);
    const two = descriptionHeight(description("Places the order.\nOr queues it."), 300, measure);
    expect(two - one).toBe(18);
    expect(descriptionHeight(description("word ".repeat(60)), 300, measure)).toBeGreaterThan(two);
    expect(descriptionHeight(description("word ".repeat(60)), null, measure)).toBe(one);
  });

  it("size an expanded card to its header, its signature, and its drawn flow, wider than 880 only for the flow", () => {
    const geometry = layoutFlow(place, { measure });
    const width = expandedCardWidth(place.header, geometry, measure);
    expect(width).toBe(Math.max(280, Math.ceil(geometry.width + 2)));
    const wide = { ...geometry, width: 2000 };
    expect(expandedCardWidth(place.header, wide, measure)).toBe(2002);
    expect(expandedCardWidth({ ...place.header, signature: "x".repeat(400) }, geometry, measure)).toBe(Math.max(CARD_MAX_WIDTH, width));
    // The header, what it says from its code (the signature and the doc), and the flow.
    const size = expandedCardSize(place.header, geometry, undefined, measure);
    const { signature: _signature, description: _doc, ...plain } = place.header;
    const bare = expandedCardSize(plain, geometry, undefined, measure);
    expect(bare.height).toBe(44 + Math.ceil(geometry.height) + 1);
    expect(size.height - bare.height).toBe(17 + 1 + 3 + 17 + 18);
  });

  it("are estimated before they are drawn: compact by their header, expanded by their laid-out flow", () => {
    const model: CardView = { type: "definition", flow: place };
    const compact = estimatedCardSize({ model, view: "compact" }, measure);
    expect(compact.height).toBe(44);
    expect(compact.width).toBeLessThanOrEqual(COMPACT_MAX_WIDTH);
    expect(estimatedCardSize({ model, view: "compact", description: description("Takes an order.") }, measure).height).toBeGreaterThan(44);
    expect(estimatedCardSize({ model, view: "expanded" }, measure)).toEqual(expandedCardSize(place.header, layoutFlow(place, { measure }), undefined, measure));
    expect(estimatedCardSize({ model: createOrder, view: "expanded" }, measure).height).toBeGreaterThan(200);
    expect(estimatedCardSize({ model: undefined, view: "expanded" }, measure)).toEqual(fallbackSize("expanded"));
  });
});

describe("app and entrance cards", () => {
  const start = fixture("card", "card_place_an_order") as StartCard & { type: "start" };

  it("size to their words between 200 and 320; expanded, to their code rows too", () => {
    expect(startCardWidth(start, "compact", measure)).toBeGreaterThanOrEqual(200);
    expect(startCardWidth({ ...start, name: "x".repeat(80) }, "compact", measure)).toBe(320);
    expect(startCardWidth(start, "expanded", measure)).toBeGreaterThanOrEqual(startCardWidth(start, "compact", measure));
  });

  it("grow by a row per code when expanded; an app's card has no small caps", () => {
    const compact = startCardSize(start, "compact", measure);
    // Border, padding, the app's small caps, the name, and what starts it.
    expect(compact.height).toBe(3 + 27 + 14 + 3 + 20 + 3 + 16);
    expect(startCardSize(start, "expanded", measure).height).toBe(compact.height + 8 + 3 * 20 + 2 * 4);
    expect(startCardSize({ ...start, kind: "app" }, "compact", measure).height).toBe(compact.height - 14 - 3);
    expect(estimatedCardSize({ model: start, view: "compact" }, measure)).toEqual(compact);
  });

  it("take in the dashed card of each code that isn't found, beside the card, and list only code that has a block", () => {
    const compact = startCardSize(start, "compact", measure);
    const missing = { ...start, code: [...start.code, { key: "gone", state: "missing" as const }, { key: "out", state: "not_analyzed" as const }] };
    const withMissing = startCardSize(missing, "compact", measure);
    // The arrow, then a card 250 wide: border, header and rule, padding, the key, and two lines under it; 16 between cards.
    expect(withMissing.width).toBe(compact.width + 56 + 250);
    expect(withMissing.height).toBe(2 * (2 + 43 + 34 + 16 + 48) + 16);
    expect(startCardSize(missing, "expanded", measure).height).toBe(Math.max(startCardSize(start, "expanded", measure).height, withMissing.height));
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
    const header = { mark: { text: "interface", word: true }, name: "Cart", location: "cart.ts:12", chevron: false };
    expect(typeCardWidth(header, fieldTexts(cart), measure)).toBeGreaterThanOrEqual(280);
    expect(typeCardWidth(header, [{ name: "x", type: "Record<string, ".repeat(20) }], measure)).toBe(TYPE_MAX_WIDTH);
    // The whole header fits (padding, border, kind, name, location, `</>`, and the gaps between them; a
    // type card has no chevron): the kind is measured as it is drawn, in 13px code.
    const job = { mark: { text: "interface", word: true }, name: "OrderJobWithAVeryLongName", location: "types/order-jobs.ts:22", chevron: false };
    const parts = measure("interface", TYPE_KIND_FONT) + measure(job.name, TITLE_FONT) + measure(job.location, LOCATION_FONT);
    expect(typeCardWidth(job, [{ name: "order", type: "OrderDraft" }], measure)).toBe(Math.ceil(26 + parts + 3 * 8 + 30));
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
    const short = listedFields(wide, false);
    expect(short.entries).toHaveLength(FIELDS_FOLDED);
    expect(short.hidden).toBe(12 - FIELDS_FOLDED);
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

  it("are estimated from their definition: a type's in every view, a class's while compact", () => {
    const type: CardView = { type: "definition", flow: { ...place, header: { ...place.header, key: CART, kind: "type", form: "interface", label: "Cart" } } };
    const size = estimatedCardSize({ model: type, view: "compact", definition: cart }, measure);
    expect(size.height).toBe(typeCardHeight(cart, 0));
    expect(estimatedCardSize({ model: type, view: "expanded", definition: cart }, measure)).toEqual(size);
    const klass: CardView = { type: "definition", flow: { ...place, header: { ...place.header, key: CART, kind: "class", form: "class", label: "Cart" } } };
    expect(estimatedCardSize({ model: klass, view: "compact", definition: cart }, measure).height).toBe(typeCardHeight(cart, 0));
    expect(estimatedCardSize({ model: klass, view: "expanded", definition: cart }, measure).height).not.toBe(typeCardHeight(cart, 0));
  });
});
