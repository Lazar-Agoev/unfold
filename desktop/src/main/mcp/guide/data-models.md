# Data models: types and tables

Two things answer two questions:
- **Type cards** answer "what shapes does the code pass around?" They are compiled:
  fields, written types, and the arrows between types all come from the code.
- **Tables** answer "what's in the database?" A table is a resource card you add. When
  the code declares its shape, back it by that type: its columns are then compiled too,
  and you add only its name and key marks.

When the code keeps data in memory (ts-orders keeps orders in a Map in
OrderRepository), say so in chat and in the table's description.

## Finding types
- search({query, kinds: ["types"]}) lists interfaces, type aliases, enums, and classes
  (dataclasses, TypedDicts, pydantic models, and NamedTuples are classes).
- read_code on a type: its members with written types and what each type name links
  to, inherited members by base, and how many places use it.
- get_relations on a type: the places that use it, with extends and implements marked.

## Type cards
- add_cards: a type card lists its fields (name, written type, optional, readonly, a
  default), an enum's values, or an alias's type.
- follow: {types: 1} adds the types its fields link to (up to 2 levels). Unfold draws a
  type arrow from the field to each linked type card. Type arrows are compiled: never
  connect over them.
- A type that extends another lists the base's fields under it; implements adds none.
- A field's type that isn't a declared type stays as written: a library's type, a
  built-in, or unknown with a reason. In the typed-lines fixture, `price: Money`
  doesn't link, because `Money = Decimal` is a variable, not a type declaration. Say so
  in chat; never guess.

## Tables backed by a type
```
add_resources({board, page, resources: [{
  kind: "table", name: "orders",
  type: "src/types.ts#Order",
  anchors: [{key: "src/db/repository.ts#OrderRepository"}],
  columns: [{name: "id", key: "primary"}],
  description: "Orders by id, in memory for each process."}]})
```
- **Columns are the type's fields**, inherited ones included: Order's id, status, and
  createdAt, and OrderDraft's customerId, email, and items.
- **columns only mark fields**: {name, key: primary | foreign | unique, references}. A
  name that isn't a field is refused, and the message lists the fields.
- **name** is the technical name, as the code writes it. It must appear in the type's
  or the anchors' text: "orders" appears in OrderRepository (`private orders = new
  Map…`).
- **anchors**: even a typed table needs one, where the table is created, declared, or
  used (the repository, the model's table name, a migration).
- When the type changes, the columns follow and the marks go to review; when it's
  gone, the table is missing (repairs topic).

## Tables with columns of their own
When the code declares columns without annotations (Django's `models.CharField()`,
Drizzle's `pgTable("orders", {…})`), or only outside the code (a SQL migration, a
Prisma schema), list the columns yourself: {name, type, key, references, optional},
with the type as written there.
- Anchor it to the model's code or to an evidence file's lines. Every column name must
  appear in the anchors' text; a column none mentions is refused.
- Evidence files: ask the person first, then update_project_settings with
  add_evidence_pattern (setup topic), compile_project, read_file for the lines, and
  anchor `{file: "prisma/schema.prisma", lines: [12, 20]}`.

## Relationships
- **Compiled**: a field whose type links to another type card draws a type arrow.
  Nothing to draw.
- **By value**: when a column holds another table's key as a plain value, connect kind
  relates from the table holding it to the other, with fromColumn, toColumn, and a
  cardinality (many-to-one: many orders per customer). Mark the column foreign, with
  references. Never relate tables the types already link.

## What stays authored
Unfold generates no ERD. Tables, columns, key marks, and relationships appear only
through add_resources and connect, each citing where the code or a schema says so. A
typed table draws no type arrows: to show the compiled links between types, add the
types as cards.

## Reads and writes
connect writes from the call that inserts, updates, or deletes, and reads from the call
that queries, each with its step. In ts-orders, OrderService.place writes orders at
`this.repo.insert(order)` (src/services/order-service.ts:22). One page can show the code
and the tables it touches.

## Worked example
The typed-lines fixture's Python shop (py/src/shop/models.py). "Show me the shop's data
model":
- search({query: "shop", kinds: ["types"]}) finds CartItem, Cart, Address, Customer,
  and Receipt, and the types in accounts.py.
- add_cards places Cart and Customer with follow: {types: 1}, which brings CartItem
  and Address. Unfold draws `items: list[CartItem]` and `address: "Address"`.
- read_code on CartItem shows `price: Money` unknown; the chat says so.
- Review, finish_work. Every field and arrow is compiled; you wrote the titles and
  descriptions.

If the person then says "these are tables in Postgres", add a table backed by each
type, anchored where the code or a migration names it.
