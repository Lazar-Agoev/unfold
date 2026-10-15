# The teaching task suite

The instructions, the guide, the tool descriptions, and the results' wording are what teach an
agent to use Unfold. They're tuned like code: run these tasks with Claude Code and with Codex, score
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
| Facts | Each item in the task's "A correct result" list; anything in its "Never" list | 1 point per item present and right; −2 per "Never" item. Everything an agent draws must stand on compiled code: every card placed by key, every resource card and connection citing code, unknown calls left `?`, nothing compiled drawn as authored, nothing authored standing in for an entrance or an arrow |
| Readability | The page against the review checklist (start topic): the brief's question answered from the page alone, the page starting at its entrances, density, few crossings, one direction, nothing overlapping | 1 to 5, judged by a person from the exported PNG |
| Words | The agent's titles, descriptions, and notes against the budgets (diagrams topic): titles on about 3 cards at most, each where the code name says nothing; descriptions on the 1 to 3 cards the page is about, each resource card, and each connection, none on context or entrance cards; names and combined steps only on expanded cards; at most 2 notes, none with status, a schedule, or reading order; frames around three or more cards or an app, never one card | 1 point per budget kept; the agent's texts per card (titles, descriptions, names, and combined steps, divided by the cards on the page) recorded as a number |
| Effort | Tool calls, tokens, refusals, and calls repeated unchanged after a refusal | Recorded as numbers; compare runs, not absolutes |
| Rules | A brief before building; entrances proposed and asked for once before building when the project has none for the code; one shape_card call per expanded card, and none on compact cards; check and get_screenshot after each section; finish_work at the end; no ids or keys in the chat; the person's words never replaced; boards, projects, settings, and `unfold.toml` changed only when asked or after a yes to a proposal; compiling asked first unless the task says to set up | 1 point each kept; a broken one is noted with the line of the transcript |

### Score sheet

| Task | Agent | Facts | Never | Readability | Words | Texts per card | Calls | Tokens | Refusals | Rules kept | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| | | | | | | | | | | | |

## The tasks

Keys below are as Unfold compiles the playgrounds. Card ids vary by run.

### 1. Explain Place an order (ts-orders)

- **Request:** "Show me how an order is placed through POST /orders, including the async path. Make it readable for
  someone new."
- **A correct result:**
  - A new board traced from the Place an order entrance (`entrance:server/place-an-order`): the entrance card and
    the cards for its code, requireUser, validateBody, and createOrder (src/routes/orders.ts:25).
  - createOrder expanded and shaped in one shape_card call, with no title (its code name says what it does); the
    context cards compact and unshaped; OrderService.place (src/services/order-service.ts:13) on the page as its own
    card, or opened in place from createOrder's `call:orderService.place`.
  - The async path: the Process an order entrance (`entrance:worker/process-an-order`, processOrder) and a queue
    connection from createOrder at `src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add` (line 28) to
    it, anchored to that entrance, with the channel `queue orders` filled in by Unfold. carries, if given, is
    `src/types.ts#OrderJob`. Or, if the worker isn't on the page, the chat says the request is queued for it.
  - Frames, if any, with short titles: Server and Worker boxes when both apps are on the page; OrderService.place,
    which both apps call, outside both.
  - A description on createOrder; none on the context cards or the entrance cards.
  - check finds no "overlap", "strays", or "too many words" after arrange_page.
- **Never:** a connection that restates a compiled call (createOrder to OrderService.place); a connection without
  anchors; a calls connection from a call that isn't unknown; a name, title, or description with code or step
  numbers; a title on a card whose code name says what it does (createOrder, OrderService.place); a description on
  a context card or an entrance card; a note with status, a schedule, or reading order; a frame around one card;
  names or combined steps on a compact card.

### 2. Shape one card (ts-orders)

- **Setup:** a board with createOrder (`src/routes/orders.ts#createOrder`) expanded on a page; that page in view.
- **Request:** "Make createOrder readable."
- **A correct result:**
  - read_code on createOrder, then one shape_card call: a one-sentence description and few names, with no title
    (createOrder says what it does).
  - The early return path summarized: `if[0]` combined (alone, or its then part's steps) under a title such as
    "Queue it for a worker", or the branch named as a yes-or-no question ("Client wants an async reply?").
  - The unknown `call:res.status().json` named ("Reply 201 with the order"), still marked `?`.
  - Optionally `call:orderService.place` opened in place.
  - No name on a call that already says what it does (`call:orderService.place`, `call:slack.post`).
- **Never:** a name on every block; a name that claims where an unknown call goes; a combined step that crosses out of
  the then part; separate shape_card calls for the parts of one card; a title on createOrder; a note.

### 3. Map the apps (ts-orders)

- **Request:** "Give me a map of ts-orders: its apps and how they talk."
- **A correct result:**
  - The apps' entrances traced in one call (create_board with trace listing their keys, or add_cards with their keys
    and follow `{callees: 1}`), compact, with Unfold's start arrows. A box per app, Server, Worker, Scheduler, and
    Orders CLI (icons api, worker, schedule, cli), each holding its entrances and their code; a zone around them, if
    any. Dev server, Dev CLI, and Legacy export left off or named in the chat.
  - Queue resource cards orders and emails, anchored to `src/queues/queues.ts#/call:Queue` and
    `src/queues/queues.ts#/call:Queue[1]`.
  - queue connections: from createOrder (line 28) to the orders queue card, from processOrder
    (`src/jobs/process-order.ts#processOrder/call:emailQueue.add`, line 10) to the emails queue card, and from each
    queue card to its worker entrance, anchored to that entrance; or straight from producer to worker entrance.
  - The chat names what it couldn't draw: Build a report's code is report-processor.js, which Unfold doesn't analyze;
    `outbox.add` in sendEmail is a Set, not a queue.
- **Never:** a connection between apps without anchors; a queue for `outbox`; a connection from the Scheduler or the
  CLI that no code shows; a note or a description that says when something runs (the Scheduler's entrance says
  `0 2 * * *`); descriptions on the entrance cards; a frame around one card.

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
  - Every field and arrow is compiled; the agent wrote at most a description or two, and no titles.
- **Never:** tables (nothing says these are stored) unless the person asks; a relates connection between types.

### 8. Explain Place an order (py-orders)

- **Folder:** `testdata/playgrounds/py-orders`.
- **Request:** "Show me how an order is placed through POST /orders. Make it readable for someone new."
- **A correct result:**
  - The Place an order entrance (`entrance:server/place-an-order`) and its code: require_user, validate_body, and
    create_order (src/orders_playground/routes/orders.py:24).
  - create_order expanded and shaped, with no title; the context cards compact and unshaped.
  - If the async path is on the page: a queue connection from create_order at
    `src/orders_playground/routes/orders.py#create_order/if[0]/then/call:celery_app.signature().delay` (line 27) to
    the Process an order entrance (`entrance:worker/process-an-order`, process_order), anchored to that entrance,
    with the channel `queue orders` filled in.
- **Never:** a connection anchored to nothing; a claim that Celery's call resolves; a description on a context card or
  an entrance card.

### 9. Map the mixed playground

- **Folder:** `testdata/playgrounds` (one project holding both playgrounds).
- **Request:** "Give me a map of the playgrounds: their apps and how they talk."
- **A correct result:**
  - A box per app of each half (TypeScript server, worker, scheduler, CLI; the Python server, worker, scheduler, CLI),
    grouped by playground in zones.
  - Each half's queues and connections as in task 3 (BullMQ in ts-orders, Celery in py-orders), with keys under
    `ts-orders/` and `py-orders/`.
  - The chat says the two halves don't talk to each other: nothing in the code connects them.
- **Never:** any connection between the TypeScript and the Python half; a note with status or a schedule.

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

### 16. A project with no entrances (ts-orders)

- **Setup:** delete the `[[app]]` tables from the copy's `unfold.toml` (keep the rest), then add the copy. Home shows
  no entrances. When the agent asks to declare apps and entrances, answer "Yes, go ahead."
- **Request:** "Can you explain to me visually on the Unfold app how an order gets placed? Assume I don't know how
  this works."
- **A correct result:**
  - list_entrances or get_project shows no apps, and the agent proposes apps and entrances in the chat before
    building anything, from package.json and the code, each with what shows it: at least Server (`npm run start`,
    `src/server.ts#`) with Place an order (POST /orders → requireUser, validateBody, createOrder), and Worker
    (`npm run worker`, `src/worker.ts#`) with Process an order (queue orders → processOrder). It asks once.
  - After the yes: set_app and set_entrance, list_entrances with each code found, and the chat says `unfold.toml`
    changed.
  - A board traced from Place an order (and Process an order, for the async path) in one call; createOrder
    expanded and shaped, with no title; the context cards compact and unshaped; the async path as in task 1.
  - Within the budgets: no titles (every code name here says what it does), descriptions on at most 3 cards plus the
    resource cards and connections, at most 2 notes. check finds no "too many words".
- **Never:** a board, page, or card made before the person answered; `unfold.toml` written before the yes; an app or
  entrance the code and package.json don't show; asking more than once; a note or a description standing in for an
  entrance ("START HERE", "Runs on every request").

### 17. Make a heavy page lighter (ts-orders)

- **Setup:** build a heavy page first, in a session of its own (or by hand in Unfold): create_board with trace
  `entrance:server/place-an-order`; add_cards with OrderService.place (`src/services/order-service.ts#OrderService.place`)
  and processOrder (`src/jobs/process-order.ts#processOrder`); shape_card on every card with a title and a
  description (the entrance card takes only a description); add_notes with five notes ("START HERE: POST /orders",
  "LIVE in production", "Runs on every request", "BROKEN: some emails fail", "Read left to right"); frame_cards with
  one frame around each of requireUser, validateBody, and processOrder. Then, in Unfold, write a note of your own
  ("Ask Maya about retries."). Start the task in a new session, with that page in view.
- **Request:** "The whole thing is too heavy and doesn't make sense. Reuse the existing functions, code blocks, and
  entrances, and make it look nicer and understandable."
- **A correct result:**
  - The page edited in place, not started over.
  - The agent's excess words taken off: shape_card with title null on the cards whose code name says what they do,
    description null on the context cards and the entrance card; remove_items for the status, schedule, and
    reading-order notes and for the single-card frames (their cards stay).
  - The entrances the page needs on it and traced: Place an order, and Process an order with processOrder, by
    add_cards with their keys and follow, so Unfold's arrows carry the flow.
  - Your note kept as you wrote it.
  - arrange_page once, then moves alone; check finds no "too many words" and no "strays".
  - The chat says what was taken off and why.
- **Never:** your note removed or rewritten; new titles, notes, or frames around one card; a description left on
  every card; a new board instead of the page.

## Tuning

- **Where the teaching lives:** `instructions.ts` (what every session starts with), `guide/*.md` (the topics),
  `tools/*.ts` (each tool's description, and its results' and refusals' wording), `results.ts` and `format.ts`
  (shared wording).
- **Change one thing at a time,** run `pnpm --filter @unfold/desktop test` (`teaching.test.ts` checks the lengths,
  that texts name only tools, arguments, and topics that exist, and that every tool is taught), rebuild the app,
  and run the same tasks with both agents again.
- **Keep what helps:** a change stays when facts or rules improve and effort doesn't grow. Write what you changed
  and why next to the scores.
