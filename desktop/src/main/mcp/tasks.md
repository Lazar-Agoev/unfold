# The teaching task suite

The instructions, the guide, the tool descriptions, and the results' wording are what teach an
agent to use Unfold (design §7). They're tuned like code: run these tasks with Claude Code and with Codex, score
each run with the rubric below, change the teaching, and run them again. This file is for the person running the
suite; agents never see it.

## Running a task

1. **A fresh copy.** Copy the task's folder out of `testdata/` to a temporary folder (never compile in
   `testdata/`). For ts-orders: `cp -R testdata/playgrounds/ts-orders /tmp/suite/ts-orders`.
2. **Unfold.** Start the app (`just dev`, or a built app); its agent server runs while it is open. Add the copy as a
   project (it compiles at once), unless the task starts from setup.
3. **The agent, with nothing else.** A new session with only Unfold's server installed:
   - Claude Code: `claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp`, then start
     `claude` in an empty folder, and type the task's request.
   - Codex: `codex mcp add unfold --url http://127.0.0.1:29180/mcp`, and type the task's request.
4. **Record** the transcript (tool calls, refusals, the chat), the board (the board menu's Export PNG… for each page,
   and the board file from `.unfold/boards/`), any change to the copy's `unfold.toml`, and the session's token use.
5. **Score** it on the sheet below, then reset: delete the copy's `.unfold/boards/` or the whole copy.

## The rubric

| Part | What's scored | How |
| --- | --- | --- |
| Facts | Each item in the task's "A correct result" list; anything in its "Never" list | 1 point per item present and right; −2 per "Never" item. Everything an agent draws must stand on compiled code: every card placed by key, every resource card and connection citing code, unknown calls left `?`, nothing compiled drawn as authored |
| Readability | The page against the review checklist (start topic): the brief's question answered from the page alone, density, titles, names, and descriptions, few crossings, one direction, nothing overlapping | 1 to 5, judged by a person from the exported PNG |
| Effort | Tool calls, tokens, refusals, and calls repeated unchanged after a refusal | Recorded as numbers; compare runs, not absolutes |
| Rules | A brief before building; one shape_card call per card; check and get_screenshot after each section; finish_work at the end; no ids or keys in the chat; the person's words never replaced; boards, projects, settings, and `unfold.toml` changed only when asked; compiling asked first unless the task says to set up | 1 point each kept; a broken one is noted with the line of the transcript |

### Score sheet

| Task | Agent | Facts | Never | Readability | Calls | Tokens | Refusals | Rules kept | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| | | | | | | | | | |

## The tasks

Keys below are as Unfold compiles the playgrounds. Card ids vary by run.

### 1. Explain Place an order (ts-orders)

- **Request:** "Show me how an order is placed through POST /orders, including the async path. Make it readable for
  someone new."
- **A correct result:**
  - A new board traced from the Place an order entrance (`entrance:server/place-an-order`): the entrance card and
    the cards for its code, requireUser, validateBody, and createOrder (src/routes/orders.ts:25).
  - createOrder expanded and shaped in one shape_card call; the context cards compact; OrderService.place
    (src/services/order-service.ts:13) on the page as its own card, or opened in place from createOrder's
    `call:orderService.place`.
  - The async path: the Process an order entrance (`entrance:worker/process-an-order`, processOrder) and a queue
    connection from createOrder at `src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add` (line 28) to
    it, anchored to that entrance, with the channel `queue orders` filled in by Unfold. carries, if given, is
    `src/types.ts#OrderJob`. Or, if the worker isn't on the page, the chat says the request is queued for it.
  - Frames, if any, with short titles and descriptions (a "Request checks" box around requireUser and validateBody;
    Server and Worker boxes); OrderService.place, which both apps call, in a box of its own or in none.
  - A description on createOrder and on each frame.
  - check finds no overlap after arrange_page.
- **Never:** a connection that restates a compiled call (createOrder to OrderService.place); a connection without
  anchors; a calls connection from a call that isn't unknown; a name, title, or description with code or step
  numbers.

### 2. Shape one card (ts-orders)

- **Setup:** a board with createOrder (`src/routes/orders.ts#createOrder`) expanded on a page; that page in view.
- **Request:** "Make createOrder readable."
- **A correct result:**
  - read_code on createOrder, then one shape_card call: a title, a one-sentence description, and few names.
  - The early return path summarized: `if[0]` combined (alone, or its then part's steps) under a title such as
    "Queue it for a worker", or the branch named as a yes-or-no question ("Client wants an async reply?").
  - The unknown `call:res.status().json` named ("Reply 201 with the order"), still marked `?`.
  - Optionally `call:orderService.place` opened in place.
  - No name on a call that already says what it does (`call:orderService.place`, `call:slack.post`).
- **Never:** a name on every block; a name that claims where an unknown call goes; a combined step that crosses out of
  the then part; separate shape_card calls for the parts of one card.

### 3. Map the apps (ts-orders)

- **Request:** "Give me a map of ts-orders: its apps and how they talk."
- **A correct result:**
  - A zone around boxes for the apps Server, Worker, Scheduler, and Orders CLI (icons api, worker, schedule, cli),
    each holding its entrances, compact. Dev server, Dev CLI, and Legacy export left off or named in the chat.
  - Queue resource cards orders and emails, anchored to `src/queues/queues.ts#/call:Queue` and
    `src/queues/queues.ts#/call:Queue[1]`.
  - queue connections: from createOrder (line 28) to the orders queue card, from processOrder
    (`src/jobs/process-order.ts#processOrder/call:emailQueue.add`, line 10) to the emails queue card, and from each
    queue card to its worker entrance, anchored to that entrance; or straight from producer to worker entrance.
  - The chat names what it couldn't draw: Build a report's code is report-processor.js, which Unfold doesn't analyze;
    `outbox.add` in sendEmail is a Set, not a queue.
- **Never:** a connection between apps without anchors; a queue for `outbox`; a connection from the Scheduler or the
  CLI that no code shows.

### 4. Follow the async pipeline (ts-orders)

- **Request:** "What happens after we enqueue an order?"
- **A correct result:**
  - Producers on the left (createOrder, processOrder), the queue cards orders and emails, or direct lines, then the
    Process an order and Send an email entrances (`entrance:worker/send-an-email`) with their code: processOrder,
    and the callback that calls sendEmail (src/jobs/send-email.ts:6) through withRetry.
  - queue connections as in task 3, each from the add call's step. carries `src/types.ts#OrderJob` when given; the
    save accepts it with the Process an order entrance or `src/queues/queues.ts#` as an anchor.
  - The chat says the worker places the order with OrderService.place, which the server calls too.
- **Never:** `outbox.add` as a producer; a channel the code doesn't name.

### 5. Draw the data model (ts-orders)

- **Request:** "Which data does the order flow keep, and where?"
- **A correct result:**
  - The chat says orders are kept in memory, in a Map in OrderRepository (src/db/repository.ts), not a database.
  - Either type cards (Order, OrderDraft, OrderItem, with Unfold's type arrows) or a table orders backed by
    `src/types.ts#Order`, anchored to `src/db/repository.ts#OrderRepository`, marked id primary, with columns from
    Order and the fields it inherits from OrderDraft.
  - A writes connection from OrderService.place at
    `src/services/order-service.ts#OrderService.place/call:this.repo.insert` (line 22); reads from the code that
    lists or finds orders (`OrderService.list`, `OrderService.get`), each with its step.
  - The table's description says it's in memory.
- **Never:** columns the agent typed for a table its type declares; a customers table (nothing stores customers);
  a relates connection the code doesn't support.

### 6. Explain a call (typed-lines fixture)

- **Folder:** `testdata/fixtures/typed-lines`.
- **Setup:** a board with createOrder (ts/src/orders/create.ts:23) and chargePayment (ts/src/payments.ts:14) on a
  page; select the arrow between them.
- **Request:** "What happens if this fails?"
- **A correct result:**
  - Nothing on the board changes, and finish_work isn't needed.
  - The answer, from read_call (`ts/src/orders/create.ts#createOrder/try[0]/body/call:chargePayment`) and read_code:
    chargePayment can throw CardDeclinedError (ts/src/payments.ts:17); the call sits in createOrder's try, whose
    catch takes it and throws PaymentError (ts/src/orders/create.ts:30) with the error's text.
- **Never:** errors the contract doesn't list stated as fact (Stripe's own errors may be mentioned only as unknown).

### 7. Draw the shop's data model (typed-lines fixture, Python)

- **Folder:** `testdata/fixtures/typed-lines`.
- **Request:** "Show me the Python shop's data model."
- **A correct result:**
  - Type cards: Cart, CartItem, Customer, Address (`py/src/shop/models.py#Cart` and so on), with Unfold's type arrows
    `items: list[CartItem]` and `address: "Address"`; Receipt if the agent includes it.
  - The chat says `price: Money` doesn't link, because `Money = Decimal` is a variable, not a type.
  - The agent wrote only titles and descriptions; every field and arrow is compiled.
- **Never:** tables (nothing says these are stored) unless the person asks; a relates connection between types.

### 8. Explain Place an order (py-orders)

- **Folder:** `testdata/playgrounds/py-orders`.
- **Request:** "Show me how an order is placed through POST /orders. Make it readable for someone new."
- **A correct result:**
  - The Place an order entrance (`entrance:server/place-an-order`) and its code: require_user, validate_body, and
    create_order (src/orders_playground/routes/orders.py:24).
  - create_order expanded and shaped, the context cards compact.
  - If the async path is on the page: a queue connection from create_order at
    `src/orders_playground/routes/orders.py#create_order/if[0]/then/call:celery_app.signature().delay` (line 27) to
    the Process an order entrance (`entrance:worker/process-an-order`, process_order), anchored to that entrance,
    with the channel `queue orders` filled in.
- **Never:** a connection anchored to nothing; a claim that Celery's call resolves.

### 9. Map the mixed playground

- **Folder:** `testdata/playgrounds` (one project holding both playgrounds).
- **Request:** "Give me a map of the playgrounds: their apps and how they talk."
- **A correct result:**
  - A box per app of each half (TypeScript server, worker, scheduler, CLI; the Python server, worker, scheduler, CLI),
    grouped by playground in zones.
  - Each half's queues and connections as in task 3 (BullMQ in ts-orders, Celery in py-orders), with keys under
    `ts-orders/` and `py-orders/`.
  - The chat says the two halves don't talk to each other: nothing in the code connects them.
- **Never:** any connection between the TypeScript and the Python half.

### 10. Fix boards after a rename (ts-orders)

- **Setup:** copy `testdata/boards/order-system.json` into the copy's `.unfold/boards/`, compile, and in Unfold write
  a description of your own on the card OrderService.place. Then rename `place` to `placeOrder` in
  src/services/order-service.ts and at its callers (src/routes/orders.ts, src/jobs/process-order.ts,
  src/jobs/import-orders.ts), and let the graph update.
- **Request:** "I renamed OrderService.place to placeOrder. Fix my boards."
- **A correct result:**
  - The card reconnected to `src/services/order-service.ts#OrderService.placeOrder`; the calls and writes
    connections that leave it whole again, their steps moved along.
  - The agent's outdated texts (createOrder's description, the Server frame's) re-signed after read_code, or
    rewritten.
  - Your description reported in the chat as yours to re-sign, with what changed; not re-signed by the agent.
  - Every problem check still finds is one only you can sign off.
- **Never:** a removed card or connection that could be reconnected; a person's text re-signed or replaced.

### 11. Build the page the person linked (ts-orders)

- **Setup:** in Unfold, add a page "Sending emails" to a board and use its Copy link; leave another page in view.
- **Request:** "<the pasted link> Use this page to show how my email service works."
- **A correct result:**
  - Everything lands on the linked page; the page in view doesn't change.
  - The Send an email entrance with its code (the callback in src/queues/email-worker.ts) expanded, and sendEmail
    (src/jobs/send-email.ts:6) as its own card or opened in place.
  - The emails queue card, or a direct line, and a queue connection from processOrder (src/jobs/process-order.ts:10).
  - No proposed steps: nobody asked for a change.
- **Never:** edits on the page in view; `outbox` as a queue or a database.

### 12. Propose a change, on request (ts-orders)

- **Setup:** task 1's page.
- **Request:** "Orders placed right away never get a confirmation email. Show where one would go."
- **A correct result:**
  - One proposed step "Email a confirmation" after `src/routes/orders.ts#createOrder/call:orderService.place`
    (line 32), with a reason that names the difference from the queued path (processOrder queues an email,
    src/jobs/process-order.ts:10).
  - The compiled path is unchanged; the chat says the step is only proposed: no code implements it.
- **Never:** a status or "done" in the words; a card, connection, or name for code that doesn't exist; proposals
  added while only explaining code.

### 13. Declare how a project starts (ts-orders, setup)

- **Setup:** delete the copy's `unfold.toml`, then add the copy. Home shows no entrances.
- **Request:** "Set up how ts-orders starts: its apps and what each one handles."
- **A correct result:**
  - Apps from package.json written with set_app: Server (`npm run start`, code `src/server.ts#`), Worker
    (`npm run worker`, `src/worker.ts#`), Scheduler (`npm run scheduler`, `src/scheduler.ts#`), and the orders CLI
    (the `orders` bin, `src/cli.ts#`); dev and legacy scripts as apps or named in the chat.
  - Entrances written with set_entrance, each `code` found: the routes (GET /health → `src/server.ts#healthCheck`,
    POST /orders → `src/routes/middleware.ts#requireUser`, `#validateBody`, then `src/routes/orders.ts#createOrder`,
    GET /admin/stats → `src/routes/admin.ts#adminStats`, GET /health/live →
    `src/routes/health.ts#/call:healthRouter.get("/live")/callback[0]`, and the rest), the queues (queue orders →
    `src/jobs/process-order.ts#processOrder`, queue emails → `src/queues/email-worker.ts#/call:Worker("emails")/callback[0]`),
    the schedules (`0 2 * * *` → `src/jobs/cleanup.ts#cleanupExpiredOrders`), SIGTERM → `src/server.ts#shutdown`, and
    the CLI commands.
  - list_entrances at the end, with every code found or the exceptions explained (queue reports runs
    report-processor.js, which Unfold doesn't analyze).
  - The chat says `unfold.toml` changed and is shared.
- **Never:** a code key that names a call, branch, or exit; an app or entrance invented without code that shows it;
  update_project_settings used for entrances.

### 14. Fix a broken entrance (ts-orders)

- **Request:** "Home shows a problem on Admin stats. What's wrong? Fix it."
- **A correct result:**
  - list_entrances: Admin stats' code `src/server.ts#adminStats` is missing, with the fix `src/routes/admin.ts#adminStats`
    (the definition moved).
  - One set_entrance call on the app Server, entrance Admin stats, with code `src/routes/admin.ts#adminStats`; the
    diff changes one line of `unfold.toml`.
  - list_entrances again shows it found; the chat says `unfold.toml` changed.
- **Never:** other entrances changed; the entrance removed and re-added; `unfold.toml` written when the request was
  only "What's wrong?".

### 15. Explain the code in the panel (ts-orders)

- **Setup:** a board with createOrder expanded; select its `if[0]` decision so the code panel shows it.
- **Request:** "What does this code do?"
- **A correct result:**
  - get_current_view, which reports the code panel's block (`src/routes/orders.ts#createOrder/if[0]`) and lines
    (src/routes/orders.ts:27–31); read_code for the rest.
  - The answer in chat: when the client sends `prefer: respond-async`, createOrder adds a place-order job to the orders
    queue and replies 202 with the job id, then stops; ordersQueue.add is a call into bullmq.
  - Nothing on the board changes, and the code panel isn't moved.
- **Never:** an answer about the whole function instead of the selected block; edits to the board.

## Tuning

- **Where the teaching lives:** `instructions.ts` (what every session starts with), `guide/*.md` (the topics),
  `tools/*.ts` (each tool's description, and its results' and refusals' wording), `results.ts` and `format.ts`
  (shared wording).
- **Change one thing at a time,** run `pnpm --filter @unfold/desktop test` (`teaching.test.ts` checks the lengths,
  that texts name only tools, arguments, and topics that exist, and that every tool is taught), rebuild the app,
  and run the same tasks with both agents again.
- **Keep what helps:** a change stays when facts or rules improve and effort doesn't grow. Write what you changed
  and why next to the scores.
