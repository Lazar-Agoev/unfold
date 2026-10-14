# Writing on boards

Your words are read by the person, then by teammates who open the board from git. They
sit beside compiled code, so they say what the code is for and why, in plain language.
Unfold signs each text against the code it describes; when that code changes, the text
goes to review.

## Rules for every text
- Plain words. Never raw code, step numbers ("1.", "Step 2"), ids, or keys.
- What and why, not how. Don't restate what the card already shows.
- Code names are fine in words: createOrder, OrderService.place, POST /orders.
- One idea per text.
- Only what you read in the code. If you aren't sure, leave it out and ask in chat.

## What to write, where
| What | Tool | Rule | Good | Not |
| --- | --- | --- | --- | --- |
| Card title | shape_card title | What the card does, 1 to 5 words | "Place an order" | "createOrder function" |
| Name | shape_card names | What the block does, 2 to 6 words; a branch as a yes-or-no question | "Reply 201 with the order", "Client wants an async reply?" | "Call res.status().json" |
| Combined step title | shape_card combine | What the steps do together, 2 to 6 words | "Queue it for a worker" | "Steps 3–5" |
| Description | shape_card for a card (an app's, entrance's, or type's too); describe for a combined step or connection; the description of frame_cards and add_resources | One sentence: what and why | "Places the order now, or queues it when the client asks for an async reply." | A paragraph that restates the code |
| Frame title | frame_cards | A noun, 1 to 4 words | "Request checks", "Server" | "Group 1" |
| Connection label | connect | What travels, 1 to 5 words | "place-order job" | "calls" |
| Connection description | connect | The evidence, one sentence | "createOrder adds a place-order job to the orders queue." | "They are connected." |
| Proposed step | propose | The intended behavior, 2 to 6 words, and a reason; only when asked | "Email a confirmation" | "Later", "Done" |
| Note | add_notes | Two sentences at most, for what a reader would miss | "Queued orders are placed by the worker. The client gets a job id, not the order." | A summary of the page |
| Page title | create_board, edit_pages | What the page explains, 1 to 6 words | "Placing an order" | "Page 2" |
| Board title | create_board | The story | "Order placement" | "Board for ts-orders" |

Limits: titles and names 120 characters, frame titles 80, descriptions and notes 400.

## Names
Name only what the code doesn't already say. `orderService.place` reads fine as it is;
`res.status().json` reads better as "Reply 201 with the order". The code name stays
beside yours, so never repeat it.

## Notes
A note is for a caveat, a reading order, or an open question. It isn't checked against
code and never goes to review, so never use one to claim what code does: give the
card a description with shape_card, or connect with anchors. Attach it to the item it's
about, and add notes sparingly.

## People's words
Text a person wrote is theirs. shape_card, describe, propose, and remove_items refuse
to replace or remove it, and edit_pages refuses to delete a page that holds it, unless
replacePeopleText is true: set it only when the person asked for that change. The other
tools keep it: pass the person's text as it is.

## In the chat
- Name things as Unfold shows them, with file:line. Never ids or keys.
- After building, say what you made, which parts go beyond what Unfold compiled (your
  connections, tables, and notes, each with what it cites), and what you couldn't cite.
