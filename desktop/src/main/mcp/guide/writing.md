# Writing on boards

Your words are read by the person, then by teammates who open the published board.
They sit beside compiled code, so they say what the code is for and why, in plain
language, and they stay true: Unfold signs each text against the code it describes,
and it goes to review when that code changes.

## Rules for every text
- Plain words. Never raw code, never step numbers ("1.", "Step 2"), never ids or keys.
- What and why, not how. Don't restate what the card already shows.
- Unfold's names are fine in words: createOrder, OrderService.place, POST /orders.
- One idea per text. A description is one quiet line on the board, with "more".
- Only what you read in the code. If you aren't sure, leave it out and ask in chat.

## What to write, where
| What | Tool | Rule | Good | Not |
| --- | --- | --- | --- | --- |
| Flow label | label_steps | 2 to 8 words: a verb, or a question for a condition | "Did the client ask for an async reply?" | `req.header("prefer") === "respond-async"`, "Step 2" |
| Step group title | group_steps | What the steps achieve, 2 to 6 words | "Queue it for a worker, reply 202" | "Lines 28–30" |
| Proposed step title | propose | The intended behavior, 2 to 6 words, only when asked | "Email a confirmation" | "TODO", "Would call sendEmail" |
| Frame title | frame_cards | A noun, 1 to 4 words | "Request checks", "Order API" | "Group 1", "Frame" |
| Description | describe (or the description field of group_steps, frame_cards, add_resources) | One sentence: what and why, not how | "Places the order now, or queues it when the client asks for an async reply." | A paragraph that restates the code |
| Connection label | connect | What travels, 1 to 5 words | "place-order job" | "connection", "calls" |
| Connection description | connect | The evidence, one sentence | "createOrder adds a place-order job to the orders queue." | "They are connected." |
| Note | add_notes | Two sentences at most, for what a reader would otherwise miss | "Queued orders are placed by the worker. The client gets a job id, not the order." | A summary of the page |
| Page title | create_board, edit_pages | What the page explains, 1 to 6 words | "Placing an order" | "Page 2", "Diagram" |
| Board title | create_board | The story | "Order placement" | "Board for ts-orders" |

Limits: Flow labels and step group titles 120 characters, frame titles 80,
descriptions and notes 400.

## Flow labels
- Each step of an expanded card shows a fallback name from the code (dotted underline)
  until it has a label. read_code with view "flow" lists the steps with their keys;
  read_page lists the ones still unlabeled and check counts them per card, leaving out
  the steps a collapsed step group hides; label those too, since they show when the
  person expands the group.
- Say what happens: "Charge the customer", not "this.payments.charge". A condition is
  a yes-or-no question; its branches already read yes and no. A return says what comes
  back, or why the function stops there: "Stop: the worker places it".
- Start and End take no label.

For createOrder (src/routes/orders.ts:25–35):
```
label_steps({board, page, card: "create-order", labels: [
  {step: "src/routes/orders.ts#function:createOrder/variable:draft", text: "Read the order draft"},
  {step: "src/routes/orders.ts#function:createOrder/if[0]", text: "Did the client ask for an async reply?"},
  {step: "src/routes/orders.ts#function:createOrder/if[0]/variable:job", text: "Queue a place-order job"},
  {step: "src/routes/orders.ts#function:createOrder/if[0]/expression_statement[0]", text: "Reply 202 with the job id"},
  {step: "src/routes/orders.ts#function:createOrder/if[0]/return[0]", text: "Stop: the worker places it"},
  {step: "src/routes/orders.ts#function:createOrder/variable:order", text: "Place the order now"},
  {step: "src/routes/orders.ts#function:createOrder/expression_statement[0]", text: "Tell #orders on Slack"},
  {step: "src/routes/orders.ts#function:createOrder/expression_statement[1]", text: "Reply 201 with the order"}]})
```

## Titles and descriptions
- A step group's title says what its steps achieve together; its description is
  optional.
- A proposed step's title says what would happen, never that it happens; its reason
  says why the person wants it ("The queued path emails a confirmation; placing it now
  doesn't."). No status words: "done", "built", "pending".
- A frame's title names the unit or service; its description says in one sentence
  what it does ("The Express API: requests come in, orders are placed or queued.").
- A card's description says what the function does for this page's question.
- A resource card's description says what it holds and where it lives ("Orders by id,
  in memory for each process.").
- Write the same text again and it's kept, not rewritten.

## Notes
A note is for a caveat, a reading order, or an open question. It isn't checked against
code and never goes to review, so never use one to claim what code does: describe a
card, or connect with anchors. Attach it to the item it's about. Add notes sparingly.

## People's words
Text a person wrote is theirs. describe, label_steps, propose, and remove_items refuse
to replace or remove it (a person's proposed step too), and edit_pages refuses to
delete a page that holds it, unless replacePeopleText is true: set it only when the
person asked for that change. The other tools always keep it: pass the person's text
as it is. A step group titled before titles were signed counts as a person's.

## In the chat
- Name things as Unfold shows them, with file:line. Never ids or keys.
- After building, say what you made, which parts go beyond what Unfold compiled (your
  connections, tables, and notes, each with what it cites), and what you couldn't
  cite.
