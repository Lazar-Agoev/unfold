# v2 · 09 · Source peek

A whole source file (`src/payments/stripe.ts`) open in the **peek sheet** over the board from 02. The sheet slides in from the right,
covering the right part of the canvas **and its top bar**; it is not pinned (the canvas is not narrowed). The file was opened from a
text match, so line 41 is the current match and line 59 holds another. The sheet's `⋯` menu is open. Light theme. Shared chrome is in
[components.md](components.md).

Artboard: 1440×900. Everything except the sheet and its menu is identical to [02](02-board-canvas.md) (same tabs, sidebar, cards and
positions), with two changes noted in 1.1.

## 1. Regions (window coordinates)

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Tab strip | 0 | 0 | 1440 | 38 | as 02 |
| Sidebar | 0 | 38 | 264 | 862 | as 02 |
| Canvas area (top bar + stage) | 264 | 38 | 1176 | 862 | as 02, partly covered |
| **Peek sheet** | **800** | **38** | **640** | **862** | absolute, child of the window root (sits over the top bar too) |
| · Sheet header | 800 | 38 | 640 | 48 | lines up with the top bar's height |
| · Code area | 800 | 86 | 640 | 778 | rows from y 94 (padding 8) |
| · Footer hint | 800 | 864 | 640 | 36 | |
| · Scrollbar | 1430 | 94 | 8 | 762 | `right: 2; top: 8; bottom: 8` inside the code area |
| **`⋯` menu** | 1196 | 80 | 208 | 66 | `top: 42; right: 36` inside the sheet |

### 1.1 What changes behind the sheet

- **Lens hidden.** The top bar's Code | Flow lens is `display: none` (it would sit under the sheet's left edge at x 790–914).
- **Dock removed.** The Stage has no dock (its center, x 852, would fall under the sheet).
- Share and Publish are covered by the sheet header. The page title and "Published · 2 unpublished changes" stay visible.

Decide: are lens and dock hidden **because a sheet is open**, or should they re-center on the uncovered canvas (x 264–800)? The doc
(§4.6) says pinning narrows the canvas; when pinned, re-centering on the narrowed canvas is the natural rule. Unpinned: the artboard
hides them.

## 2. Sheet container

| Property | Value |
| --- | --- |
| Width | **640** (fixed in the artboard; resize rule not specified) |
| Height | full body height (window minus the tab strip) |
| Fill | `#FFFFFF` |
| Left edge | 1px `#E0E2E7` (reader-light-border) |
| Radius | none |
| Shadow | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` (same as the dock) |
| Layout | flex column: header (48, `flex-shrink: 0`), code (`flex-grow: 1; min-height: 0`), footer (36, `flex-shrink: 0`) |
| Motion | slides in from the right (doc §4.6); duration not specified — use `--duration` 160ms, none with reduced motion |

## 3. Sheet header

Height **48**; flex centered; gap **10**; padding-left **16**, padding-right **8**; border-bottom 1px **`#E2E8F0`** (`--color-rule`, not
the `#E0E2E7` used elsewhere — pick one).

1. 20px monogram "S" (components §3).
2. Path: JetBrains Mono **13/20 500** `#202126` — "src/payments/stripe.ts"
3. Meta: Inter 12/16 400 `#656974` — "312 lines · TypeScript"
4. Spacer `flex-grow: 1`
5. Actions: flex, gap **2**, three 28×28 icon buttons (radius 6), x 1344 / 1374 / 1404:

| Button | State drawn | Icon (16×16, `viewBox="0 0 24 24"`, stroke 1.5, round caps/joins) |
| --- | --- | --- |
| Pin | rest (not pinned), stroke `#656974` | `<path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>` |
| More `⋯` | **open/pressed**: bg `#EEF0F3`, stroke `#202126` | `<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>` (stroked, not filled — differs from the sidebar's filled r 1.6 dots) |
| Close | rest, stroke `#656974` | `<path d="M18 6 6 18"/><path d="m6 6 12 12"/>` |

Pinned state is not drawn (suggest: pin icon in `#5862C0` on `#EDF0FC`, like a selected row).

## 4. `⋯` menu

Spec in [components.md §8](components.md#8-menus--popovers). Items: "Open in editor" with "⌘⇧O" (hovered: `#EDF0FC`), "Reveal in
Finder". Anchored 4px below the More button, right edge 2px right of the button's right edge (`right: 36` in the sheet).

## 5. Code area (read-only file)

Container: flex column, padding **8px 0**, `overflow: clip` (scrolls in code), `position: relative`.

### 5.1 Row anatomy (every line)

Height **20**; flex centered; padding-left **8**; then four lanes:

| Lane | Width | Content |
| --- | --- | --- |
| Add slot | **20** | empty; `⊕` on a definition **not** on the page; 7px accent dot on a definition **already** on the page |
| Line number | **36** | JetBrains Mono 12/16 `#64748B` (`--color-subtle`), right-aligned |
| Fold slot | **24** | empty; fold chevron on a definition line |
| Code | rest | JetBrains Mono **12/16**, `white-space: pre`, literal indentation |

So code starts 88px from the sheet's left edge (x 888 in the window). No right padding is set; long lines (line 22) run under the
menu/scrollbar — clip or scroll horizontally.

### 5.2 Affordances

| Affordance | Spec |
| --- | --- |
| `⊕` add (def not on page) | 14×14 SVG centered in the 20px slot, `viewBox="0 0 24 24"`, stroke `#656974` **1.75**, round caps: `<circle cx="12" cy="12" r="9"/><path d="M12 8v8"/><path d="M8 12h8"/>`. Drawn **at rest** on lines 35, 44, 53, 66 (not hover-only). Click adds that function to the current page. |
| On-page mark (def already on page) | 7×7 circle, radius 999, `#5862C0`, centered in the slot (line 26 `chargePayment`). |
| Fold, expanded | 12×12 chevron-down, `viewBox 0 0 24 24`, stroke **`#94A3B8`** 2, round: `<path d="m6 9 6 6 6-6"/>` (lines 26, 35, 53, 66) |
| Fold, folded | 12×12 chevron-right, stroke **`#656974`** 2: `<path d="m9 18 6-6-6-6"/>` (line 44), and the body replaced by a **fold pill** after ` { `: h 16, padding 0 5, radius 4, bg `#EEF0F3`, text "7 lines" JetBrains Mono 11/14 `#656974`, then ` }`. Line numbers jump 44 -> 52. |
| Call link | `#5862C0`, underline 1px, `text-underline-position: from-font`, weight 400 (`toCharge` 32, `markRefunded` 40 and 62, `markPaid` 60). Click follows / adds the callee. |
| Current text match line | row bg `#EDF0FC`, **inset left bar** `box-shadow: inset 2px 0 0 #5862C0`; line number `#5862C0` **600** |
| Current match span | bg `#5862C038` (accent 22 %), radius 2, ring `0 0 0 1px #5862C073` (accent 45 %); text keeps its syntax color |
| Other match span | bg `#EDF0FC`, radius 2, no ring (line 59) |
| Ligatures | the `===` span sets `font-feature-settings: "calt" 0, "liga" 0`; apply to all code so operators never ligate |

Only definitions get fold markers and add slots; blocks (`switch`, object literals) do not.

### 5.3 Scrollbar with match ticks

Overlay track 8 wide at `right: 2; top: 8; bottom: 8` (no track fill). Thumb: `left: 1`, 6×112, radius 3, `#0F172A24` (ink 14 %),
drawn at top 52. Match ticks: 8×2 at the match's relative position; current match `#5862C0` (top 98), others `#A0A8E0` (untokened;
tops 142, 468, 611).

### 5.4 Syntax colors

The v7 syntax tokens (same as in cards): keyword `--color-syntax-keyword` `#0033B3`, function names at definitions
`--color-syntax-function` `#00627A`, types `--color-syntax-type` `#7A5C14`, object keys `--color-syntax-property` `#871094`,
strings `--color-syntax-string` `#067D17`, everything else `--color-syntax-plain` `#202126`; call links accent.

### 5.5 File content shown (verbatim, with markers)

```
   22      const stripe = new Stripe(config.stripeKey, {
   23        apiVersion: '2024-06-20',
   24      })
   25
•  26  v   export async function chargePayment(cart: Cart, user: User) {
   27        const intent = await stripe.paymentIntents.create({
   28          amount: cart.total,
   29          currency: 'usd',
   30          customer: user.stripeId,
   31        })
   32        return toCharge(intent)
   33      }
   34
⊕  35  v   export async function refundCharge(charge: Charge, reason?: string) {
   36        const refund = await stripe.refunds.create({
   37          payment_intent: charge.intentId,
   38          reason: reason ?? 'requested_by_customer',
   39        })
   40        await markRefunded(charge.orderId, refund.id)
   41        return refund.status === 'succeeded'          <- current match: succeeded
   42      }
   43
⊕  44  >   export function toCharge(intent: Stripe.PaymentIntent): Charge { [7 lines] }
   52
⊕  53  v   export async function handleWebhook(req: Request) {
   54        const sig = req.headers['stripe-signature']
   55        const event = stripe.webhooks.constructEvent(
   56          req.rawBody, sig, config.webhookSecret,
   57        )
   58        switch (event.type) {
   59          case 'payment_intent.succeeded':            <- other match: succeeded
   60            return markPaid(event.data.object)
   61          case 'charge.refunded':
   62            return markRefunded(event.data.object)
   63        }
   64      }
   65
⊕  66  v   async function markPaid(intent: Stripe.PaymentIntent) {
```

(`•` = on-page dot, `⊕` = add, `v` = expanded fold, `>` = folded.)

## 6. Footer hint

Height **36**; flex centered; gap **6**; padding 0 16; bg `#FFFFFF`; border-top 1px `#E2E8F0` (`--color-rule`).

`[⊕ 14px icon, stroke #656974 1.75]` "adds a function to" (Inter 12/16 `#656974`) "Placing an order" (Inter 12/16 **500** `#202126`,
the current page's name) "·" (muted) `[esc]` (outlined key chip, components §7.6) "closes · pin keeps it open" (muted).

## 7. States shown and not shown

Shown: sheet open and unpinned; `⋯` menu open with the first item hovered; current and other text matches with scrollbar ticks; one
definition already on the page; four addable definitions; one folded definition; call links.

Not shown: pinned sheet (canvas narrowed), hover on a line or on `⊕`, hover on a call link, the sheet in dark, a non-TypeScript file,
a very long file / horizontal scroll, the sheet opened from Add -> Files with no match (presumably scrolled to top, no match
highlight).
