# Connections: what the graph can't show

The graph links a call only when it can prove the target. It never links a queue's
producer to its worker, an event to its handlers, a client's request to the route, code
to the tables it writes, or a call to the subclass that runs at runtime. A connection
is a typed line you draw for one of those, citing the code that shows it. It's drawn in
the authored color with its kind's line and a chip, and carries your agent mark. It
never changes a compiled arrow, Called by, Reached from, or Flow.

## Before you draw
- **Read both sides.** read_code on the sending function gives each call's line and
  key (its source); read_code, list_entrances, or read_file gives the receiving side.
- **Is it compiled already?** If read_code says the call resolves, Unfold draws its
  arrow when both cards are on the page. Never restate a compiled call.
- **Can you cite the receiving side?** If not, don't draw it: say in chat what you
  found and what you couldn't confirm.

## Kinds
| kind | Use for | from → to | Anchors (yours) | Chip |
| --- | --- | --- | --- | --- |
| http | A client's request to a route | The calling card (step: the call) → the route's entrance card | Required: the route's entrance | HTTP · POST /orders |
| rpc | gRPC, tRPC, GraphQL, other request and reply | The calling card → the handler | Required: the handler's registration | RPC · … |
| queue | A job or message through a queue or topic | The sending card (step: the add or send call) → the worker entrance, or a queue resource card | Required: where the queue is created, or the worker entrance | queue · orders |
| event | An event emitted and handled | The emitting card (step: the emit call) → the handler | Required: the handler's registration | event · order.placed |
| writes | Inserting, updating, or deleting | The writing card (step: the write) → a table, cache, or storage card | Optional: the step is the evidence | writes · orders |
| reads | Querying or loading | The reading card (step: the read) → a table, cache, or storage card | Optional: the step is the evidence | reads · orders (the head points at the code) |
| calls | Where a call Unfold can't follow goes at runtime | The card with the call (step: that call) → the function it reaches | Required: why it goes there | runtime call |
| relates | A relationship between two tables | Table → table, with fromColumn, toColumn, cardinality | None (columns are checked) | its cardinality (N : 1); crow's feet on the line |
| other | Anything else | Any → any | Optional | its label (required) |

Ends (from and to) are ids of cards, resource cards, or frames on the page. from is the
side that acts: the caller, the sender, the emitter, the code that reads or writes.
When a frame is collapsed, its members' connections attach to it.

## The step: where the line leaves
step is the key of the call or statement inside from's code that talks: read_code
lists each call's source key and each Flow step's key. With a step, the line leaves
from that line in Code and from that step in Flow, the way compiled arrows do; in
Diagram it leaves from the card's edge. Only a code card has steps: when from is an
entrance, a resource card, or a frame, leave step out.

## Channel
- When to is an entrance, Unfold fills the channel in from it: an http route's
  `METHOD path`, a worker's queue. Leave channel out; the result shows what it filled.
- Otherwise pass channel only when the step's or an anchor's text names it, such as
  the queue's name in `new Queue<OrderJob>("orders", …)`. The save checks it.

## What it carries
carries is the key of a type declaration or class the code writes for what travels,
such as `OrderJob` in `ordersQueue: Queue<OrderJob>`. The save checks that the step's
function or an anchor's code uses that type. It shows in the chip
(`queue · orders · OrderJob`), and clicking it opens the type's definition. Pass
carriesProject when the type is in another project.

## Anchors: what counts as evidence
An anchor is `{key}` (a block: a function, class, variable, statement, or one call;
or an entrance's key) or `{file, lines: [from, to]}` (lines of an evidence file; setup
topic). Add project when it's in another project. Cite the receiving side:
- http: the route's entrance key;
- queue: the variable where the queue is created, and the worker entrance if it's on
  the page;
- event: the call that registers the handler (`events.on("order.placed", …)`);
- calls: what makes the call go there at runtime (the factory that returns the
  subclass, the place it's injected);
- rpc: the handler's registration.

A name that merely looks alike, a comment, a test, or your inference is not evidence.
Unfold signs each connection over its step and anchors: when that code changes, the
connection goes to review (repairs topic).

## A line or a queue card
- One producer and one consumer: a queue connection straight from the producer's step
  to the worker entrance.
- Several producers or consumers, or a queue worth describing: a queue resource card
  anchored where the queue is created (add_resources), a queue connection from each
  producer to it, and one from it to each worker entrance (anchored to the entrance).
- Tables, caches, storage, and outside services are resource cards: connect code to
  them with writes and reads, or other with a label for an outside service (createOrder
  posts to Slack: other, label "posts to #orders", to an external card "slack").

## Label and description
- label: what travels, in 1 to 5 words ("place-order job"). Required for other.
- description (required): the evidence in one sentence ("createOrder adds a
  place-order job to the orders queue.").

## Relationships
relates joins two different tables by their columns: fromColumn on from's table,
toColumn on to's, and a cardinality (one-to-one, one-to-many, many-to-one,
many-to-many). Draw one only where the code relates the tables by a value the types
don't link (data-models topic).

## Runtime calls
calls is the connection most likely to be read as proof, so the save holds it to the
call: its step must be the call itself (the call's own key from read_code's calls,
such as `…/expression_statement[0]/call[0]`, not the statement around it), and that
call must be unresolved or external. A call that
resolves is refused: Unfold already draws it. The compiled call stays dashed in the
card; the connection adds where it goes, with its evidence.

## When the save refuses
The edit is undone and nothing changes. The message says what to do:
- "it has no anchors": cite the receiving side.
- "the channel … doesn't appear in its step or anchors": anchor where the channel is
  named, or leave channel out.
- "… isn't used in … or in its anchors": carry a type the sending code or an anchor
  writes, or leave carries out.
- "it has no step" / "its step (line N) isn't a call" / "… resolves to …, so Unfold
  already draws it": a calls connection needs the unresolved call itself as its step.
- "… isn't a column of the table …": use one of the columns listed.
- "it cites …, which is not connected": cite code of a connected project.
connect with an existing id updates that connection; the same kind, ends, step, and
columns again is kept, not duplicated. A person's connection keeps their words: pass
them as they are.

## In ts-orders
Each of these exists in the code and passes the save's checks:
| kind | from (step) | to | anchors |
| --- | --- | --- | --- |
| queue | createOrder at `ordersQueue.add("place-order", …)`, src/routes/orders.ts:28 | The orders worker entrance (channel orders, filled in) | `ordersQueue` in src/queues/queues.ts:5; carries OrderJob |
| queue | processOrder at `emailQueue.add("confirmation", …)`, src/jobs/process-order.ts:10 | The emails worker entrance | `emailQueue` in src/queues/queues.ts:6; carries EmailJob |
| event | OrderService.place at `events.emit("order.placed", order)`, src/services/order-service.ts:23 | notifyCustomer | `events.on("order.placed", notifyCustomer)`, src/services/notifier.ts:25 |
| calls | OrderService.place at `this.payments.charge(draft)`, src/services/order-service.ts:15 (unresolved: overridden by a subclass) | StripeProvider.charge | createProvider, which returns `new StripeProvider()` (src/services/payment.ts:19) |
| writes | OrderService.place at `this.repo.insert(order)`, src/services/order-service.ts:22 | The orders table | None needed |

The first, as a call:
```
connect({board, page, connections: [{
  kind: "queue", from: "create-order", to: "orders",
  step: "src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]",
  carries: "src/types.ts#interface:OrderJob",
  anchors: [{key: "src/queues/queues.ts#variable:ordersQueue"}],
  label: "place-order job",
  description: "createOrder adds a place-order job to the orders queue."}]})
```
The result names the connection with its id, the channel Unfold filled in (orders),
and the carried type. In the chat, say what you drew and why it isn't compiled:
"Unfold sees ordersQueue.add as a call into bullmq; nothing static links it to the
worker, so the queue line is mine, citing where the queue is created."
