# Diagrams: pages that read well

A page answers one question for one reader. Choose its level of detail first, then
which cards to expand, then the frames that make it read in a few pieces. Everything
on it starts from compiled code; you choose, expand, frame, and arrange.

## Levels of detail
| Level | The page shows | Expansion | In ts-orders |
| --- | --- | --- | --- |
| A system | A zone around the services; a box per service with its entrances; queues and stores between them | All compact | "How do the services talk?" |
| A service | One application's entrances and the code they share | Compact, one or two cards expanded | "What does the API do?" |
| A path | An entrance, its code, and the functions they reach | The one or two that matter expanded; the rest compact | "How is an order placed?" from POST /orders |
| A function | One card | Expanded | "What does createOrder do?" |
| A data model | Types or tables and the links between them | Fields (type declarations take no expansion; classes stay compact) | "What's in an order?" (data-models topic) |

One page, one level. When the question needs two, make two pages (edit_pages adds a
page) rather than one page that does both.

## Compact and expanded
Each card is compact or expanded; set_view sets it (view "compact" or "expanded"),
and a page's default view covers cards without their own.
- **Compact**: a function card is one node with its name, file:line, and description.
  All the calls from a compact card to another card merge into one arrow ("3 calls").
  Use it for context and overviews.
- **Expanded**: the function's drawn flow: its call boxes in the order they run,
  decisions, loops, tries, exits, and gap ticks for code left out. Arrows leave from
  the call that makes them. Expand the one or two functions the page is about.
- **Apps and entrances**: compact, a node with its name; expanded, what starts it and
  the code it starts at.
- **Type and table cards** keep their fields and columns; they take no view. A
  class's card shows its fields compact and its class-level code expanded.
- add_cards takes view too. Expansion changes sizes: arrange_page afterwards.

## Placing cards
- add_cards puts functions, methods, classes, types, and entrances on a page by key
  (from search, list_entrances, read_code, or get_relations). A card already on the
  page is kept, and its id is returned.
- follow adds a call tree: callees up to 3 levels, callers up to 2, through resolved
  calls and constructions only. On a type card, types up to 2 adds the types its
  fields link to. Following stops at external and unresolved calls and at 40 cards
  per card; the result says where it stopped and why.
- near places a card beside an item; frame puts the cards a spec adds into a frame.
- Follow one or two levels, then take off (remove_items) what doesn't serve the
  question: loggers, validators, and small helpers usually don't.
- create_board with trace puts an entrance and its handlers on the first page.

## Reading a card's code
read_code prints a definition's compiled tree: each block with its line, its short
label, and its full key, and how the card draws it (▸ a project call box, ? an unknown
call). Use it to name what a block really does, and to find the keys propose, connect,
and read_call take. It reads only code Unfold compiled; otherwise it says the file
changed.

## Proposed steps
A proposed step is an intended addition, not code: a title, an optional reason, and
where it would go, drawn dashed green as PROPOSED beside the compiled path, which
stays as it is.
- Only when the person asks for an intended change. Explaining existing code never
  adds proposals, and never invent behavior the code lacks.
- propose with after (the key of the block it follows, from read_code), or start
  (the key of a definition or a part, for the start of its body). Not after a
  return, throw, break, or continue. It applies page-wide.
- No status, completion, or "would call". When the code exists, remove_items takes the
  proposal off (that never edits code).
- When check finds its anchor missing or held for review, propose with its id and a
  new after or start moves it, or remove_items takes it off.
- In ts-orders, asked for a confirmation email on the direct path: propose
  {after: <the key of createOrder's call to OrderService.place>, title: "Email a
  confirmation", reason: "The queued path emails a confirmation; placing it now
  doesn't."}.

## Frames
A frame is a named, described box around items on a page.
- **look "box"**: an outlined box with a title bar, for a unit of several functions
  ("Request checks") or a service ("Order API").
- **look "zone"**: a tinted background with a large title, for an area that holds
  services (a system, a layer, a repository).
- **icon** says what it is: api (an HTTP service), worker (queue workers), schedule
  (scheduled jobs), cli (commands), web (a front end), library (shared code), system
  (a whole system, usually a zone).
- **Members**: cards, resource cards, notes, or frames of the page. An item is in one
  frame at most. Frames nest at most three deep (zone › box › box): pass parent. An
  item joins a frame from no frame, from the frame itself, or from its parent; take
  it out of any other frame first.
- **direction** "right" or "down": how arrange_page lays its members out.
- **Collapsed**, a frame is one node that says what it holds ("2 entrances ·
  3 functions · 1 table"), and lines to its members attach to it and merge ("3 calls",
  "queue · 2 connections"). Frames start expanded. Collapse one when the person asked,
  when check finds the page too dense, or when your brief said so.
- **Shared code**: an item is in one frame, so code two services both call (in
  ts-orders, OrderService.place, from the API and the orders worker) goes in a box of
  its own with icon library, or stays outside the service frames.
- **Updating**: frame_cards with an existing id updates it; members it no longer
  lists go to the frame that holds it. remove_items on a frame keeps its members
  unless keepMembers is false.

## Layout and direction
- arrange_page lays out from the inside out: each frame's members by call depth and
  connections in the frame's direction, then the page with each frame as one block.
  Type cards come after the types that link to them; tables sit between the code that
  writes them and the code that reads them; attached notes go under their items.
- One main direction: requests left to right, data top to bottom. Pass frame and
  direction to set one frame's direction and arrange only its inside; direction alone
  lays out the page's top level that way for this call.
- moves, after arranging: at [x, y], or rightOf or below another item. Don't
  hand-place every card: arrange, then nudge. The same page always arranges the same
  way.

## Density
About 30 visible items on a page (cards, resource cards, and collapsed frames), 12 in a
frame, and 12 visible steps in an expanded card. Over that, in this order:
- keep context cards compact (set_view with view "compact");
- collapse the combined steps the page isn't about;
- collapse frames the page isn't about;
- take off cards that don't serve the question;
- split the page (edit_pages adds one).

## Other repositories
A page shows one project. Two applications in one project are two frames on one page,
and a call from one to the other is an http or queue connection you draw, citing both
sides (connections topic). A service in another repository is an external resource
card, with a connection to it that cites the client code.

## Recipes
Adapt them; the tools are listed in the order you'd call them. When the person pasted a
page link (Copy link on a page), pass it as page to every tool: that page, not the one
in view, is the one to build.

### Explain a request
"How does placing an order work?" One page: the entrance, its handlers, and the
services they call; one or two cards expanded, the rest compact.
- list_entrances({filter: "orders"}) finds POST /orders (application server) and the
  orders worker; read_code on the entrance lists its handlers: requireUser,
  validateBody, createOrder.
- read_code and get_relations on createOrder and on what it calls.
- Brief. Then create_board({project, title: "Order placement", pages: ["Placing an
  order"], trace: <POST /orders' key>}), or use the page the person linked.
- add_cards for what the handlers reach that matters; set_view: createOrder expanded.
- frame_cards a "Request checks" box around requireUser and validateBody; describe
  the cards and frames.
- When the path crosses a queue or another service, add the worker and connect it
  (connections topic).
- arrange_page, check, get_screenshot, fix; finish_work.

### Service map
"Show me the services." A zone, and a box per application with its entrances, all
compact; connections between services; resource cards for queues and stores, and
external cards for services in other repositories.
- list_entrances for the project.
- add_cards for each application's entrances, frame_cards a box per application (icon
  api, worker, schedule, or cli) inside a zone.
- read_code on each entrance's handler: its calls list where services talk (external
  calls such as `ordersQueue.add` into bullmq, or `fetch`); read each one before
  drawing anything.
- add_resources for the queues and stores; connect producers and consumers
  (connections topic).
- A box with more than about 12 entrances: nest the minor ones (admin, health) in a
  collapsed box inside it.

### Async pipeline
"What happens after we enqueue?" Producers on the left, a queue card, workers on the
right; a page per queue when there are many.
- search({query: "queues", kinds: ["files"]}) and read_file find where the queues are
  created; read_code on the handlers lists the producers' calls (`ordersQueue.add`,
  external, bullmq); get_relations and list_entrances find the workers.
- add_cards for the producers and the worker entrances (follow callees 1 brings the
  handler), add_resources a queue card anchored where the queue is created, connect
  producer → queue and queue → worker.
- In ts-orders: createOrder adds to `orders` (src/routes/orders.ts:28), the orders
  worker runs processOrder, which adds to `emails` (src/jobs/process-order.ts:10).
  sendEmail's `outbox.add` (src/jobs/send-email.ts:8) is a Set, not a queue: reading
  the code tells you.

### Explain a call
"What does this call send?" Nothing changes on the page: answer in chat from the
line's contract (graph topic).
- get_current_view shows the selected arrow; read_call without call reads it;
  read_code on the types it names.

### Data model
"Which tables?" Tables backed by the code's types, so their columns and links are
compiled; relates only where the types don't link; then code cards with reads and
writes (data-models topic).
- search({query, kinds: ["types"]}), read_code on each type, add_resources (tables
  with type), connect (relates), add_cards for the code, connect (reads, writes).

### Data shapes
"What does the worker receive?" Type cards with their type arrows, beside the code that
sends and receives them.
- search({query, kinds: ["types"]}), add_cards (the types, follow types 1), and a
  connection that names the type it carries (connect with carries).

### Across repositories
"From the web app to the API." On the web app's page, the API is an external resource
card, and an http connection goes from the client's call to it.
- search the web app by name for the client function, read_code to find the call,
  add_cards the function, add_resources an external card for the API anchored to the
  client code, then connect kind http from the calling card (step: the call) to that
  card.

## Reviewing a page
check returns the page's health (repairs topic) and these page checks, each with its
items and the call that fixes it:
| Check | Finds | Fix |
| --- | --- | --- |
| overlap | Items that overlap | arrange_page, or moves |
| crossings | More than 3 line crossings, or a line through a frame that holds neither end | arrange_page; a frame's direction; moves |
| density | Over the limits above | view "compact", collapse, split |
| missing words | Frames, combined steps, and resource cards without a description | describe |
| evidence | Resource cards (not actors) and connections that cite nothing | add_resources or connect again, with anchors |
| strays | Cards and resource cards with no line, connection, or frame | connect, frame, or remove_items |

get_screenshot draws the page as the person sees it, offscreen. A large page is drawn
smaller, and the result says so: take a frame's or a card's image (item) to read it,
at scale 2 for small text. Look for what check can't judge: does the page answer the
brief's question, does it read in one direction, are titles clear, does
anything authored or proposed look compiled. Give a one-line verdict per section in
chat, then fix with targeted edits.
