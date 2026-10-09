# v2 · 10 · Board across projects

A draft board in `storefront-api` whose page holds cards from three projects: `checkout` from **storefront-web** (W), `POST /orders`
and `createOrder` from **storefront-api** (S), and `sendReceipt` from **billing-worker** (B), which this machine has not connected.
Two hand-drawn links join cards across projects: "calls over HTTP" (drawn by Lazar, **selected**) and "queues a receipt" (drawn by
Maya). The sidebar's Selected section shows the link. Light theme. Shared chrome is in [components.md](components.md).

Artboard: 1440×900, background `#F7F8FA`.

## 1. Regions (window coordinates)

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Tab strip | 0 | 0 | 1440 | 38 | |
| Tab "Home" | 84 | 7 | 82 | 30 | inactive |
| Tab "Order lifecycle" | 168 | 7 | 130 | 30 | inactive (drawn without transparent border; use 02's 133) |
| Tab "Cart → API trace" | 300 | 7 | 143 | 30 | **active** |
| Sidebar | 0 | 38 | 264 | 862 | |
| Sidebar scroll region | 0 | 38 | 263 | 591 | |
| · Board header | 8 | 46 | 247 | 60 | |
| · Pages | 8 | 122 | 247 | 84 | 2 rows |
| · On this page | 8 | 222 | 247 | 196 | 6 rows, no hint row |
| Selected section (link) | 0 | 629 | 263 | 271 | |
| Top bar | 264 | 38 | 1176 | 48 | |
| Stage | 264 | 86 | 1176 | 814 | |
| Dock | 557 | 836 | 590 | 40 | narrower trail |

Stage contents (Stage `left/top`; window = +264, +86):

| Element | Stage x, y | Window x, y | w × h |
| --- | --- | --- | --- |
| Links SVG | 440, 90 | 704, 176 | 580 × 330 |
| Card `checkout` (W, Code) | 32, 96 | 296, 182 | 412 × 256 |
| Link label "calls over HTTP · Lazar" (selected) | 458, 161 | 722, 247 | 154 × 24 |
| Entrance card `POST /orders` (S) | 624, 96 | 888, 182 | 200 × 163 |
| Card `createOrder` (S, compact) | 856, 96 | 1120, 182 | 288 × 185 |
| Link label "queues a receipt · Maya" | 930, 333 | 1194, 419 | 158 × 24 |
| Placeholder card `sendReceipt` (B) | 856, 408 | 1120, 494 | 288 × 165 |

(Paper layer names on this artboard are stale — e.g. the `checkout` card is named "Card · createOrder (selected)". Go by content.)

## 2. Copy (verbatim)

- Tabs: "Home", "Order lifecycle", "Cart → API trace" (monogram "S" on both board tabs)
- Breadcrumb: "storefront-api" (no folder); title: "Cart → API trace"
- Pages: "Checkout call" (selected), "Webhook"
- On this page ("follow order"):
  1. [W] ƒ `checkout`
  2. [ ] dashed-arrow "calls over HTTP" (selected)
  3. [S] entrance icon `POST /orders`
  4. [S] ƒ `createOrder`
  5. [ ] dashed-arrow "queues a receipt"
  6. [B] ƒ `sendReceipt` (muted) + "not connected"
- Selected: "Link" "· drawn by Lazar · 5 min ago"; "Label" "calls over HTTP"; "From" [W] `checkout`; "To" [S] `POST /orders`;
  note "Drawn by hand. Unfold doesn’t trace calls between projects; health checks that both ends still exist."; buttons "Edit label",
  "Remove link"
- Top bar: "Checkout call", "Draft · uses storefront-web", "Code", "Flow", "Share", "Publish"
- Dock: "Add", "A", `checkout`, `POST /orders`, "Arrange", "100%", "Fit"
- Link labels: "calls over HTTP" "· Lazar"; "queues a receipt" "· Maya"
- Placeholder: "B", "From billing-worker · not connected", "ƒ sendReceipt", "Maya added this card. Connect the project to read it.",
  "Connect project"
- `checkout` card: "W", "ƒ", "checkout", "app/api/cart/checkout.ts", "Code", "Flow", "POST" "/api/cart/checkout"
- `createOrder` card: "ƒ", "createOrder", "orders/create.ts", "Checks the cart, holds the stock, then charges the card."

**Copy conflict with the redesign doc (§4.8):** the doc says placeholders read "From storefront-web, not connected · Connect". The
artboard uses "From billing-worker · not connected" + an explanatory line + a "Connect project" button. Follow the artboard (it is
newer and names the teammate), or update the doc.

## 3. Sidebar differences from 02

- Breadcrumb shows only the project when the board isn't in a folder.
- **On this page** rows carry a project column: `[14px monogram or 14px empty spacer] [16px glyph] [label] [16px trailing]`, gap 8,
  padding 0 8. Rows are **not indented** here (02 indents 16px per depth). Decision needed: either indent after the monogram column
  (monogram stays in a fixed lane at x 16, glyph moves right by 16 per depth), or drop the monogram when all cards share the board's
  project (02 shows no monogram column). Recommended: show the monogram column only when the page holds cards from more than one
  project, keep it in a fixed lane, and indent the glyph+label after it.
- Drawn links appear as rows in follow order, between the two cards they join, with the dashed-arrow icon
  (components §4.5). Selected link row: fill `#EDF0FC`, icon `#5862C0`, label Inter 13/20 500.
- A card from an unconnected project: label and ƒ in `#656974`, trailing "not connected" Inter 12/16 `#656974` in place of the
  16px trailing slot.
- No "Label them" hint row (no Flow steps on this page).
- Selected section shows a **link** (components §4.6, "For a drawn link"). Relation label column is 72px here vs 88px for cards.

## 4. Top bar difference

Publish state: dot `#94A3B8` (faint) + "Draft · uses storefront-web". Pattern: `<state> · uses <other project names>`.

## 5. Stage components

### 5.1 Cross-project function card `checkout` (412 wide)

Card shell as in [02 §4](02-board-canvas.md#4-cards-restyled--differences-from-uisrccanvascards), unselected (1px `#E0E2E7`).

- **Header** (padding 10 12, gap 8, border-bottom `#E0E2E7`): **20px monogram "W"** (`#E6E8EC`, radius 5, Inter 11/14 600 `#3F4451`)
  -> "ƒ" Mono 13/16 700 **`#5862C0`** (accent although the card is not selected; in 02 the accent ƒ marks the selected card — treat
  this as a mockup slip and use muted unless selected) -> `checkout` Mono 13/16 600 `#202126` -> `app/api/cart/checkout.ts` Mono
  11/14 `#656974` (`flex: 1`) -> mini lens (Code active).
- **Route row** (new): flex centered, gap 8, padding **8px 12px**, border-bottom 1px `#E0E2E7`. "POST" Mono 11/14 `#656974`;
  "/api/cart/checkout" Mono 12/16 `#202126`. A hidden third span "· storefront-web" (Inter 12/16 muted, `display: none`) exists in
  the layer — a variant that names the project in text; not shown.
- **Code** (padding 8 top, 10 bottom; 20px rows; 40px gutter, numbers right-aligned with 14px right padding, `#64748B`; Mono 12/16):

  ```
   6 export async function checkout(req: Request) {
   7   const cart = await getCart(req)
   8   const res = await fetch('/orders', {
   9     method: 'POST',
  10     body: JSON.stringify(cart),
  11   })
  12   return Response.json(await res.json())
  13 }
  ```

  Colors use the syntax tokens: keyword, function (`checkout`), type (`Request`), string (`'/orders'`, `'POST'`), property
  (`method`, `body`), plain. `getCart` is an accent underlined call link (400). Row 8 — the row the drawn link leaves from — has fill
  `#F4F5F8` (step).

### 5.2 Entrance card `POST /orders` (200 wide)

Same as 02's entrance card at 200 wide (header "POST" "/orders"; `src/routes/orders.ts:22`; "Handlers, in order"; `requireAuth`,
`rateLimit`, `createOrder` in accent 600 underlined). No monogram on it although it is from a different project than `checkout`:
**only cards from a project other than the board's own get a monogram** (the board lives in storefront-api; `checkout` is W).

### 5.3 Compact function card `createOrder` (288 wide)

Header: "ƒ" muted, `createOrder` (flex 1), trailing "orders/create.ts" (no lens). Body: flex column, gap 8, padding 10 12 12:
- Description: Inter 12/18 `#A8346F` (wraps to two lines; no strip, no divider, no "more").
- Four 20px rows of calls only, no line numbers: `await validateCart(…)`, `await reserveInventory(…)`, `await chargePayment(…)`,
  `await queue.add('receipt', …)` — keyword `await`, accent underlined call names (400), plain `(…)`, string `'receipt'`.

This "calls only" body is a different card mode from 02's full code; it isn't in today's components. Decide whether it is a real
mode (e.g. a folded or summary view) or a mockup shortcut.

### 5.4 Placeholder card (project not connected)

| Property | Value |
| --- | --- |
| Box | 288 wide; padding **14**; flex column, gap **12** |
| Fill | `#F1F2F5` (untokened) |
| Border | **1.5px dashed** `#B6BCC8` (untokened); radius 10 |
| Shadow | none |
| Row 1 | flex centered, gap 8: 20px monogram "B" + "From billing-worker · not connected" Inter 12/16 **500** `#656974` |
| Row 2 | "ƒ sendReceipt" JetBrains Mono 12/16 `#656974` |
| Row 3 | "Maya added this card. Connect the project to read it." Inter 12/16 `#656974` |
| Row 4 | Secondary button "Connect project" (h30, white, 1px `#E0E2E7`, radius 6, padding 0 12, Inter 13/16 500 `#202126`), `align-self: flex-start` |

Today's placeholder is `.board-card.placeholder { border-style: dashed }` on a normal card; the new one has its own fill, a 1.5px
lighter dashed border, and no header/body split.

### 5.5 Links (drawn and graph)

SVG `viewBox="0 0 580 330"` at Stage (440, 90):

```
<!-- drawn link, selected: checkout line 8 -> POST /orders header -->
<path d="M4 141 C 94 141, 94 25, 176 25" fill="none" stroke="#5862C0" stroke-width="1.75" stroke-dasharray="5 4"/>
<path d="M176 20.5 L184 25 L176 29.5 Z" fill="#5862C0"/>
<circle cx="4" cy="141" r="3.5" fill="#FFFFFF" stroke="#5862C0" stroke-width="1.5"/>

<!-- graph edge: entrance handler -> createOrder (solid, contour) -->
<path d="M384 147 L409 147" fill="none" stroke="#8B93A1" stroke-width="1.5"/>
<path d="M409 143 L416 147 L409 151 Z" fill="#8B93A1"/>

<!-- drawn link, not selected (teammate's): createOrder -> placeholder, vertical -->
<path d="M560 191 L560 311" fill="none" stroke="#8B93A1" stroke-width="1.5" stroke-dasharray="5 4"/>
<path d="M556 311 L560 318 L564 311 Z" fill="#8B93A1"/>
```

| Link kind | Stroke | Width | Dash | Start | Head |
| --- | --- | --- | --- | --- | --- |
| Drawn, selected | `#5862C0` | 1.75 | 5 4 | hollow dot r 3.5 (white fill, 1.5 accent ring) on the source card edge | filled triangle 8 long × 9 wide |
| Drawn, unselected | `#8B93A1` | 1.5 | 5 4 | none | triangle 7 × 8 |
| Graph edge (entrance -> handler) | `#8B93A1` | 1.5 | solid | none | triangle 7 × 8 |

"Signed" = the label carries the author's name.

### 5.6 Link labels (chips on the link)

| Property | Selected ("calls over HTTP · Lazar") | Unselected ("queues a receipt · Maya") |
| --- | --- | --- |
| Box | h 24, padding 0 8, gap 6, radius 6, flex centered | same |
| Fill | `#EDF0FC` (selection) | `#FFFFFF` |
| Border | 1px `#5862C0` | 1px `#E0E2E7` |
| Ring | `0 0 0 3px #5862C01F` | none |
| Label | Inter 12/16 **500** `#5862C0` | Inter 12/16 500 `#202126` |
| Author | "· Lazar" Inter 12/16 400 `#5862C0` | "· Maya" Inter 12/16 400 `#656974` |

The label sits on the curve's midpoint (selected) or straddles the vertical line (unselected).

## 6. States shown

Active tab is the second board tab; selected page; selected **link** (row, chip, curve, Selected section); unconnected-project
card (row + placeholder card); draft publish state; Code lens; Forward disabled. Not drawn: hovering a link, drawing a link (drag
handle), editing a label inline, a link whose end is missing (health), dark theme.
