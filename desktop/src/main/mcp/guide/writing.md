# Writing on boards

Your words are read by the person, then by teammates who open the board from git. They
sit beside compiled code, so they say what the code is for and why, in plain language.
Unfold signs each text against the code it describes; when that code changes, the text
goes to review.

## Rules for every text
- The fewest words a newcomer needs. Code names stay, and a card the page isn't about
  gets no words of yours.
- Plain words. Never raw code, step numbers ("1.", "Step 2"), ids, or keys.
- What and why, not how. Don't restate what the card already shows.
- Code names are fine in words: createOrder, OrderService.place, POST /orders.
- One idea per text.
- Only what you read in the code. If you aren't sure, leave it out and ask in chat.

## What to write, where
| What | Tool | Rule | Good | Not |
| --- | --- | --- | --- | --- |
| Card title | shape_card title | Only when the code name says nothing about what the card does, on a card the page is about; about 3 on a page. 1 to 5 words; the code name shows beside it | "Read options, then sync" on `main` | "Place an order" on createOrder; "createOrder function" |
| Name | shape_card names | What the block does, 2 to 6 words, on an expanded card; a branch as a yes-or-no question | "Reply 201 with the order", "Client wants an async reply?" | "Call res.status().json" |
| Combined step title | shape_card combine | What the steps do together, 2 to 6 words, on an expanded card | "Queue it for a worker" | "Steps 3–5" |
| Description | shape_card for a card; describe for a resource card, combined step, or connection; the description of add_resources and frame_cards | One sentence: what and why. On the 1 to 3 cards the page is about, each resource card, and each connection; never on a context card or an entrance's | "Places the order now, or queues it when the client asks for an async reply." | A paragraph that restates the code; one on every card |
| Frame title | frame_cards | A noun, 1 to 4 words; a description only when the title can't say it | "Server", "Request checks" | "Group 1" |
| Connection label | connect | What travels, 1 to 5 words | "place-order job" | "calls" |
| Connection description | connect | The evidence, one sentence | "createOrder adds a place-order job to the orders queue." | "They are connected." |
| Proposed step | propose | The intended behavior, 2 to 6 words, and a reason; only when asked | "Email a confirmation" | "Later", "Done" |
| Note | add_notes | At most 2 on a page, two sentences each: a caveat or an open question a reader would miss | "Queued orders are placed by the worker. The client gets a job id, not the order." | "START HERE", "Runs every 6 hours", "LIVE: …", a summary of the page |
| Page title | create_board, edit_pages | What the page explains, 1 to 6 words | "Placing an order" | "Page 2" |
| Board title | create_board | The story | "Order placement" | "Board for ts-orders" |

Limits: titles and names 120 characters, frame titles 80, descriptions and notes 400.

## Names and titles
Name only what the code doesn't already say. `orderService.place` reads fine as it is;
`res.status().json` reads better as "Reply 201 with the order". The code name stays
beside yours, so never repeat it. Name blocks only on expanded cards. A card keeps its
code name: createOrder needs no title, `main` may get one.

## Notes
A note is for a caveat or an open question a reader would miss: at most 2 on a page.
It isn't checked against code and never goes to review, so never use one to claim
what code does: give the card a description with shape_card, or connect with anchors.
Never schedules, reading order, or what a card, an entrance, or a connection already
says (an entrance's on says when it runs, so declare the entrance instead), and no
status (live, broken, counts, dates) unless the person asks for it on the page.
Attach a note to the item it's about.

## People's words
Text a person wrote is theirs. shape_card, describe, propose, and remove_items refuse
to replace or remove it, and edit_pages refuses to delete a page that holds it, unless
replacePeopleText is true: set it only when the person asked for that change. The other
tools keep it: pass the person's text as it is.

## In the chat
- Name things as Unfold shows them, with file:line. Never ids or keys.
- Status and findings from outside the code (a production check, logs, a database's
  rows) go here, not on the page, unless the person asks for them there.
- After building, say what you made, which parts go beyond what Unfold compiled (your
  connections, tables, and notes, each with what it cites), and what you couldn't cite.
