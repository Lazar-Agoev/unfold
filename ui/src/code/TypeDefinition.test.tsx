// @vitest-environment jsdom
import "../test/dom";
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TypeDefinition as TypeDefinitionModel, TypeMember } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { renderWithClient, testQueryClient } from "../test/render";
import { TypeDefinition } from "./TypeDefinition";
import { memberName } from "./typeKinds";

const P = "p-shop000000";
const CART = "src/cart.ts#interface:Cart";

const cart: TypeDefinitionModel = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 12,
  language: "typescript",
  members: [
    { name: "id", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartId", spans: [] } },
    { name: "coupon", kind: "property", optional: true, readonly: false, default: false, type: { text: "string", spans: [] } },
    { name: "createdAt", kind: "property", optional: false, readonly: true, default: false, type: { text: "Date", spans: [] } },
  ],
  inherited: [],
  uses: 6,
};

const ORDER = "src/model.ts#interface:Order";
const DRAFT = "src/model.ts#interface:OrderDraft";

const property = (name: string, type: string): TypeMember => ({ name, kind: "property", optional: false, readonly: false, default: false, type: { text: type, spans: [] } });

const order: TypeDefinitionModel = {
  key: ORDER,
  kind: "interface",
  name: "Order",
  path: "src/model.ts",
  line: 21,
  language: "typescript",
  members: [property("id", "string")],
  inherited: [
    {
      from: { text: "OrderDraft", spans: [{ start: 0, end: 10, style: "declared", target: DRAFT }] },
      members: [property("customerId", "string"), property("email", "string")],
    },
    { from: { text: "Address", spans: [{ start: 0, end: 7, style: "unresolved", note: "ambiguous target" }] }, members: [] },
    { from: { text: "Stripe.MetadataParam", spans: [{ start: 0, end: 20, style: "external", module: "stripe", note: "stripe" }] }, members: [] },
    { from: { text: "Error", spans: [{ start: 0, end: 5, style: "builtin", module: "global" }] }, members: [] },
  ],
  uses: 2,
};

function renderCart() {
  const client = testQueryClient();
  client.setQueryData(engineKey("type_definition", { project: P, key: CART }), cart);
  return renderWithClient(<TypeDefinition project={P} typeKey={CART} row="send:0" depth={1} openTypes={{}} onToggle={() => undefined} />, client);
}

describe("a type's definition", () => {
  it("puts every member in one grid, name then type, so the types share a column", () => {
    renderCart();
    const definition = screen.getByRole("group", { name: "interface Cart" });
    const grids = definition.querySelectorAll(".type-def-members");
    expect(grids).toHaveLength(1);
    const cells = [...(grids[0]?.children ?? [])].map((cell) => [cell.className, cell.textContent]);
    expect(cells).toEqual([
      ["type-def-field", "  id"],
      ["type-def-type", "CartId"],
      ["type-def-field", "  coupon?"],
      ["type-def-type", "string"],
      ["type-def-field", "  readonly createdAt"],
      ["type-def-type", "Date"],
    ]);
  });

  it("draws `readonly` as a quieter prefix inside the name cell", () => {
    renderCart();
    const prefix = screen.getByText("readonly");
    expect(prefix.className).toBe("type-def-readonly");
    expect(prefix.parentElement?.className).toBe("type-def-field");
  });

  it("lists inherited members under a quiet `from <Type>` for each base, in the same grid", () => {
    const client = testQueryClient();
    client.setQueryData(engineKey("type_definition", { project: P, key: ORDER }), order);
    renderWithClient(<TypeDefinition project={P} typeKey={ORDER} row="send:0" depth={1} openTypes={{}} onToggle={() => undefined} />, client);
    const definition = screen.getByRole("group", { name: "interface Order" });
    const grids = definition.querySelectorAll(".type-def-members");
    expect(grids).toHaveLength(1);
    const cells = [...(grids[0]?.children ?? [])].map((cell) => [cell.className, cell.textContent]);
    expect(cells).toEqual([
      ["type-def-field", "  id"],
      ["type-def-type", "string"],
      ["type-def-from", "from OrderDraft"],
      ["type-def-field", "  customerId"],
      ["type-def-type", "string"],
      ["type-def-field", "  email"],
      ["type-def-type", "string"],
      ["type-def-from", "from Address · ambiguous target"],
      ["type-def-from", "from Stripe.MetadataParam · stripe"],
      ["type-def-from", "from Error"],
    ]);
    // A declared base reads as a name in the sub-heading; it doesn't open there.
    expect(screen.queryByRole("button", { name: "OrderDraft" })).toBeNull();
  });

  it("opens an inherited member's type under its own row", () => {
    const client = testQueryClient();
    const linked: TypeDefinitionModel = {
      ...order,
      inherited: [{ from: { text: "OrderDraft", spans: [] }, members: [{ ...property("cart", "Cart"), type: { text: "Cart", spans: [{ start: 0, end: 4, style: "declared", target: CART }] } }] }],
    };
    client.setQueryData(engineKey("type_definition", { project: P, key: ORDER }), linked);
    client.setQueryData(engineKey("type_definition", { project: P, key: CART }), cart);
    const toggled: string[] = [];
    renderWithClient(
      <TypeDefinition
        project={P}
        typeKey={ORDER}
        row="send:0"
        depth={1}
        openTypes={{ "send:0/from0/0": 0 }}
        onToggle={(row) => {
          toggled.push(row);
        }}
      />,
      client,
    );
    expect(screen.getByRole("group", { name: "interface Cart" })).toBeTruthy();
    screen.getByRole("button", { name: "Cart" }).click();
    expect(toggled).toEqual(["send:0/from0/0"]);
  });

  it("names a member as written, keeping `?` and naming signatures", () => {
    expect(memberName({ name: "coupon", kind: "property", optional: true, readonly: true, default: false })).toBe("coupon?");
    expect(memberName({ kind: "call", optional: false, readonly: false, default: false })).toBe("()");
    expect(memberName({ kind: "construct", optional: false, readonly: false, default: false })).toBe("new ()");
  });
});
