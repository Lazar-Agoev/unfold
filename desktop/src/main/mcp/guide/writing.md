# Writing on boards

Your words are read by the person, then by teammates who open the board from git.
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
| Proposed step title | propose | The intended behavior, 2 to 6 words, only when asked | "Email a confirmation" | "TODO", "Would call sendEmail" |
| Frame title | frame_cards | A noun, 1 to 4 words | "Request checks", "Order API" | "Group 1", "Frame" |
| Description | describe (or the description field of frame_cards, add_resources) | One sentence: what and why, not how | "Places the order now, or queues it when the client asks for an async reply." | A paragraph that restates the code |
| Connection label | connect | What travels, 1 to 5 words | "place-order job" | "connection", "calls" |
| Connection description | connect | The evidence, one sentence | "createOrder adds a place-order job to the orders queue." | "They are connected." |
| Note | add_notes | Two sentences at most, for what a reader would otherwise miss | "Queued orders are placed by the worker. The client gets a job id, not the order." | A summary of the page |
| Page title | create_board, edit_pages | What the page explains, 1 to 6 words | "Placing an order" | "Page 2", "Diagram" |
| Board title | create_board | The story | "Order placement" | "Board for ts-orders" |

Limits: proposed step titles 120 characters, frame titles 80, descriptions and notes
400.

## Titles and descriptions
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
Text a person wrote is theirs. describe, propose, and remove_items refuse
to replace or remove it (a person's proposed step too), and edit_pages refuses to
delete a page that holds it, unless replacePeopleText is true: set it only when the
person asked for that change. The other tools always keep it: pass the person's text
as it is.

## In the chat
- Name things as Unfold shows them, with file:line. Never ids or keys.
- After building, say what you made, which parts go beyond what Unfold compiled (your
  connections, tables, and notes, each with what it cites), and what you couldn't
  cite.
