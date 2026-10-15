# Diagrams: pages that read well

A page answers one question for one reader, from compiled code. Choose its level first,
then the entrances to trace and how deep, then which cards to expand. What you add
yourself comes last, and only where code can't say it.

## Levels of detail
| Level | The page shows | Cards | In ts-orders |
| --- | --- | --- | --- |
| System | Each app's entrances traced to their first project calls; the stores and queues between apps as resource cards with connections; a box per app; a zone only when several app boxes share the page | Compact, or frames collapsed | "How do the apps talk?" |
| Service | One app's entrances traced, and the code they share | Compact; one or two expanded | "What does the Server do?" |
| Path | One entrance, the code it reaches, and what it talks to | The one or two that matter expanded; a call opened in place where it helps | "How is an order placed?" |
| Function | One card | Expanded, with 3 to 7 combined steps | "What does createOrder do?" |
| Data model | Types or tables and their links | Fields | data-models topic |

One page, one level. When a question needs two, make two pages (edit_pages adds one).

## Tracing entrances
The spine of a page is its entrances and the code they reach. Unfold draws it: a start
arrow from each entrance to the code it starts, and a call arrow for each project call
between cards. Trace it in one call:
- create_board with trace, one key or a list (apps or entrances, such as
  `entrance:server/place-an-order`), puts each entrance and the code it starts on the
  new board's first page, compact.
- On a page that exists, add_cards with the entrances' keys and follow {callees: 1}
  or {callees: 2} puts each entrance, the code it starts, and (at 2) what that calls
  on the page, with their arrows.
- No entrance for the code? Propose them first (setup topic).

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
- A titled card shows its code name beside the title.
- Expanding changes sizes: arrange_page afterwards.

## What you add, and how much
Compiled code explains itself. Add the fewest words a newcomer needs, and only what
code can't show.
| What | When | Never |
| --- | --- | --- |
| Title (shape_card) | The code name says nothing about what the card does (`main`, `run`, `handle`) and the page is about it: about 3 titled cards on a page at most | On a card whose code name says what it does |
| Description (shape_card, describe, add_resources) | The 1 to 3 cards the page is about (usually the expanded ones), every resource card, every connection (connect needs one) | On context cards; on entrance cards (their on and about say it) |
| Names, combined steps (shape_card) | Expanded cards, where the drawn flow shows them; shape each once | On a compact card |
| Resource cards, connections | Tables, queues, storage, and outside services the code talks to, each citing code | For what Unfold already draws |
| Frames (frame_cards) | Optional: a box per app when two or more apps share the page, or around three or more cards that work as one unit; a zone only to hold several app boxes | Around a single card, or a card and its note |
| Notes (add_notes) | At most 2: a caveat or an open question a reader would miss | Schedules, reading order, or what a card, entrance, or connection already says; status (live, broken, counts, dates) unless the person asks for it |

What you found outside the code (production checks, logs, databases) goes in the chat,
not on the page, unless the person asks for it there.

## Shaping a card
A compiled flow is code-shaped. shape_card turns it into the few named steps a person
would draw, in one call per card and one undo step. Shape only the expanded cards the
page is about, each once; leave compact cards unshaped. Read the card with read_code
first: it lists each block's short label and full key. The result is the card as the
person now sees it.
- Short labels name the card's own blocks. Name a block of a call the card opens in
  place by its full key (read_code on that call lists them); in **open**, each next
  block may be a short label of what the one before opens.
- **title** renames the card only when its code name says nothing (`main`, `run`);
  Unfold shows the code name beside it. **description** says in one sentence what
  it does and why. null takes either off. An app's or entrance's card, or a type card,
  draws no flow and takes only a description; leave an entrance's off: its on and
  about say it.
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

In ts-orders, createOrder (src/routes/orders.ts:25) gets no title: its name already
says what it does.
```
shape_card({board, page, card: <createOrder's card id>,
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
A frame is a titled box around items on a page. Frames are optional: Unfold's arrows
already show what calls what.
- look "box" for an app when two or more apps share the page ("Server"), or around
  three or more cards that work as one unit; look "zone", a tinted area, only to hold
  several app boxes on a system page.
- Never around a single card, or around one card and its note.
- A short title, 1 to 4 words; a description only when the title can't say it.
- icon: api, worker, schedule, cli, web, library, or system.
- Members are cards, resource cards, notes, or frames. An item is in one frame at
  most; frames nest three deep (zone › box › box): pass parent.
- direction "right" or "down": how arrange_page lays out its members.
- Collapsed, a frame is one node that says what it holds, and lines to its members
  attach to it. Collapse only when the person asked, your brief said, or check finds
  the page too dense.
- Code two apps share stays outside both app boxes (OrderService.place, called from
  createOrder and from processOrder), or goes in a library box when three or more
  cards share it.
- frame_cards with an existing id updates it. remove_items on a frame keeps its
  members unless keepMembers is false.

## Layout
arrange_page lays out from the inside out: each frame's members in its direction, then
the page with each frame as one block. Type cards follow the types that link to them;
tables sit between the code that writes and the code that reads them. Pass frame to
arrange one frame's inside. arrange_page once, then arrange_page with only moves to
nudge (at [x, y], rightOf, or below): a call with only moves moves those items and
arranges nothing, so nudges stick. Any other call arranges first, which resets earlier
moves. Never hand-place every card.

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
| missing words | Resource cards without a description | describe, or add_resources with the card's id |
| evidence | Resource cards (not actors) and connections that cite nothing | add_resources or connect again, with anchors |
| strays | Cards and resource cards with no line (a call arrow, a start arrow, or a connection) to the rest of the page; a frame doesn't count | add_cards with follow (the code an entrance starts, a caller, a callee); connect a store or outside service with a cited connection; or remove_items |
| too many words | Your titles on more than about 3 cards, your descriptions on more than about 3 compact cards (type cards don't count), more than 2 of your notes; people's words never count | shape_card with title or description null; remove_items for notes |

get_screenshot draws the page as the person sees it, offscreen. A large page comes
smaller: take a frame's or a card's image (item), at scale 2 for small text. Judge what
check can't: does the page answer the question, does it read in one direction, is
anything authored or proposed made to look compiled, or standing in for an entrance.
