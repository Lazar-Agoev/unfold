# Data models: types and tables

Two things answer two questions:
- **Type cards** answer "what shapes does the code pass around?" A type card is a
  compiled card: its fields, their written types, and the arrows between types all
  come from the code.
- **Tables** answer "what's in the database?" A table is a resource card. When the code
  declares its shape, back it by that type: its columns are then compiled too, and you
  add only the table's name and its key marks.

Use both when the person asks for both. When the code keeps its data in memory (ts-orders
keeps orders in a Map in OrderRepository), say so in chat and in the table's
description; don't present it as a database.

## Finding types
- search({query, kinds: ["types"]}) lists interfaces, type aliases, enums, and classes
  (dataclasses, TypedDicts, pydantic models, and NamedTuples are classes).
- read_code on a type: each member with its written type and what each name in it
  links to, the members it inherits by base, and how many places use it.
- get_relations on a type: the places that use it, with extends and implements marked.

## Type cards
- add_cards with view "diagram": the card lists the type's fields (name, written type,
  `?`, `readonly`, a default as ` = …`), an enum's values, or an alias's type. A card
  with 12 or more rows shows 8, then "N more".
- follow: {types: 1} on a type card adds the types its fields link to (up to 2 levels).
  Unfold draws a type arrow from the field's row to each linked type card of the same
  project, labeled with the written type. Type arrows are compiled: never connect over
  them.
- Inherited fields: a type that extends another (a TypeScript interface or class, a
  Python base class) lists the base's fields under "from <Base>". implements adds none:
  the class writes those fields itself. When the base has its own card on the page, the
  base's fields take their arrows from there.
- A field's type that isn't a declared type stays as written: a library's type
  (Stripe.PaymentIntent), a built-in (string, Date), or unresolved with a reason. In the
  typed-lines fixture, `price: Money` doesn't link, because `Money = Decimal` is a
  variable, not a type declaration. Say so in chat; never guess what it is.

## Tables backed by a type
```
add_resources({board, page, resources: [{
  kind: "table", name: "orders",
  type: "src/types.ts#interface:Order",
  anchors: [{key: "src/db/repository.ts#class:OrderRepository"}],
  columns: [{name: "id", key: "primary"},
            {name: "customerId", key: "foreign", references: "customers.customerId"}],
  description: "Orders by id, in memory for each process."}]})
```
- **Columns are the type's fields**: its properties and those it inherits through
  extends (methods aren't columns). `interface Order extends OrderDraft` gives id,
  status, and createdAt, then customerId, email, and items from OrderDraft. A table
  shows 12 columns, then "N more".
- **columns only mark fields**: {name, key: primary | foreign | unique, references}.
  A name that isn't a field is refused, and the message lists the fields. No type or
  optional on marks: those come from the code.
- **name** is the technical name, as the code writes it, and must appear in the
  verified text of the type or the anchors: "orders" appears in OrderRepository
  (`private orders = new Map…`).
- **anchors**: even a typed table needs one, where the table is created, declared, or
  used (the repository, the model's table name, a migration).
- **project**: the type's project, when it isn't the board's own.
- It follows the code: when the type changes, the columns change with it, and the
  marks go to review; when the type is gone, the table is missing (repairs topic).

## Tables with columns of their own
When the code declares columns without annotations (Django's `name =
models.CharField()`, Drizzle's `pgTable("orders", {…})`), or only outside the code (a
SQL migration, a Prisma schema), the table lists its columns itself:
{name, type, key, references, optional}, with the type as written there.
- Anchor it to the model's code, or to lines of an evidence file. Every column name
  must appear in the anchors' verified text; a column no anchor mentions is refused
  ("the column "customer_id" doesn't appear in its anchors"): cite where it's defined,
  or leave it out.
- Evidence files: ask the person first; update_project_settings with
  add_evidence_pattern (setup topic), compile_project, then read_file to find the
  lines, and anchor `{file: "prisma/schema.prisma", lines: [12, 20]}`.

## Relationships
- **Compiled**: a field whose type links to another type on the page draws a type
  arrow (`items: CartItem[]` from Cart to CartItem). Nothing to draw.
- **By value**: when a column holds another table's key as a plain value
  (customerId: string), draw connect kind relates from the table holding the value to
  the other, with fromColumn, toColumn, and a cardinality (many-to-one: many orders per
  customer). Mark the column foreign, with references. The two tables must differ, and
  columns of a typed table must be its fields.
- Don't relate tables the types already link.

## Reads and writes
connect writes from the line that inserts, updates, or deletes (head at the table), and
reads from the line that queries (head at the code), each with its step. In ts-orders,
OrderService.place writes orders at `this.repo.insert(order)`
(src/services/order-service.ts:22). A page can mix code cards and tables: "the order
flow and the tables it touches" is one page.

## Layout
arrange_page puts type cards after the types that link to them, and tables between the
code that writes them and the code that reads them. Data reads best top to bottom: pass
direction "down" for a frame of tables.

## Worked example
The typed-lines fixture's Python shop (py/src/shop/models.py) declares a dataclass, a
TypedDict, a pydantic model, and a NamedTuple. "Show me the shop's data model":
- search({query: "shop", kinds: ["types"]}) finds CartItem, Cart, Address, Customer,
  and Receipt (and the account types in accounts.py).
- add_cards places Cart and Customer in Diagram with follow: {types: 1}, which brings
  CartItem and Address. Unfold draws `items: list[CartItem]` from Cart to CartItem and
  `address: "Address"` from Customer to Address.
- read_code on CartItem shows `price: Money` unresolved; the chat says so.
- get_relations on Cart lists who uses it; a second page can place those functions
  beside the type.
- Review, finish_work. Every field, type, and arrow on the page is compiled; you wrote
  the titles and descriptions.

If the person then says "these are tables in Postgres", turn each type into a table
backed by it, anchored where the code or a migration names the table: you add the
names and key marks, and the columns stay compiled.
