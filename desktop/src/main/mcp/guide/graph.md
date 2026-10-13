# The graph: what Unfold proves, and what it doesn't

Unfold reads TypeScript and Python without running them. What it records is precise:
it claims a fact only with proof, and says "unknown" otherwise. Explain the code in
those terms. Never fill a gap with what the code probably does.

## What Compile records
- **Structure**: files, functions, methods, classes, type declarations, variables,
  and inside each function its statements, branches, loops, returns, throws, and calls.
- **Calls**: every call, construction (`new`), and import, and where it goes.
- **Entrances**: how code starts, found by detectors or declared in settings.
- **Types as written**: each function's parameters and return type, each call's
  arguments paired with the parameters they fill, where each result lands, the fields
  of every interface, type alias, enum, and class, and a link from each written type
  name to its declaration.
- Every item has a key (`src/routes/orders.ts#function:createOrder`), which boards and
  tools use. Keys are for tools: say names and file:line to the person.

## Where a call goes
| Resolution | Means | On a board |
| --- | --- | --- |
| resolved | One proven target in the graph | An arrow to the target's card |
| external | A proven origin outside the graph: a library, named by its module | Dashed, with the module |
| unresolved | No target Unfold can prove, with a reason | Dashed, with the reason |

The reasons, in the words results use:
| Reason | When |
| --- | --- |
| unknown name | No declaration exists for the name, and it isn't a known global |
| receiver type unknown | The type of what the method is called on is unknown (`res.status(202).json`) |
| parameter | The function called comes from a parameter |
| mutable binding | The variable or member is reassigned, so no one target is stable |
| not a concrete function | No body to point at: an interface member, an abstract method, a function type |
| accessor | The call calls what a getter returns |
| ambiguous target | More than one body could be the target |
| overridden by a subclass | A subclass in the graph overrides the method |
| unbound this | `this` in a plain function that isn't a method |
| not indexed | The target exists, but outside the compiled files |
| not found | A relative import, launch path, or locator names nothing |
| dynamic call | No static shape: a computed key, a call's result, a built path |
| wrapper | A wrapper or decorator returns the function, so which body runs isn't proven |
| unsupported form | A form the compiler recognizes but doesn't cover, such as a spread |

Nothing upgrades a guess. A call whose text looks like a known function stays
unresolved. Dashed calls stay dashed; a connection of kind calls can show where one
goes at runtime, with its evidence (connections topic).

## Entrances and applications
- **Kinds**: process (a launch), command, http, worker, schedule, event. Detectors:
  package.json scripts and bins, Express routes, BullMQ workers, node-cron schedules,
  pyproject console scripts, FastAPI routes, Celery tasks, APScheduler jobs, and
  entrances declared in `unfold.toml` (setup topic).
- **Trigger**: exact, partial (only part known, such as a router mounted somewhere
  Unfold can't follow), or unknown ("GET unknown trigger").
- **Applications** are named after how each one starts: the startup file (`server`,
  `worker`) or a console script's function (`cli#main`). An entrance belongs to the
  applications whose code reaches it.
- **"Reached at least from"**: what follows resolved calls from an entrance, through
  every branch. It over-approximates: it says code can be reached, never that it runs.
  Unresolved and external calls stop it, so it can also miss code.

## What the graph never claims
- What happens at runtime: which branch runs, which subclass, in what order, how often.
- Data flow: which value reaches where, beyond a call's paired arguments and result.
- Links between projects, and links through queues, events, HTTP, or databases.
- Flow is "paths from written structure", not a trace. Source order is an outline,
  not an execution order: in `save(convert(value))`, convert runs first. Short-circuit
  expressions stay as written, exceptions and async behavior aren't modeled, and a
  switch, a try with finally, and Python's with, match, and try draw as one opaque step.

## Types: written, never inferred
- **A contract** (read_call, and Selected when the person selects an arrow) says what the call sends (each argument beside the
  parameter it fills, or why it fills none: a spread argument, an unknown receiver
  binding, a class without a constructor), what comes back and where it lands (a
  variable, the caller's return, another call's argument, nowhere), whether it's
  awaited, what the callee throws directly, and whether the caller catches it. Types
  are as written: a missing annotation shows as missing. Only resolved calls and
  constructions have contracts.
- **Direct throws** are the callee's own `throw new X(…)` (Python: `raise X(…)`) that
  its own try doesn't catch. Not a re-throw, not `throw error`, and not what the
  callee's callees throw. Say "can throw", and name only what the contract lists.
- **Type links**: a written type name links to its declaration (declared), a library
  (external), a built-in, or nothing (unresolved, with a reason). Extends and
  implements are links too; extends brings the base's fields along.

## Explaining why something isn't connected
In ts-orders:
- `ordersQueue.add("place-order", …)` (src/routes/orders.ts:28) is external, into
  bullmq. Nothing links it to the orders worker, because no static proof does. Draw an
  anchored queue connection, or say so.
- `this.payments.charge(draft)` (src/services/order-service.ts:15) is unresolved:
  overridden by a subclass, since StripeProvider overrides charge. A calls connection
  can show where it goes at runtime, citing createProvider.
- `events.emit("order.placed", order)` resolves into src/services/events.ts, but there
  `handler(payload)` is unresolved (not a concrete function): the handlers come from a
  list. notifyCustomer is reached only through `events.on`, which an event connection
  can show.
- `res.status(202).json(…)`: res.status is external (express); json, called on its
  result, has receiver type unknown.

read_code lists each call with where it goes; get_relations lists callers and what
reaches a function. Quote them: "Unfold can't follow this call: overridden by a
subclass", not "this calls StripeProvider.charge".

## Explaining a call
In the typed-lines fixture, the person selects the line from createOrder to
chargePayment and asks "what happens if this fails?":
- get_current_view shows the selected line, from createOrder at
  ts/src/orders/create.ts:27 to chargePayment.
- read_call: it sends `cart` as `cart: Cart` and `req.user` as `user: User`; the
  awaited `Promise<Charge>` lands in `charge`; chargePayment can throw
  CardDeclinedError (ts/src/payments.ts:17), and the call sits in a try whose catch
  catches it.
- read_code on createOrder: its own direct throw is PaymentError, at line 30, inside
  the catch.
- The answer, in chat, with nothing changed on the page: "If the card is declined,
  chargePayment throws CardDeclinedError. createOrder catches it and throws a
  PaymentError with the error's text instead."
