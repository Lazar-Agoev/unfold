# Repairs: keeping boards true after the code changes

Everything written on a board remembers the code it was written for. When that code
changes, Unfold never drops the text and never attaches it to different code: the item
goes to review, and the board's health lists it with the repairs that fit. You apply
them with repair, one at a time, after reading the new code.

## Finding what needs repair
- list_boards shows each board's health ("needs review: 2 missing · 1 broken connection").
- check({board}) lists every page's problems, each with its item and the exact repair
  calls it allows: `repair({board: "…", page: "…", item: "…", repair: "reconnect",
  to: <the new key>})`. check({page}) does one page.
- A problem's item is a card's, combined step's, connection's, resource card's,
  frame's, or proposed step's id, or, for a name, the block's key.

## Problems and the repairs that fit
| Problem | When | Repairs |
| --- | --- | --- |
| missing | A card's or a name's code is gone (renamed, moved, deleted); a resource card's type or anchor is gone; an evidence file isn't recorded or got shorter | reconnect to the new key; or remove |
| outdated | The code under a description, name, combined step, connection, resource card, or frame description changed | resign when the text is still true; else rewrite it (describe) or remove_description, remove |
| held for review | A guarded key now names code of another kind or owner, or a position-based key (`if[0]`) whose code changed | accept when it's still the same code; else reconnect, or remove |
| broken combined step | Its blocks are no longer consecutive blocks of one body | remove |
| broken connection | An end card is missing; a connection's step, carried type, or anchor is gone, or its step left its card | reconnect the step, type, or anchor; or remove |
| proposed step missing or held | The block a proposed step goes after (or starts) is gone, or now names code of another kind or owner | reconnect to that block's new key, accept when it's still that block, or remove; propose with its id and a new after or start moves it too |

## Reconnect
- After a rename or a move, find the new key: search the new name, get_relations on a
  caller, or read_file on the file.
- repair with repair "reconnect" and to: the new key. It reconnects that key
  everywhere on the page: the card, the combined steps and names on its blocks, and
  tables' types, anchors, and carried types that cite it. The steps of connections that
  leave the card move along, to the same calls under the new key.
- An anchor of a resource card or connection: pass anchor (its position, which check
  gives) when several have problems, and to: a key, or an evidence file's lines
  `{file, lines: [from, to]}`.
- Reconnected text whose code changed is outdated next: read it and re-sign or rewrite.

## Re-sign and accept: whose sign-off it is
Re-signing says "this text is still right for the new code"; accepting says "this
reference still means the same code". Both vouch for words, so:
- **You may re-sign or accept only what agents wrote**, and only after reading the new
  code (read_code). A text you'd write differently now: rewrite it instead.
- **What a person wrote is theirs to sign off.** repair refuses it, and check shows it
  as theirs to do. Tell them in chat what changed in the code and whether their text
  still reads right to you.
- What a repair vouches for: re-signing a card, its description; accepting a card, its
  description and the combined steps and names inside it too; a combined step, its
  title and description; a name, its text; a connection, the connection with its label
  and description; a resource card, the card and its description; a frame, its
  description.

## Removing
remove takes the item off the page: a card with the connections to it, a resource card
with its connections, a combined step (its blocks stay), a name, a connection, or a
proposed step (never code). A frame left empty goes too, and a note attached to what's removed stays, free. Remove
only what's gone for good, or what the person asked to remove: removing is undoable,
but a reconnect keeps everything written about the code.

## Worked example
"I renamed OrderService.place to placeOrder. Fix my boards."
- list_boards: Order placement needs review: 2 missing · 1 broken connection.
- check({board}): the card for OrderService.place is missing on two pages, and the
  writes connection that leaves it is broken.
- search({query: "placeOrder", kinds: ["functions"]}) finds OrderService.placeOrder at
  src/services/order-service.ts:13.
- repair reconnects the card on each page to
  `src/services/order-service.ts#OrderService/placeOrder`. The
  connection's step moves along, so the connection is whole again.
- check again: two descriptions are outdated, the person's on the card and yours on
  the frame "shared". read_code on placeOrder shows the frame's description still
  holds, so repair re-signs it.
- finish_work, then in chat: "I reconnected 2 cards and re-signed my description of the
  shared frame. Your description on OrderService.placeOrder is outdated because the
  method changed; it still reads right to me, but re-signing it is yours to do."
