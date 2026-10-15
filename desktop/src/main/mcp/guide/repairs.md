# Repairs: keeping boards true after the code changes

Everything written on a board remembers the code it was written for. When that code
changes, Unfold never drops the text and never moves it to other code: the item goes to
review, and the board's health lists it with the repairs that fit. Apply them with
repair, one at a time, after reading the new code.

## Finding what needs repair
- list_boards shows each board's health ("needs review: 2 missing · 1 broken
  connection").
- check({board}) lists every page's problems, each with its item and the exact repair
  calls it allows, as many pages as fit: pass its cursor for the rest. check({page})
  does one page.
- A problem's item is the id of a card, combined step, connection, resource card,
  frame, or proposed step, or, for a name, the block's key.

## Problems and the repairs that fit
| Problem | When | Repairs |
| --- | --- | --- |
| missing | The code under a card, name, or proposed step is gone (renamed, moved, deleted); a resource card's type or anchor is gone | reconnect to the new key, or remove |
| outdated | The code under a description, name, combined step, connection, resource card, or frame description changed | resign when the text is still true; else rewrite it, remove_description, or remove |
| held for review | A key now names code of another kind or owner, or a position key (`if[0]`) whose code changed | accept when it's still the same code; else reconnect or remove |
| broken combined step | One of its blocks is gone, or they no longer follow the rules, or it overlaps another | reconnect a gone block; else remove |
| broken connection | An end card is missing; its step, carried type, or anchor is gone | reconnect the step, type, or anchor; or remove |

## Reconnect
- After a rename or a move, find the new key: search the new name, get_relations on a
  caller, or read_file on the file.
- repair "reconnect" with to: the new key. It reconnects that key everywhere on the
  page: the card, the names and combined steps on its blocks, and the types and anchors
  that cite it. The steps of connections that leave the card move along.
- A resource card's or connection's anchor: pass anchor (its position, which check
  gives) when several have problems, and to: a key or an evidence file's lines.
- Reconnected text whose code changed is outdated next: read it, then re-sign or
  rewrite.

## Re-sign and accept: whose sign-off it is
Re-signing says "this text is still right for the new code"; accepting says "this
reference still means the same code". Both vouch for words, so:
- **Re-sign or accept only what agents wrote**, after reading the new code
  (read_code). A text you'd write differently now: rewrite it.
- **What a person wrote is theirs to sign off.** repair refuses it, and check shows it
  as theirs. Tell them in chat what changed and whether their text still reads right.
- Accepting a card vouches for its description and the names and combined steps on
  its blocks too.

## Removing
remove takes the item off the page: a card with its connections, a resource card with
its connections, a combined step (its blocks stay), a name, a connection, or a proposed
step. Never code. Remove only what's gone for good, or what the person asked to remove:
a reconnect keeps everything written about the code. A removal that takes a person's
words (their description, name, combined step, or connection, or one going with the
item) is refused unless replacePeopleText is true: pass it only when they asked.

## Worked example
"I renamed OrderService.place to placeOrder. Fix my boards."
- list_boards: Order system needs review.
- check({board}): the card for OrderService.place is missing, and the connections that
  leave it are broken.
- search({query: "placeOrder", kinds: ["functions"]}) finds OrderService.placeOrder at
  src/services/order-service.ts:13.
- repair reconnects the card to `src/services/order-service.ts#OrderService.placeOrder`.
  Its connections' steps move along, so they're whole again.
- check again: the descriptions of createOrder and the Server frame are outdated,
  since their code changed. read_code shows both still hold, and an agent wrote them,
  so repair re-signs them. A description the person wrote is theirs to re-sign.
- finish_work, then in chat: what you reconnected and re-signed, and what is the
  person's to sign off, with what changed in the code.
