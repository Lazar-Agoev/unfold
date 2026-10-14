# Diagrams: pages that read well

A page answers one question for one reader. Choose its level first, then which cards to
expand, then the frames that make it read in a few pieces.

## Levels of detail
| Level | The page shows | Cards | In ts-orders |
| --- | --- | --- | --- |
| System | A zone; a box per app with its entrances; the queues and stores between them | Compact, or frames collapsed | "How do the apps talk?" |
| Service | One app's entrances and the code they share | Compact; one or two expanded | "What does the Server do?" |
| Path | One entrance, the code it reaches, and what it talks to | The one or two that matter expanded; a call opened in place where it helps | "How is an order placed?" |
| Function | One card | Expanded, with 3 to 7 combined steps | "What does createOrder do?" |
| Data model | Types or tables and their links | Fields | data-models topic |

One page, one level. When a question needs two, make two pages (edit_pages adds one).

## Placing cards
- add_cards puts modules, functions, classes, types, apps, and entrances on a page by
  key (from search, list_entrances, read_code, or get_relations). A card already on
  the page is kept, and its id returned. frame puts new cards in a frame; near places
  a card beside an item.
- follow brings neighbors: callees up to 3 levels, callers up to 2, and on a type card
  the types its fields link to, up to 2. It stops at library and unknown calls and at
  40 cards per card; the result says where.
- Follow one or two levels, then remove_items what doesn't serve the question:
  loggers, validators, small helpers.
- create_board with trace (an entrance's key, such as
  `entrance:server/place-an-order`) puts the entrance and its code on the first page,
  compact.

## Compact and expanded
set_view (or add_cards' view) makes a card "compact" or "expanded".
- **Compact**: the name, file:line, and description. For context and overviews.
- **Expanded**: the drawn flow. Call boxes in the order they run (`▸` a project
  call), decisions, loops, tries, exits, and `⋯ N lines` ticks for code that isn't a
  block. Library and unknown calls sit in thin rows. Arrows leave from the call that
  makes them.
- **Apps and entrances**: compact, the name with `run` or `on`; expanded, their code.
- **Types** always show their fields. A class shows its fields compact and its
  class-level code expanded.
- Expanding changes sizes: arrange_page afterwards.

## Shaping a card
A compiled flow is code-shaped. shape_card turns it into the few named steps a person
would draw, in one call per card and one undo step. Read the card with read_code
first: it lists each block's short label and full key. The result is the card as the
person now sees it.
- **title** names the card; **description** says in one sentence what it does and why.
- **names** `{block: text}`: any block (a call, a branch, a loop, an except part, an
  exit, a callback). Name only what the code doesn't say: `orderService.place` needs
  none. Ask a branch as a yes-or-no question whose yes is the written test's yes.
  Naming a library or unknown call lifts it out of its thin row into a step; it stays
  `?`. null takes a name off.
- **combine** `[{steps, title, collapsed}]`: consecutive steps of one body (the
  function's own, a branch, a loop body, an except part), or one whole control block.
  Never overlapping or nested; collapsed by default. A combined step keeps its ways out
  (a return still goes to End, a throw to its handler or the caller) and counts the
  unknown calls inside.
- **open** `[[blocks]]`: calls or callbacks drawn in place, each a path of 1 to 3,
  outermost first, each next block a call of what the one before opens. It replaces
  what the card opens; `[]` closes them. Open the one or two calls a reader needs;
  follow the rest to their own cards.
- Names and combined steps apply page-wide, by key: wherever that code is drawn,
  inside other cards' opened calls too.
- A refusal lists the blocks you can choose from.

In ts-orders, createOrder (src/routes/orders.ts:25):
```
shape_card({board, page, card: <createOrder's card id>,
  title: "Place an order",
  description: "Places the order now, or queues it when the client asks for an async reply.",
  names: {"call:res.status().json": "Reply 201 with the order"},
  combine: [{steps: ["if[0]"], title: "Queue it if the client wants an async reply"}],
  open: [["call:orderService.place"]]})
```

## Proposed steps
A proposed step is an intended change, not code: a title, a reason, and where it would
go. It is drawn dashed as PROPOSED; the compiled path stays as it is.
- Only when the person asks for an intended change. Explaining code never adds one.
- propose with after (the key of a step, from read_code) or start (the key of a
  definition or a part: the start of its body). It applies page-wide.
- No status words. Once the code exists, remove_items takes it off.
- When check finds its block missing or held, propose with its id and a new after or
  start moves it.
- In ts-orders, asked for a confirmation email on the direct path:
  `propose({board, page, proposals: [{after:
  "src/routes/orders.ts#createOrder/call:orderService.place", title: "Email a
  confirmation", reason: "Queued orders get one from processOrder; these don't."}]})`.

## Frames
A frame is a named, described box around items on a page.
- look "box" for a unit of several functions ("Request checks") or a service
  ("Server"); look "zone", a tinted area, for what holds services.
- icon: api, worker, schedule, cli, web, library, or system.
- Members are cards, resource cards, notes, or frames. An item is in one frame at
  most; frames nest three deep (zone › box › box): pass parent.
- direction "right" or "down": how arrange_page lays out its members.
- Collapsed, a frame is one node that says what it holds, and lines to its members
  attach to it. Collapse only when the person asked, your brief said, or check finds
  the page too dense.
- Code two apps share goes in a library box of its own, or outside both frames
  (OrderService.place, called from createOrder and from processOrder).
- frame_cards with an existing id updates it. remove_items on a frame keeps its
  members unless keepMembers is false.

## Layout
arrange_page lays out from the inside out: each frame's members in its direction, then
the page with each frame as one block. Type cards follow the types that link to them;
tables sit between the code that writes and the code that reads them. Pass frame to
arrange one frame's inside. Then moves: at [x, y], rightOf, or below. Arrange, then
nudge; never hand-place every card.

## Density
About 30 items on a page, 12 in a frame, 12 drawn steps in an expanded card. Over that,
in this order: combine steps, keep context cards compact, collapse frames the page
isn't about, remove cards that don't serve the question, split the page.

## Other repositories
A page shows one project. Two apps in one project are two frames on one page, joined by
connections that cite both sides (connections topic). A service in another repository
is an external resource card, with a connection that cites the client code.

## Reviewing a page
check returns the page's health (repairs topic) and these checks, each with the tool
that fixes it:
| Check | Finds | Fix |
| --- | --- | --- |
| overlap | Items that overlap | arrange_page, or moves |
| crossings | Lines that cross, or a line through a frame that holds neither end | arrange_page; a frame's direction; moves |
| density | Over the limits above | combine, compact, collapse, split |
| missing words | Frames, combined steps, and resource cards without a description | describe |
| evidence | Resource cards (not actors) and connections that cite nothing | add_resources or connect again, with anchors |
| strays | Cards and resource cards with no line, connection, or frame | connect, frame_cards, or remove_items |

get_screenshot draws the page as the person sees it, offscreen. A large page comes
smaller: take a frame's or a card's image (item), at scale 2 for small text. Judge what
check can't: does the page answer the question, does it read in one direction, is
anything authored or proposed made to look compiled.
