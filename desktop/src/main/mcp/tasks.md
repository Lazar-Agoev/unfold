# The teaching task suite

The instructions, the guide, the prompts, the tool descriptions, and the results' wording are what teach an
agent to use Unfold (design §7). They're tuned like code: run these tasks with Claude Code and with Codex, score
each run with the rubric below, change the teaching, and run them again. This file is for the person running the
suite; agents never see it.

## Running a task

1. **A fresh copy.** Copy the task's folder out of `testdata/` to a temporary folder (never compile in
   `testdata/`). For ts-orders: `cp -R testdata/playgrounds/ts-orders /tmp/suite/ts-orders`.
2. **Unfold.** Start the app (`just dev`, or a built app), turn on Preferences › Agents › Let agents connect, and
   save. Add the copy as a project and compile it, unless the task starts from setup.
3. **The agent, with nothing else.** A new session with only Unfold's server installed:
   - Claude Code: `claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp`, then start
     `claude` in an empty folder. Start the task with its prompt (`/unfold:<name> …`) or its request.
   - Codex: `codex mcp add unfold --url http://127.0.0.1:29180/mcp`. Codex doesn't offer MCP prompts, so type the
     task's request.
4. **Record** the transcript (tool calls, refusals, the chat), the board (Share › Export PNG of each page, and the
   board file from `.unfold/boards/`), and the session's token use.
5. **Score** it on the sheet below, then reset: delete the copy's `.unfold/boards/` or the whole copy.

## The rubric

| Part | What's scored | How |
| --- | --- | --- |
| Facts | Each item in the task's "A correct result" list; anything in its "Never" list | 1 point per item present and right; −2 per "Never" item. Everything an agent draws must stand on compiled code: every card placed by key, every table and connection grounded, nothing compiled drawn as authored |
| Readability | The page against the review checklist (start topic): the brief's question answered from the page alone, density, titles and descriptions, labeled Flow steps, few crossings, one direction, nothing overlapping | 1 to 5, judged by a person from the exported PNG |
| Effort | Tool calls, tokens, refusals, and calls repeated unchanged after a refusal | Recorded as numbers; compare runs, not absolutes |
| Rules | A brief before building; check and get_screenshot after each section; finish_work at the end; no ids or keys in the chat; the person's words never replaced; settings, projects, and boards changed only when asked; compiling asked first unless the task says to set up | 1 point each kept; a broken one is noted with the line of the transcript |

### Score sheet

| Task | Agent | Facts | Never | Readability | Calls | Tokens | Refusals | Rules kept | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| | | | | | | | | | |

## The tasks

### 1. Explain POST /orders (ts-orders)

- **Prompt:** `/unfold:explain-entrance POST /orders`
- **Request:** "Show me how an order is placed through POST /orders, including the async path. Make it readable for
  someone new."
- **A correct result:**
  - A new board with one page, traced from POST /orders: the entrance card and its handlers requireUser,
    validateBody, and createOrder (src/routes/orders.ts:25).
  - createOrder expanded, the context cards compact; OrderService.place
    (src/services/order-service.ts:13) on the page, as its own card or expanded in place from createOrder's
    call at line 32.
  - Step groups on createOrder that follow the rule, such as lines 28–30 (the queued branch) and 32–34 (placing it
    now), titled by what they achieve; every Flow step of createOrder labeled, the condition as a question.
  - The async path: the orders worker entrance (application worker) with processOrder, and a queue connection from
    createOrder's step at line 28 (`ordersQueue.add`, key `src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]`
    or its statement) to the worker entrance, anchored to `src/queues/queues.ts#variable:ordersQueue` or the worker
    entrance, with the channel orders filled in by Unfold; carries, if given, is `src/types.ts#interface:OrderJob`.
    Or, if the agent leaves the worker off the page, the chat says the request is queued for the orders worker.
  - Frames, if any, with short titles and descriptions (a "Request checks" box around requireUser and
    validateBody; server and worker boxes); OrderService.place, which both services call, in a box of its own or in
    none.
  - A description on createOrder and on each frame.
  - check finds no overlap after arrange_page.
- **Never:** a connection that restates a compiled call (createOrder to OrderService.place); a connection without
  anchors; a calls connection from a call that resolves; a label or title with code or step numbers.

### 2. Simplify createOrder (ts-orders)

- **Setup:** a board with createOrder on a page (task 1's board, or one card added by hand), selected.
- **Prompt:** `/unfold:simplify-function createOrder`
- **Request:** "createOrder has too many steps. Group them."
- **A correct result:**
  - Two or three step groups on the card, each whole statements of one branch: lines 28–30 and 32–34, or an
    equally valid choice the brief explained.
  - Titles of 2 to 6 words that say what the steps achieve; every one of the card's Flow steps labeled (Read the
    draft, the condition as a question, the steps inside the groups too).
  - The card expanded with the groups collapsed: it reads in four steps between Start and End.
  - check finds nothing on the card; get_screenshot of the card.
- **Never:** a group across the if's branches; a group that changes what the code does; labels that quote code.

### 3. Map the services (ts-orders)

- **Prompt:** `/unfold:service-map ts-orders`
- **Request:** "Give me a map of ts-orders: its services and how they talk."
- **A correct result:**
  - A zone around boxes for the applications server, worker, scheduler, and cli (icons api, worker, schedule, cli),
    each holding its entrances, compact.
  - Queue resource cards orders and emails, anchored to `src/queues/queues.ts#variable:ordersQueue` and
    `#variable:emailQueue`.
  - queue connections: from createOrder (line 28) to the orders queue card, from processOrder
    (src/jobs/process-order.ts:10, `emailQueue.add`) to the emails queue card, and from each queue card to its
    worker entrance, anchored to that entrance (or straight from producer to worker entrance, anchored to the queue).
  - The chat names what it couldn't draw: the report worker's processor is a JavaScript file Unfold doesn't graph,
    and `outbox.add` in sendEmail is a Set, not a queue.
- **Never:** a connection between applications without anchors; a queue for `outbox`; a connection from the
  scheduler or the CLI that no code shows.

### 4. Follow the async pipeline (ts-orders)

- **Request:** "What happens after we enqueue an order?"
- **A correct result:**
  - Producers on the left (createOrder, processOrder), the queue cards orders and emails in the middle, the worker
    entrances orders and emails on the right, with processOrder and sendEmail as their handlers' code.
  - queue connections as in task 3, each from the add call's step; carries OrderJob and EmailJob when given
    (`src/types.ts#interface:OrderJob`, `#interface:EmailJob`), which the save checks.
  - The chat says the worker places the order with OrderService.place, which the server calls too.
- **Never:** `outbox.add` as a producer; a channel the code doesn't name.

### 5. Draw the data model (ts-orders)

- **Prompt:** `/unfold:data-model ts-orders`
- **Request:** "Which data does the order flow keep, and where?"
- **A correct result:**
  - The chat says orders are kept in memory, in a Map in OrderRepository (src/db/repository.ts), not a database.
  - Either type cards (Order, OrderDraft, OrderItem, with Unfold's type arrows) or a table orders backed
    by `src/types.ts#interface:Order`, anchored to `src/db/repository.ts#class:OrderRepository`, marked id primary,
    with columns from Order and the fields it inherits from OrderDraft.
  - A writes connection from OrderService.place at `this.repo.insert(order)` (src/services/order-service.ts:22);
    reads from the code that lists or finds orders, each with its step.
  - The table's description says it's in memory.
- **Never:** columns the agent typed for a table its type declares; a customers table (nothing stores customers);
  a relates connection the code doesn't support.

### 6. Explain a call (typed-lines fixture)

- **Folder:** `testdata/fixtures/typed-lines`.
- **Setup:** a board with createOrder (ts/src/orders/create.ts:23) and chargePayment (ts/src/payments.ts:14) on a
  page; select the line between them.
- **Request:** "What happens if this fails?"
- **A correct result:**
  - Nothing on the board changes, and finish_work isn't needed.
  - The answer, from read_call and read_code: chargePayment can throw CardDeclinedError (ts/src/payments.ts:17);
    the call sits in createOrder's try, whose catch takes it and throws PaymentError (ts/src/orders/create.ts:30)
    with the error's text.
- **Never:** errors the contract doesn't list stated as fact (Stripe's own errors may be mentioned only as unknown).

### 7. Draw the shop's data model (typed-lines fixture, Python)

- **Folder:** `testdata/fixtures/typed-lines`.
- **Request:** "Show me the Python shop's data model."
- **A correct result:**
  - Type cards: Cart, CartItem, Customer, Address (py/src/shop/models.py), with Unfold's type arrows
    `items: list[CartItem]` and `address: "Address"`; Receipt if the agent includes it.
  - The chat says `price: Money` doesn't link, because `Money = Decimal` is a variable, not a type.
  - The agent wrote only titles and descriptions; every field and arrow is compiled.
- **Never:** tables (nothing says these are stored) unless the person asks; a relates connection between types.

### 8. Explain POST /orders (py-orders)

- **Folder:** `testdata/playgrounds/py-orders`.
- **Prompt:** `/unfold:explain-entrance POST /orders`
- **A correct result:**
  - The FastAPI entrance POST /orders and its handlers require_user, validate_body, and create_order
    (src/orders_playground/routes/orders.py:24).
  - create_order expanded with step groups and labels, as in task 2.
  - If the async path is on the page: a queue connection from create_order's step at line 27
    (`celery_app.signature("orders.process", …).delay()`, key
    `src/orders_playground/routes/orders.py#function:create_order/if[0]/variable:job/call[0]`) to the Celery worker
    entrance orders (process_order), anchored to that entrance, with the channel orders filled in.
- **Never:** a connection anchored to nothing; a claim that Celery's call resolves.

### 9. Map the services of the mixed playground

- **Folder:** `testdata/playgrounds` (one project holding both playgrounds).
- **Prompt:** `/unfold:service-map`
- **A correct result:**
  - A frame per application of each half (the TypeScript applications and the Python console scripts), grouped by
    language or by playground in zones.
  - Each half's queues and connections as in task 3 (BullMQ in ts-orders, Celery in py-orders).
  - The chat says the two halves don't talk to each other: nothing in the code connects them.
- **Never:** any connection between the TypeScript and the Python half.

### 10. Fix boards after a rename (ts-orders)

- **Setup:** copy `testdata/boards/order-system.json` into the copy's `.unfold/boards/`, compile, and in Unfold write
  a description of your own on the card OrderService.place. Then rename `place` to `placeOrder` in
  src/services/order-service.ts and at its callers (src/routes/orders.ts, src/jobs/process-order.ts,
  src/jobs/import-orders.ts), and let the graph update.
- **Prompt:** `/unfold:fix-boards ts-orders`
- **Request:** "I renamed OrderService.place to placeOrder. Fix my boards."
- **A correct result:**
  - The card reconnected to `src/services/order-service.ts#class:OrderService/method:placeOrder`; the calls and
    writes connections that leave it whole again, their steps moved along.
  - The agent's outdated texts (the server frame's description) re-signed after read_code, or rewritten.
  - Your description reported in the chat as yours to re-sign, with what changed; not re-signed by the agent.
  - check on the board finds nothing but your sign-off.
- **Never:** a removed card or connection that could be reconnected; a person's text re-signed or replaced.

### 11. Build the page the person linked (ts-orders)

- **Setup:** in Unfold, add a page "Sending emails" to a board and use its Copy link; leave another page in view.
- **Request:** "<the pasted link> Use this page to show how my email service works."
- **A correct result:**
  - Everything lands on the linked page; the page in view doesn't change.
  - The emails worker entrance (src/queues/email-worker.ts) with its handler expanded, and the withRetry callback at
    line 9 expanded in place to show sendEmail's steps, still through withRetry.
  - sendEmail (src/jobs/send-email.ts:6) as its own card, with its simulated delay and logging (lines 9–10) combined
    into one step such as "Simulate delivery"; the recipient check and the outbox steps stay separate.
  - The emails queue card and a queue connection from processOrder (src/jobs/process-order.ts:10).
  - No proposed steps: nobody asked for a change.
- **Never:** edits on the page in view; a direct sendEmail call drawn without the wrapper; `outbox` as a queue or a
  database; a step group made on the handler's card for sendEmail's steps.

### 12. Propose a change, on request (ts-orders)

- **Setup:** task 1's page.
- **Request:** "Orders placed right away never get a confirmation email. Show where one would go."
- **A correct result:**
  - One proposed step "Email a confirmation" on createOrder's card, after the step at line 32 (`orderService.place`,
    key `src/routes/orders.ts#function:createOrder/variable:order`), with a reason that names the difference from the
    queued path (processOrder queues an email, src/jobs/process-order.ts:10).
  - The compiled path is unchanged; the chat says the step is only proposed: no code implements it.
- **Never:** a status or "done" in the words; a card, connection, or label for code that doesn't exist; a proposal
  on a callee shown in place; proposals added while only explaining code.

## Tuning

- **Where the teaching lives:** `instructions.ts` (what every session starts with), `guide/*.md` (the topics),
  `tools/*.ts` (each tool's description, and its results' and refusals' wording), `results.ts` and `format.ts`
  (shared wording), and `tools/prompts.ts` (the prompts).
- **Change one thing at a time,** run `pnpm --filter @unfold/desktop test` (`teaching.test.ts` checks the lengths,
  that texts name only tools, arguments, and topics that exist, and that every tool is taught), rebuild the app,
  and run the same tasks with both agents again.
- **Keep what helps:** a change stays when facts or rules improve and effort doesn't grow. Write what you changed
  and why next to the scores.
