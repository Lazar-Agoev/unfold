# Shared chrome: component spec

Source: Paper file "Lazar's Playground", page "Unfold App", artboards **v2 · 02 · Board — canvas**, **v2 · 13 · Board — dark**,
**v2 · 10 · Board across projects** and **v2 · 09 · Source peek** (plus the sidebar footer from **v2 · 01 · Home — project**, the only
artboard that shows the gear). Every value below is read from `get_jsx` / `get_computed_styles`, not from screenshots.

This file covers the **light** theme. Dark values for every color are in [dark-theme.md](dark-theme.md). Screen-specific parts
(cards, drawn links, placeholder card, peek sheet) are in the per-artboard files:
[02](02-board-canvas.md), [13](13-board-dark.md), [10](10-board-across-projects.md), [09](09-source-peek.md).

## 0. Conventions

- All boxes are `box-sizing: border-box`. Sizes include borders.
- Text: `-webkit-font-smoothing: antialiased`, `font-synthesis: none`, `overflow-wrap: anywhere` everywhere (set on every Paper
  frame).
- Fonts: **Inter** (UI) and **JetBrains Mono** (code, paths, function names, routes). In code these are `--font-ui` and
  `--font-code`.
- "T:" lines give typography as `family size/line-height weight letter-spacing color`.

### 0.1 Color tokens used by the chrome (light)

Paper token -> value -> the existing code token in `ui/src/theme/tokens.css` that holds the same value (if any).

| Paper token | Hex | Code token today | Used for |
| --- | --- | --- | --- |
| `--color-reader-light-canvas` | `#F7F8FA` | `--color-canvas` | Canvas ground **and the sidebar background** |
| `--color-reader-light-surface` / `--color-ground` | `#FFFFFF` | `--color-surface`, `--color-popover` | Top bar, active tab, cards, dock, menus, peek sheet, secondary buttons |
| `--color-reader-light-sidebar` | `#F3F4F6` | `--color-sidebar`, `--color-chrome` | Only the card's mini Code/Flow track. **Not** the sidebar any more (see 0.3) |
| `--color-reader-light-text` | `#202126` | `--color-text` | Primary text, active icons |
| `--color-reader-light-muted` | `#656974` | `--color-muted` | Secondary text, section labels, all resting icons |
| `--color-reader-light-border` | `#E0E2E7` | `--color-border` | All hairlines in the shell (sidebar edge, tab strip, top bar, card borders, dock, dividers) |
| `--color-rule` | `#E2E8F0` | — | Peek sheet header and footer hairlines only (see 0.3) |
| `--color-reader-light-accent` | `#5862C0` | `--color-accent` | Selection icon tint, primary button, links, publish dot, selected card border |
| `--color-reader-light-selection` | `#EDF0FC` | `--color-selection` | Selected row fill, menu hover fill, selected link label fill |
| `--color-reader-light-contour` | `#8B93A1` | `--color-contour` | Entrance->handler arrow, dashed "unresolved" underline, teammate link |
| `--color-reader-light-step` | `#F4F5F8` | `--color-step` | Highlighted call rows in card code |
| `--color-reader-light-authored` | `#A8346F` | `--color-authored` | Descriptions |
| `--color-reader-light-authored-tint` | `#FBEFF5` | `--color-authored-tint` | Description box in Selected |
| `--color-reader-light-warn` / `-warn-fill` | `#8A5A00` / `#FFF3D6` | `--color-warn` / `--color-warn-fill` | "Changed since it was described" banner |
| `--color-decision` | `#D97706` | (code `--color-decision`, light) | Needs-review dot (6px) on page and layer rows |
| `--color-faint` | `#94A3B8` | — | Disabled icon (Forward), trail chevron, fold chevrons, Draft dot |
| `--color-subtle` | `#64748B` | — | Line numbers in card code and the peek sheet |
| `--color-ink` | `#0F172A` | — | Base of every shadow (at 6–10 %) and the scrollbar thumb |

### 0.2 Colors that match **no** Paper token (flagged)

| Hex | Where | Suggested new token |
| --- | --- | --- |
| `#ECEDF0` | Tab strip background; window Code/Flow lens track | `--color-tabstrip` / `--color-track` |
| `#E6E8EC` | Project monogram fill | `--color-monogram` |
| `#3F4451` | Project monogram letter | `--color-monogram-ink` |
| `#EEF0F3` | Icon button in pressed/open state (peek `⋯`); fold pill in the peek | `--color-pressed` |
| `#F0F1F4` | Divider under a card's description strip (lighter than border) | `--color-divider-soft` |
| `#F1F2F5` | Placeholder card fill | `--color-placeholder` |
| `#B6BCC8` | Placeholder card dashed border | `--color-placeholder-line` |
| `#A0A8E0` | Non-current match ticks on the peek scrollbar | `--color-accent-soft` |
| `#FF5F57` `#FEBC2E` `#28C840` | macOS traffic lights (drawn only in mockups; the real ones come from the OS) | none needed |
| `#FFFFFF2E` | Key chip on the primary Add button (white at 18 %) | derived |
| `#5862C01F` | Selection ring, 3px (accent at 12 %) | derived |
| `#5862C038` / `#5862C073` | Current text match fill (accent 22 %) / its 1px ring (accent 45 %) | derived |
| `#0F172A0F` `#0F172A1A` `#0F172A14` `#0F172A24` | Ink at 6 %, 10 %, 8 %, 14 % (shadows, scrollbar thumb) | derived |
| `#DB2777` (= `--color-external`) | Syntax color of `create` in `stripe.paymentIntents.create` inside a card (an external call) | token exists, but it is not in the code's syntax set |
| `#F3C66B` `#7A4B00` `#1E3A8A` `#FCA5A5` `#991B1B` | Flow-node borders/text inside the reserveInventory card | card internals, out of chrome scope |

### 0.3 Token-level inconsistencies to resolve

1. **Sidebar color.** Light sidebar is `--color-reader-light-canvas` `#F7F8FA` (same as the canvas, as §7 of the redesign doc says),
   while the code's `--color-sidebar` / `--color-chrome` are `#F3F4F6`. Change `--color-sidebar` to `#F7F8FA` (or point it at
   `--color-canvas`). In dark the sidebar is `--color-reader-dark-sidebar` `#191B1F`, which is **not** the dark canvas; see
   [dark-theme.md](dark-theme.md#3-structural-differences).
2. **Hairline color.** §7 of the doc says hairlines are `#E2E8F0` (`--color-rule`). The board artboards use
   `--color-reader-light-border` `#E0E2E7` everywhere except the peek sheet's header/footer, which use `--color-rule`. The two
   differ by a hair; use one (`#E0E2E7` = code `--color-border` is the path of least change).
3. **Review amber.** Design uses `#D97706` (`--color-decision`) for the 6px review dot; code's `--color-pending` is `#C26A0A`.

### 0.4 Size and type tokens

| Value | Paper token | Notes |
| --- | --- | --- |
| 9px | — | Monogram letter (14px monogram) |
| 10px | `--text-2xs` | not used in this chrome |
| 11px | `--text-xs` | Section labels, key chips, card mini-lens, 20px monogram letter, file location in card header |
| 12px | `--text-sm` | Secondary text, tab labels, mono rows, relation rows |
| 13px | `--text-base` | Row labels, buttons, page title |
| 15px | `--text-md` | Board title in sidebar |
| 14 / 16 / 20px | `--leading-xs` / `--leading-sm` / `--leading-base` | Line heights. 18px (descriptions in cards) has no token |
| 0.06em | `--tracking-chip` | Section labels. `-0.01em` (board title) has no token |
| 2 / 6 / 10 / 999px | `--radius-xs` / `--radius-sm` / `--radius-md` / `--radius-full` | 4px (monogram, key chip, card-lens segment), 5px (20px monogram), 8px (tab top corners, window lens track), 3px (scroll thumb) have no token |
| 4 / 8 / 12 / 16 / 24px | `--spacing-1/2/3/4/6` | 2, 6, 10, 14, 20px also occur (no token) |
| weights | 400 / 500 / 600 / 700 | `--font-weight-regular/medium/semibold/bold` |

---

## 1. Window frame

| Region | Geometry (in a 1440×900 window) | Fill |
| --- | --- | --- |
| Window root | 1440×900, flex column, `overflow: clip` | `#F7F8FA` (canvas) |
| Tab strip | y 0–38, full width | `#ECEDF0` |
| Body | y 38–900 (862 tall), flex row, `min-height: 0` | — |
| Sidebar | x 0–264, full body height | `#F7F8FA`, 1px right border `#E0E2E7` |
| Canvas area | x 264–1440 (1176 wide), flex column, `min-width: 0` | `#F7F8FA` |
| Top bar | y 38–86 (48 tall) inside canvas area | `#FFFFFF`, 1px bottom border `#E0E2E7` |
| Stage | y 86–900 (814 tall), `position: relative; overflow: clip` | transparent (canvas shows through) |

There is no window title bar: the tab strip **is** the title bar (macOS `titleBarStyle: hiddenInset`; the strip is the drag region).

## 2. Tab strip

**Container** — height 38; `display: flex; align-items: flex-end; gap: 2px`; padding-left **84px** (clears the traffic lights),
padding-right 12px; background `#ECEDF0` (no token); border-bottom 1px `#E0E2E7`; `position: relative`.

**Traffic lights (mockup only)** — absolute `left: 16px; top: 13px`; three 12×12 circles, gap 8 (group 52×12, x 16–68):
`#FF5F57`, `#FEBC2E`, `#28C840`. The real ones come from macOS; keep 84px clear on the left.

### 2.1 Tab (shared)

| Property | Value |
| --- | --- |
| Height | 30px (bottom-aligned, so y 7–37 inside the 38px strip) |
| Padding | 0 12px |
| Gap | 8px |
| Corners | `border-top-left-radius: 8px; border-top-right-radius: 8px`; bottom corners square |
| Border | 1px top/left/right; **no bottom border** |
| Leading glyph | Home icon (Home tab) or 14px project monogram (board tabs) |
| Label | T: Inter 12/16, color and weight by state (below) |

| State | Background | Side/top border | Label |
| --- | --- | --- | --- |
| Inactive | none | 1px `#00000000` (transparent, keeps width stable) | Inter 12/16 **500** `#656974` (muted) |
| Active | `#FFFFFF` | 1px `#E0E2E7` | Inter 12/16 **600** `#202126` (text) |

- In light the active tab is exactly 30px tall and the strip's 1px bottom border still runs beneath it (the tab does **not**
  merge into the top bar). The dark artboard does merge it (31px, `margin-bottom: -1px; padding-bottom: 1px; z-index: 1`). Pick
  one; merging is the usual tab look. See [dark-theme.md](dark-theme.md#3-structural-differences).
- In artboard 10 the inactive "Order lifecycle" tab omits the transparent border (so it is 130 wide instead of 133). Treat
  02's version (transparent border on every inactive tab) as canonical.
- **Close button: not drawn on any artboard.** No hover state, no `+` new-tab button, no overflow is shown. Implementer decides
  (suggest: a 16×16 `×` icon button appearing on hover/active at the right, replacing nothing, padding-right 8).
- Measured widths in 02: Home 82 (x 84–166), "Order lifecycle" 133 (x 168–301), "Cart → API trace" 141 (x 303–444).

### 2.2 Home tab

Icon: 14×14 SVG, `viewBox="0 0 16 16"`, no fill, stroke 1.4, `stroke-linejoin: round`:

```
<path d="M2.5 7L8 2.5L13.5 7V13.5H9.5V10H6.5V13.5H2.5V7Z"/>
```

Stroke `#656974` when inactive, `#202126` when active (artboard 01). Label "Home".

### 2.3 Board tab

`[monogram 14px] [board name]`. Label is the board's name verbatim (e.g. "Order lifecycle", "Cart → API trace"). The monogram is
the **project the board lives in**, not every project it touches.

## 3. Project monogram

A neutral rounded square with the project's initial (uppercase). Two sizes:

| Size | Box | Radius | Fill | Letter | Used in |
| --- | --- | --- | --- | --- | --- |
| 14 | 14×14, flex centered, `flex-shrink: 0` | 4px | `#E6E8EC` | Inter **9px**/14px 600 `#3F4451` (line-height 12px in artboard 10's sidebar rows) | Tabs, sidebar breadcrumb, "On this page" rows (10), Selected From/To rows (10) |
| 20 | 20×20 | 5px | `#E6E8EC` | Inter **11px**/14px 600 `#3F4451` | Card header (cross-project card), placeholder card, peek sheet header |

Neither color is a Paper token (flagged in 0.2). No hue per project, by design.

## 4. Sidebar (board mode)

**Container** — width **264px** (incl. 1px right border `#E0E2E7`), full body height, flex column, background `#F7F8FA`. Default
font 12/16. Resizable 220–360 per §7 (not drawn).

It has two stacked parts:

| Part | Layout | Size in 02 |
| --- | --- | --- |
| **Scroll region** | `flex: 1 1 0; overflow: clip` (make it `auto` in code); flex column; **gap 16px** between sections; padding **8px 8px 16px** (top, inline, bottom) | 263×567 (y 38–605) |
| **Selected section** | `flex-shrink: 0`; border-top 1px `#E0E2E7`; flex column; **gap 8px**; padding **8px 8px 16px** | 263×295 (y 605–900) |

All rows inside are 247 wide (264 − 1 border − 2×8 padding). Content x: rows start at x 8; the 16px icon slot sits at x 16; row
text starts at x 40.

### 4.1 Board header

Flex column, no gap, 60 tall.

**Breadcrumb row** — height 28; flex, `align-items: center`; gap **6px**; padding-left 6, padding-right 8.

1. Back-to-Home icon button: 20×20, radius 6, centered. Icon 16×16 chevron-left, `viewBox="0 0 24 24"`, stroke `#656974`
   1.5, round caps/joins: `<path d="M15 18l-6-6 6-6"/>`.
2. Monogram 14 (section 3).
3. Text: Inter 12/16 400 `#656974`. Copy is `project / folder` ("storefront-api / Checkout"), or just the project when the board
   is not in a folder ("storefront-api", artboard 10).

**Title row** — height 32; flex, centered; gap 8; padding-left 8.

1. Board name: `flex-grow: 1`; Inter **15/20 600**, letter-spacing **-0.01em**, `#202126`. ("Order lifecycle")
2. Board menu icon button: 28×28, radius 6. Icon 16×16 `viewBox="0 0 24 24"`, three filled dots `#656974`:
   `<circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/>`.

### 4.2 Section header (section label row)

Height 28; flex, centered.

| Variant | Padding | Trailing |
| --- | --- | --- |
| With icon button (Pages, Selected) | left 8, right 4 | 24×24 icon button, radius 6, 16px icon |
| With text (On this page) | 0 8, gap 8 | "follow order": Inter 12/16 400 `#656974` |

Label: `flex-grow: 1`; Inter **11/14 600**, letter-spacing **0.06em**, `text-transform: uppercase`, `#656974`.
Copy (as authored, displayed uppercase): "Pages", "On this page", "Selected".

Icons:
- Add page `+`: `viewBox="0 0 24 24"`, stroke `#656974` 1.5, round caps: `<path d="M12 5v14M5 12h14"/>`
- Collapse (Selected): chevron-down, same stroke: `<path d="M6 9l6 6 6-6"/>`

### 4.3 List row (shared by Pages and On this page)

| Property | Value |
| --- | --- |
| Height | **28px** |
| Layout | flex, `align-items: center`, gap **8px** |
| Padding | 0 8px (base); left padding grows with depth in On this page (4.5) |
| Radius | 6px |
| Leading slot | 16×16, flex centered, `flex-shrink: 0` |
| Label | `flex-grow: 1` |
| Trailing slot | 16×16, `flex-shrink: 0`; empty, or holds the review dot / a note |
| Selected | background `#EDF0FC` (selection); leading icon tinted `#5862C0`; label weight 500 (Inter) or 600 (mono) |
| Hover | **not drawn**. Suggest `--color-hover` (`rgb(32 33 38 / .06)`) |

Review mark (trailing): 6×6 circle, radius 999, `#D97706`, centered in the 16×16 slot.

### 4.4 Pages

Rows (02): "Placing an order" (selected), "Payment succeeded", "Refunds" (needs review). Rows (10): "Checkout call" (selected),
"Webhook".

- Label: Inter **13/20** 400 `#202126`; selected 500.
- Icon: page/file, `viewBox="0 0 24 24"`, stroke 1.5 round, `#656974` (selected: `#5862C0`):

```
<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/>
<path d="M14 3v5h5"/>
```

### 4.5 On this page (layer tree)

Cards in follow order, indented by call depth. **Indent = 16px per depth level**, applied as row `padding-left`:

| Depth | padding-left | Example (02) |
| --- | --- | --- |
| 0 | 8 | `POST /orders` (entrance) |
| 1 | 24 | `createOrder` (selected) |
| 2 | 40 | "Hold stock, then charge" (group) |
| 3 | 56 | `reserveInventory` (review dot), `chargePayment` |

padding-right stays 8. A group is a row like any other; its children are one level deeper.

Row content by kind:

| Kind | Leading glyph (16 slot) | Label typography |
| --- | --- | --- |
| Entrance | log-in icon (below), stroke `#656974` | JetBrains Mono 12/16 400 `#202126` ("POST /orders") |
| Function | text glyph **"ƒ"**: JetBrains Mono 13/16 **600**, centered in 16px, `#656974`; `#5862C0` when selected | JetBrains Mono 12/16 400 `#202126`; **600** when selected |
| Group | bracket icon (below), stroke `#656974` | Inter 13/20 400 `#202126` ("Hold stock, then charge") |
| Drawn link (10) | dashed-arrow icon (below), `#656974`; `#5862C0` when selected | Inter 13/20 400 `#202126`; 500 when selected ("calls over HTTP", "queues a receipt") |
| Card from a project that isn't connected (10) | "ƒ" muted | Mono 12/16 **`#656974`** (muted, not text) + trailing text "not connected" Inter 12/16 `#656974` (`flex-shrink: 0`, replaces the 16px trailing slot) |

Entrance (log-in) icon, `viewBox="0 0 24 24"`, stroke 1.5 round:
```
<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
<path d="M10 17l5-5-5-5"/>
<path d="M15 12H3"/>
```

Group icon, `viewBox="0 0 24 24"`, stroke 1.5 round: `<path d="M13 4H9v16h4"/>`

Link icon (artboard 10), 16×16, `viewBox="0 0 16 16"`, stroke 1.5, round cap:
```
<path d="M2 8h9" stroke-dasharray="2.5 2"/>
<path d="M10 5l3 3-3 3" stroke-linejoin="round"/>
```

**Cross-project boards (artboard 10)**: every row gets a **14px monogram slot first** (monogram of the card's project; an empty
14×14 spacer on link rows), then the 16px glyph, then the label; gap 8 throughout. In 10 the rows are **not indented** (all
padding 0 8). How depth indentation combines with the monogram column is not drawn; see [10](10-board-across-projects.md).

**"Label them" line** (last row of the section) — height 28; flex centered; **gap 4**; padding 0 8.
- "2 Flow steps unlabeled ·" Inter 12/16 400 `#656974`
- "Label them" Inter 12/16 **500** `#5862C0` (a text button)

### 4.6 Selected section

Pinned to the bottom of the sidebar (`flex-shrink: 0`), above it a 1px `#E0E2E7` rule. Gap 8 between its blocks; padding 8 8 16.
Collapsible via the chevron-down button in its header; per the doc it scrolls on its own (no scroll drawn).

**For a card (02, 13, 09):**

1. **Header** — section header (4.2) "Selected" + 24×24 collapse button.
2. **Identity** — flex column, gap **2px**, padding 0 8 (38 tall).
   - Name: JetBrains Mono **13/20 600** `#202126` ("createOrder")
   - Location: JetBrains Mono 12/16 400 `#656974` ("src/orders/create.ts:14")
3. **Description** — box: background `#FBEFF5` (authored-tint), radius 6, padding **8**, flex column, gap **4** (96 tall with 3
   lines).
   - Text: Inter **13/20** 400 `#A8346F` (authored). "Checks the cart, holds the stock, then charges the card. Gives the stock back if
     the charge fails."
   - Byline: Inter 12/16 400 `#A8346F`, **opacity 0.85**. "Lazar · 2 h ago"
   - Editable in place (not drawn).
4. **Relations** — flex column of 28px rows; each row: flex, centered, gap 8, padding 0 8.
   - Label column: **width 88px**, `flex-shrink: 0`, Inter 12/16 400 `#656974`.
   - Value: `flex-grow: 1`, JetBrains Mono 12/16 400 `#202126`.
   - Optional action at the end: "+ Open" Inter 12/16 **500** `#5862C0`, `flex-shrink: 0`.
   - Rows: "Called by" `ordersRouter` · "Reached from" `POST /orders` · "Not opened" `validateCart` "+ Open".

**For a drawn link (10):**

1. Header as above.
2. **Identity** — padding 0 8; one flex row, centered, gap **6**: "Link" Inter 13/20 **600** `#202126` + "· drawn by Lazar · 5 min ago"
   Inter 12/16 `#656974`. (20 tall.)
3. **Rows** — container padding 0 8; rows 28 tall, flex centered, gap 8, no own padding. Label column **width 72px** (not 88 as for
   cards; pick one) Inter 12/16 `#656974`.
   - "Label" -> "calls over HTTP" Inter **13/20** `#202126`
   - "From" -> monogram 14 "W" + `checkout` Mono 12/16 `#202126`
   - "To" -> monogram 14 "S" + `POST /orders` Mono 12/16 `#202126`
4. **Note** — padding 0 8; Inter 12/16 `#656974`: "Drawn by hand. Unfold doesn’t trace calls between projects; health checks that
   both ends still exist." (typographic apostrophe)
5. **Actions** — flex, gap 8, padding 4 8 0 8 (padding-top 4): two secondary buttons (7.2) "Edit label", "Remove link".

The Flow-node variant of Selected (label, code name it replaces, paths) is **not drawn** on these artboards.

### 4.7 Sidebar footer and the gear

**Not present in board mode on any of the four artboards** (the Selected section runs to the bottom edge). The gear appears only
in Home-mode sidebar (artboard 01). If board mode should also carry it, reuse 01's footer:

- Footer row: height 40; flex centered; gap 8; padding-left 6; **padding-top 8**; border-top 1px `#E0E2E7`.
- Leading: 20px-wide slot, centered, 16×16 plus icon `viewBox="0 0 16 16"`, stroke `#656974` 1.5 round:
  `<path d="M8 3.5V12.5M3.5 8H12.5"/>`
- "Add project": Inter 13/16 400 `#202126`, `flex: 1`.
- Gear icon button 28×28, radius 6; icon 16×16 `viewBox="0 0 16 16"`, stroke `#656974` 1.5:
  ```
  <circle cx="8" cy="8" r="2"/>
  <path d="M8 1.8V3.6M8 12.4V14.2M14.2 8H12.4M3.6 8H1.8M12.4 3.6L11.1 4.9M4.9 11.1L3.6 12.4M12.4 12.4L11.1 11.1M4.9 4.9L3.6 3.6" stroke-linecap="round"/>
  ```
- Home-mode sidebar container differs from board mode: padding **12px 6px** (block, inline) instead of 8/8/16.

Decide: does board mode show the footer (gear + Add project) below Selected? The artboards say no; §4.11 of the doc says the gear
is "at the bottom of the sidebar".

## 5. Canvas top bar

Height **48**; flex, `align-items: center`; gap **8**; padding-left **20**, padding-right **12**; background `#FFFFFF`; border-bottom 1px
`#E0E2E7`; `position: relative` (anchor for the centered lens).

Left to right:

1. **Page title** — Inter **13/20 600** `#202126`. ("Placing an order", "Checkout call") At x 284 in the window.
2. **Publish state** — flex centered, gap **6**, padding-left **4** (so 12px from the title):
   - Dot 6×6, radius 999:
     - `#5862C0` accent — "Published · 2 unpublished changes" (02, 09, 13)
     - `#94A3B8` faint — "Draft · uses storefront-web" (10)
     - plain "Published" and plain "Draft" are not drawn; suggest accent-less: faint for Draft, `#656974` or accent for Published.
   - Text: Inter 12/16 400 `#656974`. Copy patterns: "Draft", "Published", "Published · N unpublished changes", and for cross-project
     boards "Draft · uses storefront-web" (appends "uses <other projects>").
3. **Spacer** `flex-grow: 1`.
4. **Share** — secondary button with chevron (7.2): label "Share", chevron-down 14×14 `viewBox="0 0 24 24"`, stroke `#656974`
   **1.75**, round: `<path d="M6 9l6 6 6-6"/>`. 77×30. Its menu (Export Mermaid, Copy Mermaid) is not drawn.
5. **Publish** — primary button (7.1), 70×30.
6. **Code | Flow lens** — `position: absolute; left: 50%; top: 9px; translate: -50%` (centered on the **top bar**, i.e. on the
   canvas area, not the window: window x 790–914). See 7.5.

Hidden in artboard 09 while the peek sheet is open (lens `display: none`; Share/Publish are covered by the sheet). See
[09](09-source-peek.md).

## 6. Floating dock

**Container** — `position: absolute; left: 50%; bottom: 24px; translate: -50%` inside the Stage (centered on the canvas area);
flex, `align-items: center`; **gap 4**; **padding 4**; radius **10**; background `#FFFFFF`; border 1px `#E0E2E7`;
**shadow `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A`** (ink 6 % and 10 %). Height 40 (30 tallest child + 2×4 + 2×1). Width is
content-sized: 612 in 02 (window x 546–1158, y 836–876), 590 in 10.

Children, in order (dividers between groups):

| # | Item | Spec |
| --- | --- | --- |
| 1 | **Add** (primary) | h30; padding-left **10**, padding-right **6**; gap 6; radius 6; bg `#5862C0`. Plus icon 14×14 `viewBox="0 0 24 24"`, stroke `#FFFFFF` **2**, round: `<path d="M12 5v14M5 12h14"/>`. Label "Add" Inter 13/16 500 `#FFFFFF`. Key chip "A" (7.6, filled). 86 wide. |
| — | Divider | 1×16, bg `#E0E2E7`, `flex-shrink: 0` |
| 2 | **Back / Forward** | two 28×28 icon buttons (radius 6), no gap. Back chevron-left `<path d="M15 18l-6-6 6-6"/>` stroke `#656974`; Forward chevron-right `<path d="M9 18l6-6-6-6"/>` stroke **`#94A3B8`** (disabled). Both `viewBox 0 0 24 24`, 16px, stroke 1.5 round. |
| — | Divider | |
| 3 | **Trail** | h28; padding 0 8; gap 4; flex centered. Earlier step: JetBrains Mono 12/16 400 `#656974` ("POST /orders"). Separator chevron-right 12×12 stroke `#94A3B8` **2**: `<path d="M9 18l6-6-6-6"/>`. Current step: JetBrains Mono 12/16 **600** `#202126` ("createOrder"). Only two steps are ever drawn; overflow/truncation not specified. |
| — | Divider | |
| 4 | **Arrange** (ghost) | h28; padding 0 8; gap 6; radius 6. Icon 16 (below) stroke `#656974`. Label "Arrange" Inter 13/16 500 `#202126`. |
| — | Divider | |
| 5 | **Zoom** | flex, no gap: 28×28 minus button (`<path d="M5 12h14"/>`), percentage text box **44px wide**, centered, Inter 12/16 500 `#202126`, `font-feature-settings: "tnum"` ("100%"), 28×28 plus button (`<path d="M12 5v14M5 12h14"/>`), then **Fit**: h28, padding 0 8, radius 6, Inter 13/16 500 `#202126`. |

Arrange icon, `viewBox="0 0 24 24"`, stroke 1.5 round caps/joins:
```
<rect x="3" y="9" width="6" height="6" rx="1.5"/>
<rect x="15" y="3" width="6" height="6" rx="1.5"/>
<rect x="15" y="15" width="6" height="6" rx="1.5"/>
<path d="M9 12h3M12 6v12M12 6h3M12 18h3"/>
```

Hover/pressed states of dock buttons are not drawn.

## 7. Buttons and controls

### 7.1 Primary button

h **30**; padding 0 **12**; radius **6**; bg `#5862C0`; no border; text Inter **13/16 500** `#FFFFFF`. Examples: "Publish". The dock's
Add is a primary variant (padding 10/6, gap 6, leading icon, trailing key chip).

### 7.2 Secondary button

h **30**; padding 0 12 (Share: left 12, right 8, gap 4 with trailing 14px chevron); radius **6**; bg `#FFFFFF`; border 1px `#E0E2E7`;
text Inter **13/16 500** `#202126`. Examples: "Share", "Edit label", "Remove link", "Connect project".

### 7.3 Ghost button

h **28**; padding 0 **8**; radius 6; no bg/border; optional 16px leading icon with gap 6; text Inter 13/16 500 `#202126`.
Examples: "Arrange", "Fit".

### 7.4 Icon button

| Size | Radius | Icon | Where |
| --- | --- | --- | --- |
| 28×28 | 6 | 16px | Board menu `⋯`, dock Back/Forward/−/+, peek Pin/More/Close |
| 24×24 | 6 | 16px | Section header `+` and collapse |
| 20×20 | 6 | 16px | Breadcrumb back |

Rest: transparent, icon stroke `#656974` (1.5). Disabled: icon `#94A3B8`. **Pressed / menu open** (peek `⋯`, artboard 09): bg
`#EEF0F3` (no token), icon stroke `#202126`. Hover: not drawn.

Text button (links): Inter 12/16 500 `#5862C0`, no underline ("Label them", "+ Open").

### 7.5 Segmented lens (window Code | Flow)

| Part | Spec |
| --- | --- |
| Track | h 30; padding **2**; radius **8**; bg `#ECEDF0`; flex centered; no gap. 124 wide. |
| Segment | h 26; padding 0 **14**; radius **6**; flex centered |
| Active segment | bg `#FFFFFF`; shadow `0 1px 2px #0F172A14` (ink 8 %); text Inter 13/16 500 `#202126` |
| Inactive segment | no bg; text Inter 13/16 500 `#656974` |

Copy: "Code", "Flow". (Code active: 62 wide; Flow: 58.)

### 7.6 Key chips

| Variant | Spec | Where |
| --- | --- | --- |
| Filled (on accent) | h 18, min-width 18, padding 0 4, radius 4, bg `#FFFFFF2E` (white 18 %), Inter **11/14 500** `#FFFFFF`, centered | "A" in Add |
| Outlined | h 18, padding 0 5, radius 4, border 1px `#E0E2E7`, no bg, Inter 11/14 500 `#656974` | "esc" in the peek footer |
| Inline shortcut text | Inter 12/16 400 `#656974`, no box | "⌘⇧O" in menus |

## 8. Menus / popovers

Only one menu is drawn: the peek sheet's `⋯` menu (artboard 09).

| Part | Spec |
| --- | --- |
| Container | width **208**; padding **4**; radius **10**; bg `#FFFFFF`; border 1px `#E0E2E7`; shadow `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` (same as the dock); flex column, no gap |
| Placement | below its button: `top: 42px; right: 36px` inside the sheet (button bottom at 38; 4px offset) |
| Item | h **28**; padding 0 **8**; gap **8**; radius 6; flex centered |
| Item icon | 16×16 slot, icon stroke `#656974` 1.5 round |
| Item label | Inter **13/20** 400 `#202126`, `flex-grow: 1` |
| Item shortcut | Inter 12/16 400 `#656974` |
| Hover/active item | bg `#EDF0FC` (selection) |

Items: "Open in editor" ⌘⇧O (external-link icon), "Reveal in Finder" (folder icon). Icons:

```
external-link (24 box): <path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
folder (24 box):        <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
```

No separators, submenus, destructive items, or disabled items are drawn. Board menu, Share menu, card menu, and context menus are
not on these artboards (12 · Publish shows the board menu).

## 9. Toasts and banners

**Not drawn** on these four artboards (toasts are on artboard 07). The only banner-like element here is the card's review banner
(see [02 §4.4](02-board-canvas.md#44-review-banner)).

## 10. Shadows (floating things only)

| Element | Light | Dark |
| --- | --- | --- |
| Dock, menu, peek sheet | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` | `0 1px 2px #00000066, 0 12px 32px #0000008C` (dock; menus/sheet not drawn in dark) |
| Active lens segment | `0 1px 2px #0F172A14` | `0 1px 2px #00000066` |
| Selected card / selected link label (ring, not elevation) | `0 0 0 3px #5862C01F` | `0 0 0 3px #A0AAFF29` |

Cards, the top bar, the sidebar and tabs have **no shadow**. (Today's cards have `0 1px 2px rgb(20 24 40 / .06)`; the new design drops it.)

## 11. Hairlines and rhythm summary

| Thing | Value |
| --- | --- |
| Shell hairlines | 1px `#E0E2E7` (sidebar right, tab strip bottom, active tab sides/top, top bar bottom, Selected top, dock border, dock dividers, card borders, card header divider) |
| Peek header/footer rules | 1px `#E2E8F0` (`--color-rule`) |
| Card description divider | 1px `#F0F1F4` |
| Row height | 28 (sidebar rows, section headers, relation rows, menu items, dock items) |
| Code row height | 20 (cards and peek) |
| Section gap in sidebar | 16 |
| Sidebar side padding | 8 (rows), 16 (text x) |
| Top bar | 48 |
| Tab strip | 38; tabs 30 |
| Radii | 6 rows/buttons, 8 tabs/lens track, 10 cards/dock/menus, 4–5 monograms and chips |
