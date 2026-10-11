# Diagrams: pages that read well

A page answers one question for one reader. Choose its level of detail first, then
each card's view, then the step groups and frames that make it read in a few pieces.
Everything on it starts from compiled code; you choose, group, name, and arrange.

## Levels of detail
| Level | The page shows | Views | In ts-orders |
| --- | --- | --- | --- |
| A system | A zone around the services; a box per service with its entrances; queues and stores between them | Diagram | "How do the services talk?" |
| A service | One application's entrances and the code they share | Diagram, one or two cards in Flow | "What does the API do?" |
| A path | An entrance, its handlers, and the functions they reach | Flow for the one or two that matter, Diagram for the rest | "How is an order placed?" from POST /orders |
| A function | One card | Flow, with 3 to 7 step groups | "createOrder has too many steps" |
| A data model | Types or tables and the links between them | Diagram (fields) | "What's in an order?" (data-models topic) |

One page, one level. When the question needs two, make two pages (edit_pages adds a
page) rather than one page that does both.

## Views: Diagram, Flow, Code
- **Diagram**: one compact node with the name, file:line, and description. All the
  calls from one Diagram node to another card merge into one arrow ("3 calls"); one
  call keeps its typed label. Use it for context and overviews.
- **Flow**: the function's steps as a diagram, "paths from written structure". Use it
  for the one or two functions the page is about.
- **Code**: the verified source, with calls as links. Use it only when exact lines
  matter.
- **Types**: a type card shows its fields in Diagram (a type declaration has no paths,
  so Flow shows its fields too; a class's Flow lists its members). Code is its source.
- **Entrances**: an entrance card is a node marked with its kind (HTTP, WORKER,
  SCHEDULE, COMMAND, EVENT, PROCESS) in Diagram, and lists its handlers otherwise.
- set_view sets each card's view and the page's defaultView (the view of cards
  without their own). The person's lens (⌘1 Diagram, ⌘2 Flow, ⌘3 Code) changes what
  they see, never the page; get_screenshot draws each card's own view, as a reader
  opening the page sees it.
- Views change card sizes: arrange_page after set_view.

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

## Step groups
A step group gathers consecutive steps of one card under a title. In Flow it's a
titled region; collapsed, it's one step with the title, its number of steps, and a
returns or throws mark when something inside can return or throw. In Code it's the
magenta rail. One collapse state serves both.
- **The rule**: whole, consecutive statements with the same parent and the same
  branch (a function body, an if's then or else, a loop body, a try, catch, or
  finally). Not the card's whole block, never nested, never overlapping.
- **Lines or steps**: read_code with view "flow" lists each step's lines and key.
  Pass lines [from, to] (Unfold takes the whole statements they cover, and warns when
  that's more than you asked) or steps (the statements' keys).
- **A refusal teaches**: a range that starts at an if and ends inside it is refused,
  and the message lists the statements there; take the branch's lines, or the whole
  if.
- **How many**: aim for a Flow that reads in fewer than about 8 steps: 3 to 7 groups
  for a long function, fewer for a short one. Then label every step, those inside the
  groups too (writing topic).

In ts-orders, createOrder (src/routes/orders.ts:25–35) draws eight steps between Start
and End. Two groups make it read in four:
```
group_steps({board, page, card: "create-order", groups: [
  {title: "Queue it for a worker, reply 202", lines: [28, 30]},
  {title: "Place it now and reply 201", lines: [32, 34]}]})
```
Lines 28–30 are the if's then branch (job, the 202 reply, the return); 32–34 are the
three statements after the if.

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
  its own with icon library, or stays outside the service frames. frame_cards warns
  when it sees this.
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
- moves, after arranging: at [x, y], or rightOf or below another item, for cards,
  frames, resource cards, and notes. Don't hand-place every card: arrange, then nudge.
- The same page always arranges the same way. Arrange after adding cards or changing
  views, then check.

## Density
About 30 visible items on a page (cards, resource cards, and collapsed frames), 12 in a
frame, and 12 visible steps in a Flow card. Over that, in this order:
- show context cards in Diagram instead of Flow or Code;
- group a long Flow's steps and collapse the groups the page isn't about;
- collapse frames the page isn't about;
- take off cards that don't serve the question;
- split the page (edit_pages adds one).

## Pages across projects
A page can hold cards from several projects: pass project in add_cards. The graph has
no arrows between projects, so a client's call to another project's route is an http
connection you draw (connections topic). Frame each project (a zone per repository, a
box per application). A card from a project that isn't connected shows as a
placeholder; say so rather than remove it.

## Recipes
Adapt them; the tools are listed in the order you'd call them.

### Explain a request
"How does placing an order work?" One page: the entrance, its handlers, and the
services they call; one or two cards in Flow with step groups and labels, the rest in
Diagram.
- list_entrances({filter: "orders"}) finds POST /orders (application server) and the
  orders worker; read_code on the entrance lists its handlers: requireUser,
  validateBody, createOrder.
- read_code and get_relations on createOrder and on what it calls.
- Brief. Then create_board({project, title: "Order placement", pages: ["Placing an
  order"], trace: <POST /orders' key>}).
- add_cards for what the handlers reach that matters (OrderService.place in Flow);
  set_view: defaultView "diagram", createOrder in Flow.
- Simplify the Flow cards (below); frame_cards a "Request checks" box around
  requireUser and validateBody; describe the cards and frames.
- When the path crosses a queue or another service, add the worker and connect it
  (connections topic).
- arrange_page, check, get_screenshot, fix; finish_work.

### Simplify a function
"Too many steps." One card in Flow with a few step groups, and every step labeled.
- read_code({key, view: "flow"}) for the steps, their lines, and keys.
- Brief: the groups and what each achieves.
- group_steps, then label_steps (writing topic), then set_view to show the card in Flow
  and collapse the groups.
- check (unlabeled steps leave out those a collapsed group hides, which still show
  when the person expands it), get_screenshot({item: <the card's id>}).

### Service map
"Show me the services." A zone, and a box per application in Diagram with its
entrances; connections between services; resource cards for queues and stores.
- list_projects, list_entrances for each project.
- add_cards for each application's entrances (view "diagram"), frame_cards a box per
  application (icon api, worker, schedule, or cli) inside a zone.
- search text for where services talk (`.add(`, `new Queue(`, `fetch(`, `.delay(`),
  read each hit before drawing anything.
- add_resources for the queues and stores; connect producers and consumers
  (connections topic).
- A box with more than about 12 entrances: nest the minor ones (admin, health) in a
  collapsed box inside it.

### Async pipeline
"What happens after we enqueue?" Producers on the left, a queue card, workers on the
right; a page per queue when there are many.
- search({query: ".add(", kinds: ["text"]}) and `new Queue(` find producers and
  queues; get_relations and list_entrances find the workers.
- add_cards for the producers and the worker entrances (follow callees 1 brings the
  handler), add_resources a queue card anchored where the queue is created, connect
  producer → queue and queue → worker.
- In ts-orders: createOrder adds to `orders` (src/routes/orders.ts:28), the orders
  worker runs processOrder, which adds to `emails` (src/jobs/process-order.ts:10).
  sendEmail's `outbox.add` (src/jobs/send-email.ts:8) is a Set, not a queue: reading
  the hit tells you.

### Explain a call
"What does this call send?" Nothing changes on the page: answer in chat from the
line's contract (graph topic).
- get_current_view shows the selected line; read_call without source reads it;
  read_code on the types it names.

### Data model
"Which tables?" Tables backed by the code's types, so their columns and links are
compiled; relates only where the types don't link; then code cards with reads and
writes (data-models topic).
- search({query, kinds: ["types"]}), read_code on each type, add_resources (tables
  with type), connect (relates), add_cards for the code, connect (reads, writes).

### Data shapes
"What does the worker receive?" Type cards in Diagram with their type arrows, beside
the code that sends and receives them.
- search({query, kinds: ["types"]}), add_cards (the types, follow types 1), and a
  connection that names the type it carries (connect with carries).

### Across repositories
"From the web app to the API." A frame per project, and an http connection from the
client's call to the route.
- search in both projects, add_cards with project, frame_cards a zone per project,
  connect kind http from the calling card (step: the call) to the route's entrance
  card, anchored to that entrance.

## Reviewing a page
check returns the page's health (repairs topic) and these page checks, each with its
items and the call that fixes it:
| Check | Finds | Fix |
| --- | --- | --- |
| overlap | Items that overlap | arrange_page, or moves |
| crossings | More than 3 line crossings, or a line through a frame that holds neither end | arrange_page; a frame's direction; moves |
| density | Over the limits above | Diagram view, step groups, collapse, split |
| unlabeled steps | Flow cards whose steps still show names from the code | label_steps |
| missing words | Frames, step groups, and resource cards without a description | describe |
| evidence | Resource cards (not actors) and connections that cite nothing | add_resources or connect again, with anchors |
| strays | Cards and resource cards with no line, connection, or frame | connect, frame, or remove_items |
| views | More than 3 cards in Flow or Code on an overview page | set_view Diagram |

get_screenshot draws the page as the person sees it, offscreen. A large page is drawn
smaller, and the result says so: take a frame's or a card's image (item) to read it,
at scale 2 for small text. Look for what check can't judge: does the page answer the
brief's question, does it read in one direction, are titles and labels clear, does
anything authored look compiled. Give a one-line verdict per section in chat, then
fix with targeted edits.
