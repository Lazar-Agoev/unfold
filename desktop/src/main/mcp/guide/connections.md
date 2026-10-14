# Connections: what the graph can't show

The graph links a call only when it can prove the target. It never links a client's
request to the route, a job to its worker, an event to its handler, code to the tables
it writes, or an unknown call to what runs at runtime. A connection is a typed line you
draw for one of those, citing the code that shows it. It is drawn as authored and never
changes a compiled arrow or a drawn flow.

## Before you draw
- **Read both sides.** read_code on the sending function gives each call's key;
  read_code, list_entrances, or read_file gives the receiving side.
- **Is it compiled already?** A call to a project definition is drawn as an arrow when
  both cards are on the page. A connection that repeats one is refused; get_relations
  says what Unfold draws.
- **Can you cite the receiving side?** If not, don't draw it: say in chat what you
  found and what you couldn't confirm.

## The seven kinds
| kind | For | from (step) → to | Anchors |
| --- | --- | --- | --- |
| http | A request to another service: a route, an RPC, tRPC, or GraphQL call | The calling card (step: the call) → the route's entrance card, or the handler | Required: the route's entrance, or the handler's registration |
| queue | A job, message, or event through a queue or a bus | The sender or emitter (step: the add, send, or emit call) → a queue card, the worker's entrance, or the handler | Required: where the queue is created, the worker's entrance, or the handler's registration |
| writes | Inserting, updating, or deleting | The code (step: the call that writes) → a table, cache, or storage card | The step is the evidence |
| reads | Querying or loading | The code (step: the call that reads) → a table, cache, or storage card | The step is the evidence |
| calls | Where an unknown call goes at runtime | The card with the call (step: that call) → the function it reaches | Required: why it goes there, such as a factory or an injection |
| relates | Two tables joined by a value | Table → table, with fromColumn, toColumn, and cardinality | None: the columns are checked |
| other | Anything else, such as a call to an outside service | Any → any | Optional; a label is required |

from and to are ids of cards, resource cards, or frames on the page. from is the side
that acts. A connection to a collapsed frame's member attaches to the frame.

## The step
step is the key of the call in from's code where the line leaves, from read_code. On an
expanded card the line starts at that call; on a compact card, at its edge. Only code
cards have steps: from an entrance, a resource card, or a frame, leave step out.

## Channel
- When to is an entrance card, Unfold fills the channel in from its `on` (`POST
  /orders`, `queue orders`). Leave channel out.
- Otherwise pass channel only when the step's or an anchor's code names it, such as
  "orders" in `new Queue<OrderJob>("orders", …)`. The save checks it.

## What it carries
carries is the key of the type that travels. The save checks that the step's function
or an anchor's code names that type in a written annotation: `processOrder(job:
Job<OrderJob>)` names OrderJob, and so does the module that declares `ordersQueue:
Queue<OrderJob>`. Carry the request only; a reply through a queue is a second
connection.

## Anchors
An anchor is `{key}` (a block's, app's, or entrance's key) or `{file, lines: [from,
to]}` (lines of an evidence file; setup topic). Cite the receiving side:
- http: the route's entrance;
- queue: the call that creates the queue, the worker's entrance, or the call that
  registers the handler (`events.on("order.placed", …)`);
- calls: what makes the call go there (the factory that returns the subclass, the
  place it's injected).

A name that looks alike, a comment, a test, or your inference is not evidence. Unfold
signs each connection over its step and anchors: when that code changes, it goes to
review (repairs topic).

## A line or a queue card
- One producer and one consumer: a queue connection from the producer's step straight
  to the worker's entrance.
- Several, or a queue worth describing: a queue resource card anchored where the queue
  is created (add_resources), a connection from each producer to it, and one from it to
  each worker entrance.
- Tables, caches, storage, and outside services are resource cards: connect code to
  them with writes, reads, or other.

## Label and description
- label: what travels, in 1 to 5 words ("place-order job"). Required for other.
- description (required): the evidence in one sentence.

## Relationships
relates joins two different tables: fromColumn on from's table, toColumn on to's, and a
cardinality (one-to-one, one-to-many, many-to-one, many-to-many). Only where a column
holds the other table's key as a plain value (data-models topic).

## Runtime calls
calls is the connection most likely to be read as proof, so its step must be the
unknown call itself (`?` in read_code). A project or library call is refused. The call
stays `?` on the card; the connection adds where it goes, with its evidence.

## When the save refuses
The edit is undone and the message says what to do:
- "it has no anchors": cite the receiving side.
- "the channel … doesn't appear in its step or anchors": anchor where it's named, or
  leave channel out.
- "… isn't used in … or in its anchors": carry a type the code names, or leave
  carries out.
- "its step … is a …, not a call" or "… resolves to …, so Unfold already draws it":
  give a call's key; for calls, the unknown call.
- "… isn't a column of the table …": use one of the columns listed.
- "… repeats a compiled call": nothing to draw; the arrow is already there.

connect with an existing id updates that connection. A person's connection keeps their
words: pass them as they are.

## In ts-orders
Each of these is in the code and passes the save's checks:
| kind | from, at the step | to | anchors |
| --- | --- | --- | --- |
| queue | createOrder, at `ordersQueue.add("place-order", …)` (src/routes/orders.ts:28) | The Process an order entrance (channel `queue orders`, filled in) | `entrance:worker/process-an-order`; carries `src/types.ts#OrderJob` |
| queue | processOrder, at `emailQueue.add("confirmation", …)` (src/jobs/process-order.ts:10) | The Send an email entrance | `entrance:worker/send-an-email` |
| queue (an event) | OrderService.place, at `events.emit("order.placed", order)` (src/services/order-service.ts:23) | notifyCustomer | `src/services/notifier.ts#/call:events.on` |
| calls | OrderService.place, at `this.payments.charge(draft)` (line 15; unknown: overridden by a subclass) | StripeProvider.charge | `src/services/payment.ts#createProvider`, which returns `new StripeProvider()` |
| writes | OrderService.place, at `this.repo.insert(order)` (line 22) | An orders table | None needed |

The first, as a call:
```
connect({board, page, connections: [{
  kind: "queue", from: <createOrder's card id>, to: <Process an order's card id>,
  step: "src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add",
  carries: "src/types.ts#OrderJob",
  anchors: [{key: "entrance:worker/process-an-order"}],
  label: "place-order job",
  description: "createOrder adds a place-order job to the orders queue, which the worker handles."}]})
```
In chat, say what you drew and why it isn't compiled: "Unfold sees ordersQueue.add as a
call into bullmq; nothing static links it to the worker, so the queue line is mine,
citing the worker's entrance."
