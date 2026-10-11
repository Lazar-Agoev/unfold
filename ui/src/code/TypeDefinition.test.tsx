// @vitest-environment jsdom
import "../test/dom";
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TypeDefinition as TypeDefinitionModel } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { renderWithClient, testQueryClient } from "../test/render";
import { TypeDefinition, memberName } from "./TypeDefinition";

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
  uses: 6,
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

  it("names a member as written, keeping `?` and naming signatures", () => {
    expect(memberName({ name: "coupon", kind: "property", optional: true, readonly: true, default: false })).toBe("coupon?");
    expect(memberName({ kind: "call", optional: false, readonly: false, default: false })).toBe("()");
    expect(memberName({ kind: "construct", optional: false, readonly: false, default: false })).toBe("new ()");
  });
});
