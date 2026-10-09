# v2 · 02 · Board — canvas

A board open in a tab, light theme: the window tab strip, the board-mode sidebar (header, Pages, On this page, Selected), the canvas
top bar, four cards joined by arrows, and the floating dock. One card (`createOrder`) is selected; one (`reserveInventory`) is in
Flow and needs review. Shared chrome is specified in [components.md](components.md); this file gives geometry, copy, states, and
the card restyle.

Artboard: 1440×900, flex column, `overflow: clip`, background `#F7F8FA`.

## 1. Regions (window coordinates)

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Tab strip | 0 | 0 | 1440 | 38 | components §2 |
| Traffic lights | 16 | 13 | 52 | 12 | |
| Tab "Home" | 84 | 7 | 82 | 30 | inactive |
| Tab "Order lifecycle" | 168 | 7 | 133 | 30 | active |
| Tab "Cart → API trace" | 303 | 7 | 141 | 30 | inactive |
| Sidebar | 0 | 38 | 264 | 862 | incl. 1px right border |
| Sidebar scroll region | 0 | 38 | 263 | 567 | padding 8/8/16, gap 16 |
| · Board header | 8 | 46 | 247 | 60 | breadcrumb 46–74, title 74–106 |
| · Pages | 8 | 122 | 247 | 112 | header + 3 rows |
| · On this page | 8 | 250 | 247 | 196 | header + 5 layer rows + hint |
| Selected section | 0 | 605 | 263 | 295 | border-top; padding 8/8/16; gap 8 |
| Canvas area | 264 | 38 | 1176 | 862 | |
| Top bar | 264 | 38 | 1176 | 48 | |
| Lens | 790 | 47 | 124 | 30 | centered on the top bar (`left: 50%`, `top: 9`) |
| Share | 1273 | 47 | 77 | 30 | |
| Publish | 1358 | 47 | 70 | 30 | right padding 12 |
| Stage | 264 | 86 | 1176 | 814 | `position: relative; overflow: clip` |
| Dock | 546 | 836 | 612 | 40 | `left: 50%; bottom: 24; translate: -50%` |

Cards and arrows (positions are `left/top` inside the Stage; window = +264, +86):

| Element | Stage x, y | Window x, y | w × h |
| --- | --- | --- | --- |
| Arrows SVG | 268, 50 | 532, 136 | 596 × 310 |
| Entrance card `POST /orders` | 32, 96 | 296, 182 | 240 × 163 |
| `createOrder` (selected, Code) | 328, 56 | 592, 142 | 452 × 319 |
| `reserveInventory` (Flow, needs review) | 860, 40 | 1124, 126 | 296 × 234 |
| `chargePayment` (Code excerpt) | 860, 330 | 1124, 416 | 296 × 177 |

## 2. Copy (verbatim)

- Tabs: "Home", "Order lifecycle", "Cart → API trace" (monogram "S" on both board tabs)
- Breadcrumb: "storefront-api / Checkout"; title: "Order lifecycle"
- Section labels: "Pages", "On this page", "Selected" (rendered uppercase)
- Pages: "Placing an order" (selected), "Payment succeeded", "Refunds" (review dot)
- On this page: trailing "follow order"; rows `POST /orders` (d0), `createOrder` (d1, selected), "Hold stock, then charge" (d2 group),
  `reserveInventory` (d3, review dot), `chargePayment` (d3)
- Hint: "2 Flow steps unlabeled ·" + "Label them"
- Selected: `createOrder`, `src/orders/create.ts:14`, "Checks the cart, holds the stock, then charges the card. Gives the stock back if
  the charge fails.", "Lazar · 2 h ago", "Called by" `ordersRouter`, "Reached from" `POST /orders`, "Not opened" `validateCart` "+ Open"
- Top bar: "Placing an order", "Published · 2 unpublished changes", "Code", "Flow", "Share", "Publish"
- Dock: "Add", "A", `POST /orders`, `createOrder`, "Arrange", "100%", "Fit"
- Cards: see 4.

## 3. States shown

| State | Where | Spec |
| --- | --- | --- |
| Active tab | "Order lifecycle" | components §2.1 |
| Selected page | "Placing an order" | row fill `#EDF0FC`, page icon `#5862C0`, label 500 |
| Page needs review | "Refunds" | 6px `#D97706` dot in trailing slot |
| Selected layer | `createOrder` | fill `#EDF0FC`, ƒ `#5862C0`, label mono 600 |
| Layer needs review | `reserveInventory` | 6px `#D97706` dot |
| Selected card | `createOrder` card | 1.5px `#5862C0` border + 3px ring `#5862C01F` |
| Card needs review | `reserveInventory` card | review banner (4.4) |
| Published with unpublished changes | top bar | accent dot + muted text |
| Lens = Code | top bar | Code segment active |
| Forward disabled | dock | icon `#94A3B8` |
| Hover, focus, drag, multi-select | — | **not drawn** |

## 4. Cards (restyled — differences from `ui/src/canvas/cards/`)

The cards **are** restyled. Common card shell:

| Property | New design | Today (`cards.css`) |
| --- | --- | --- |
| Border | 1px `#E0E2E7` (border) | 1px `--color-contour` (`#8B93A1`, darker) |
| Shadow | **none** | `0 1px 2px rgb(20 24 40 / .06)` |
| Radius | 10 | 10 (`--radius-large`) — same |
| Fill | `#FFFFFF` | surface — same |
| Selected | border **1.5px** `#5862C0` + `0 0 0 3px #5862C01F` | 1px accent + `0 0 0 3px var(--color-selection)` |
| Width | content-sized; drawn at 200–452 | `min-width: 360px; max-width: 880px` — the new cards are narrower than today's minimum |
| Header | padding **10px 12px**, gap 8, border-bottom 1px `#E0E2E7`; no fold chevron drawn | min-height 44, padding 6/12/6/8, fold chevron first |
| Kind glyph | text "ƒ" JetBrains Mono 13/16 **700**; `#656974`, `#5862C0` when selected | `.card-kind` icon in `--color-syntax-function` |
| Title | JetBrains Mono **13/16** 600 `#202126` | mono 13/20 600 |
| Location | JetBrains Mono **11/14** `#656974`, `flex: 1` (pushes the lens right) or trailing | Inter 11.5/16 muted |
| Freshness | full-width **banner** under the header (4.4) | pill in the header (`.card-freshness`) |

### 4.1 Per-card Code | Flow mini lens (in the header, right)

Track: bg `#F3F4F6`, radius 6, padding 2, flex. Segment: padding 2px 8px; active: bg `#FFFFFF`, radius 4, Inter **11/14 600**
`#202126`; inactive: no bg, Inter 11/14 400 `#656974`. Copy "Code", "Flow". Shown on `createOrder` (Code active) and
`reserveInventory` (Flow active); **not** on `chargePayment` (header shows the file name there instead).

### 4.2 Description strip (`createOrder`)

Flex, gap 6, padding **8px 12px**, border-bottom 1px **`#F0F1F4`**. Text Inter **12/18** `#A8346F`, `flex: 1` ("Checks the cart, holds the
stock, then charges the card."). "more": Inter 12/16 `#A8346F`, **no underline** (today: accent-colored, underlined, 11px).

### 4.3 Code body

- Container: flex column, padding-top 8, padding-bottom 10.
- Row: height **20**, flex centered (today: min-height 21, font 12.5/21).
- Gutter: 40px; number right-aligned, 14px right padding; JetBrains Mono 12/16 **`#64748B`** (`--color-subtle`; today
  `--color-contour`, lighter). Code starts 40px from the card's inner edge.
- Code: JetBrains Mono **12/16**. Indentation is literal spaces (`white-space: pre-wrap`).
- Rows that send an arrow (16, 18) have fill `#F4F5F8` (step). Today a selected row uses `--color-selection`.
- Call links: `#5862C0`, `text-decoration: underline 1px; text-underline-position: from-font`. Calls whose card is open
  (`reserveInventory`, `chargePayment`) are weight **600**; unopened calls (`validateCart`, `toOrder`) 400.
- `release` (an unresolved method call): plain text with a 1px **dashed** bottom border `#8B93A1`.
- Syntax: keyword `#0033B3`, function name `#00627A`, number `#B4530A`, plain `#202126` (the v7 tokens, unchanged).
- `chargePayment` is an **excerpt without line numbers**: rows padding-left 14, same 20px rows; `create` in
  `stripe.paymentIntents.create` is `#DB2777` (`--color-external`).

createOrder code (lines 14–24, verbatim):
```
14 export async function createOrder(req, res) {
15   const cart = await validateCart(req.body)
16   const hold = await reserveInventory(cart.items)
17   try {
18     const charge = await chargePayment(cart, req.user)
19     return res.status(201).json(toOrder(cart, charge))
20   } catch (err) {
21     await hold.release()
22     throw err
23   }
24 }
```

chargePayment excerpt:
```
const intent = await
  stripe.paymentIntents.create({
    amount: cart.total,
    customer: user.stripeId,
  })
return toCharge(intent)
```

Header right of `chargePayment`: "payments.ts". Header of `createOrder`: "orders/create.ts".

### 4.4 Review banner

Directly under the header of `reserveInventory`: flex centered, gap 8, padding **7px 12px**, bg `#FFF3D6` (warn-fill), no border.
"Changed since it was described" Inter 12/16 400 `#8A5A00` (`flex: 1`); "Review" Inter 12/16 **600** `#8A5A00`. The card has
`overflow: clip` so the banner meets the rounded corners cleanly.

### 4.5 Entrance card `POST /orders` (240 wide)

Restyled (today: 1.5px accent border, "ENTRANCE" chip, 16px title, "Targets, in registration order" with role column):

- Header: padding 10px 12px, gap 8, border-bottom 1px `#E0E2E7`. Method "POST": JetBrains Mono **11/14 500** `#656974` (wrapper
  padding 2px 0, radius 4, no fill). Path "/orders": JetBrains Mono 13/16 600 `#202126`.
- Body: flex column, gap 8, padding 10 12 12.
  - Location `src/routes/orders.ts:22`: Mono 11/14 `#656974`
  - "Handlers, in order": Inter 11/14 600, 0.06em, uppercase, `#656974`
  - Handler list: flex column, gap 4; Mono 12/16 `#202126`: `requireAuth`, `rateLimit`; the opened one `createOrder` in `#5862C0`,
    600, underline 1px.
- No accent border; no kind chip.

### 4.6 Flow card `reserveInventory` (296 wide)

Header (ƒ muted, title flex 1, mini lens Flow-active) -> review banner -> Flow body: flex column centered, padding 14 12 16. Input pill
"items" (radius 999, padding 5/12, `#F1F5F9`/`#CBD5E1`/`#475569`), 1.5×12 connector `#CBD5E1`, decision "Every item in stock?"
(radius 6, padding 6/12, `#FEF3C7`/`#F3C66B`, text `#7A4B00` 500), two branches gap 16: "YES" -> "Hold each for 15 min"
(`#EFF6FF`/`#93C5FD`/`#1E3A8A`), "NO" -> "Out of stock" (`#FEE2E2`/`#FCA5A5`/`#991B1B`). Branch labels Inter 11/14 600, 0.04em,
`#656974`. All node text Inter 12/16. These are reader internals; keep today's Flow renderer and only check colors.

### 4.7 Arrows

SVG at Stage (268, 50), 596×310, `viewBox="0 0 596 310"`. Stroke 1.5, no fill. Arrowheads are filled triangles 7 long × 8 wide.

```
<path d="M4 187 C 34 187, 30 28, 58 28" stroke="#8B93A1"/>   entrance handler -> createOrder (contour)
<path d="M60 28 L53 24 L53 32 Z" fill="#8B93A1"/>
<path d="M512 143 C 556 143, 548 10, 590 10" stroke="#5862C0"/>  line 16 -> reserveInventory
<path d="M592 10 L585 6 L585 14 Z" fill="#5862C0"/>
<path d="M512 183 C 556 183, 548 300, 590 300" stroke="#5862C0"/>  line 18 -> chargePayment
<path d="M592 300 L585 296 L585 304 Z" fill="#5862C0"/>
```

Differences from today (`canvas.css`: every arrow `--color-accent`, 1.5, dashed variant `5 4`): the entrance -> handler arrow is
**contour gray**, not accent. Arrows leave from the card's right edge at the calling row's center and enter the target card's
header. No port dots are drawn (today: 10px accent handle dots at the card edge).
