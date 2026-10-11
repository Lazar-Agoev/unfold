import { describe, expect, it } from "vitest";
import type { Resource, TypeDefinition, TypeMember, TypeText } from "../../ipc/generated";
import { COLUMNS_LISTED, columnTypeText, listedColumns, markText, tableColumns } from "./columns";

const plain = (text: string): TypeText => ({ text, spans: [] });
const member = (name: string, type: string, extra: Partial<TypeMember> = {}): TypeMember => ({
  name,
  kind: "property",
  optional: false,
  readonly: false,
  default: false,
  type: plain(type),
  ...extra,
});

/** `interface Order extends OrderDraft`: its own fields, a method (not a column), and the draft's fields. */
const order: TypeDefinition = {
  key: "src/types.ts#interface:Order",
  kind: "interface",
  name: "Order",
  path: "src/types.ts",
  line: 20,
  language: "typescript",
  members: [member("id", "string"), member("status", "OrderStatus"), member("total", "() => number", { kind: "method" }), member("createdAt", "Date", { optional: true })],
  inherited: [{ from: plain("OrderDraft"), members: [member("customerId", "string"), member("email", "string")] }],
  uses: 4,
};

const table = (extra: Partial<Resource>): Resource => ({ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 0], author: "agent:claude", ...extra });

describe("a table's columns", () => {
  it("are its type's fields, own then inherited, with the board's marks laid on by name", () => {
    const columns = tableColumns(
      table({ type: order.key, columns: [{ name: "id", key: "primary" }, { name: "customerId", key: "foreign", references: "customers.id" }] }),
      order,
    );
    expect(columns.map((column) => [column.name, columnTypeText(column), column.optional, markText(column), column.from])).toEqual([
      ["id", "string", false, "PK", undefined],
      ["status", "OrderStatus", false, "", undefined],
      ["createdAt", "Date", true, "", undefined],
      ["customerId", "string", false, "FK → customers.id", "OrderDraft"],
      ["email", "string", false, "", "OrderDraft"],
    ]);
  });

  it("list a field the type redeclares once: its own, else the nearest base's", () => {
    const redeclared: TypeDefinition = {
      ...order,
      members: [member("id", "string"), member("email", "Email")],
      inherited: [
        { from: plain("OrderDraft"), members: [member("email", "string"), member("customerId", "string")] },
        { from: plain("Draft"), members: [member("customerId", "number"), member("note", "string")] },
      ],
    };
    expect(tableColumns(table({ type: order.key }), redeclared).map((column) => [column.name, columnTypeText(column), column.from])).toEqual([
      ["id", "string", undefined],
      ["email", "Email", undefined],
      ["customerId", "string", "OrderDraft"],
      ["note", "string", "Draft"],
    ]);
  });

  it("wait for the type's definition, and are what a table lists itself as written", () => {
    expect(tableColumns(table({ type: order.key }), null)).toEqual([]);
    const own = tableColumns(table({ columns: [{ name: "sku", type: "text", key: "unique" }, { name: "note", optional: true }] }), null);
    expect(own).toEqual([
      { name: "sku", type: "text", optional: false, key: "unique" },
      { name: "note", optional: true },
    ]);
    expect(own.map(markText)).toEqual(["unique", ""]);
    expect(tableColumns({ kind: "queue" }, order)).toEqual([]);
  });

  it("list up to 12, then 'N more', unless all are shown", () => {
    const many = Array.from({ length: COLUMNS_LISTED + 3 }, (_, index) => ({ name: `c${index}`, optional: false }));
    expect(listedColumns(many, false).shown).toHaveLength(12);
    expect(listedColumns(many, false).hidden).toBe(3);
    expect(listedColumns(many, true)).toEqual({ shown: many, hidden: 0 });
    expect(listedColumns(many.slice(0, 12), false).hidden).toBe(0);
  });
});
