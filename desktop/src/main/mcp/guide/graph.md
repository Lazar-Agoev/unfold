# The graph: what Unfold proves, and what it doesn't

Unfold reads TypeScript and Python without running them. It records a fact only with
proof, and says "unknown" otherwise. Explain the code in those terms; never fill a gap
with what the code probably does.

## What Compile records
- **Blocks.** Definitions (modules, classes, functions, methods, types), and inside
  each: calls, control blocks (if, switch or match, loops, try, with, and ternaries or
  comprehensions that hold a call), their parts (then, else, a case, a loop's body, an
  except), and exits (return, throw or raise, break, continue, yield). Statements
  aren't blocks, only the calls in them: code between blocks shows as `⋯` lines.
- **Links.** Where each call goes, functions passed as values, imports, extends, and
  the types named in annotations and except clauses.
- **Types as written.** Parameters and return types, each call's arguments paired with
  the parameters they fill, where its value lands, and every type's fields.
- **Keys.** One per block, built from names and positions:
  - `src/routes/orders.ts#` a module, `#createOrder` a function,
    `#OrderService.place` a method;
  - `src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add` a block inside;
  - `src/queues/queues.ts#/call:Queue` a module's own code; a repeat adds `[n]`
    (`call:Queue[1]`);
  - `src/queues/email-worker.ts#/call:Worker("emails")/callback[0]` an inline function.
  Copy keys from results; say names and file:line to the person.
- **Apps and entrances** come from `unfold.toml`. Unfold checks only that each `code`
  key names a start (found, missing, not analyzed, not a start, stale). It never reads
  `run` or `on`: an entrance that says POST /orders proves the function exists, not that
  the route does.

## Where a call goes
| Outcome | Means | Drawn |
| --- | --- | --- |
| project | One proven target in the project | `▸` a box, an arrow to its card; it can open in place |
| library | A proven outside module (bullmq, express, fastapi) | A thin row, or a box when it takes a callback |
| builtin | The language's own builtins | A thin row |
| unknown | No target Unfold can prove, with a reason | `?` in a thin row |

Naming a library or unknown call draws it as a step; an unknown call stays `?`.

The reasons, in the words results use:
| Reason | When |
| --- | --- |
| unknown name | No declaration exists for the name |
| receiver type unknown | The type of what the method is called on is unknown (`res.status(201).json`) |
| parameter | The function called comes from a parameter |
| mutable binding | The variable or member is reassigned, so no one target is stable |
| not a concrete function | No body to point at: an interface member, an abstract method, a function type |
| accessor | The call calls what a getter returns |
| ambiguous target | More than one body could be the target |
| overridden by a subclass | A subclass in the project overrides the method |
| unbound this | `this` in a plain function that isn't a method |
| not indexed | The target exists, but outside the compiled files |
| not found | An import or path names nothing |
| dynamic call | No static shape: a computed key, a call's result, a built path |
| wrapper | A wrapper or decorator returns the function, so which body runs isn't proven |
| unsupported form | A form the compiler doesn't cover yet, such as a spread |

Nothing upgrades a guess, and a name never resolves a call. A connection of kind calls
can show where an unknown call goes at runtime, with its evidence (connections topic).

## The drawn flow
- Steps run in written order, and a call's arguments before the call: in
  `save(convert(value))`, convert comes first.
- A decision's parts are lanes. A try's except parts hang off dashed "on error"
  routes: a possibility, not a claim that it throws.
- Code after an early return is drawn dimmed, "never runs". Calls after `&&`, `||`,
  `??`, `?.`, `and`, or `or` are marked SOMETIMES.
- "Reached at least from" (get_relations) follows project calls from each entrance's
  code through every branch: code can be reached, never that it runs. Library and
  unknown calls stop it, so it can miss code too.

## What the graph never claims
- What happens at runtime: which branch runs, which subclass, how often.
- Data flow beyond a call's paired arguments and where its value lands.
- Links between projects, or through queues, events, HTTP, or databases.

## Types: written, never inferred
- **A contract** (read_call, and Selected when the person selects an arrow): what the
  call sends (each argument beside the parameter it fills, or why it fills none), what
  comes back and where it lands, whether it's awaited, what the callee throws directly,
  and which part of the caller catches it. A missing annotation shows as missing.
- **Direct throws** are the callee's own `throw new X(…)` or `raise X(…)` outside its
  own try. Not a re-throw, and not what its callees throw. Say "can throw", and name
  only what the contract lists.
- **Type links**: a written type name links to its declaration, a library, a builtin,
  or nothing (unknown, with a reason).

## Explaining why something isn't connected
In ts-orders:
- `ordersQueue.add("place-order", …)` (src/routes/orders.ts:28) is a library call into
  bullmq. Nothing static links it to the orders worker: draw a cited queue connection,
  or say so.
- `this.payments.charge(draft)` (src/services/order-service.ts:15) is unknown:
  overridden by a subclass, since StripeProvider overrides charge. A calls connection
  can show where it goes, citing createProvider.
- `events.emit("order.placed", order)` resolves to events.emit in
  src/services/events.ts, where `handler(payload)` is unknown (not a concrete
  function). notifyCustomer is only passed to `events.on` (src/services/notifier.ts:25):
  a queue connection labeled with the event can show it.
- `res.status(201).json(order)`: res.status is a library call (express); json, called
  on its result, is unknown (receiver type unknown).

Quote the graph: "Unfold can't follow this call: overridden by a subclass", never "this
calls StripeProvider.charge".

## Explaining a call
In the typed-lines fixture, the person selects the arrow from createOrder to
chargePayment and asks "what happens if this fails?":
- get_current_view shows the selected arrow, from createOrder
  (ts/src/orders/create.ts:27) to chargePayment.
- read_call: it sends `cart` as `cart: Cart` and `req.user` as `user: User`; the
  awaited `Promise<Charge>` lands in `charge`; chargePayment can throw
  CardDeclinedError (ts/src/payments.ts:17), and createOrder's catch catches it.
- read_code on createOrder: its own throw is PaymentError, at line 30, in the catch.
- The answer, in chat, with nothing changed: "If the card is declined, chargePayment
  throws CardDeclinedError. createOrder catches it and throws a PaymentError with the
  error's text instead."
