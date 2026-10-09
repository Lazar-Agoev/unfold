# Publish (artboard v2 · 12)

Build spec for the Publish dialog, the board menu, and the "unpublished changes" state, taken from the Paper file
*Lazar's Playground*, page **Unfold App**, artboard `v2 · 12 · Publish` (1440×900). All values come from `get_jsx` and
`get_computed_styles`. Sample data (`Order lifecycle`, `storefront-api`, `Placing an order`) is illustrative; chrome
copy is verbatim.

The artboard is a **composite**: it draws the board menu (with its Share submenu) open in the sidebar **and** the
Publish dialog open over the canvas at the same time. In the app they never appear together.

## 0. Colors and tokens used

This artboard binds most colors to Paper tokens directly (`var(--color-reader-light-…)`).

| Hex | Paper token | Code token (`ui/src/theme/tokens.css`) | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` | `--color-surface` / `--color-popover` | Dialog, menus, top bar, Cancel, Share, radio fill |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, icons |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Dialog, menu, list, button borders; top-bar bottom border |
| `#E2E8F0` | `--color-rule` | **no code token** | Menu separators |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Publish buttons, selected radio, publish-state dot |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Hovered / open menu row |
| `#FFF3D6` | `--color-reader-light-warn-fill` | `--color-warn-fill` | Conflict box |
| `#8A5A00` | `--color-reader-light-warn` | `--color-warn` | Conflict text |
| `#D97706` | `--color-decision` | `--color-decision` | Conflict warning icon |
| `#DC2626` | `--color-failure` | **no exact code token** (code `--color-danger` / `--color-failure` are `#c42020`) | `Delete…` |
| `#B6BCC8` | **no token** | **no token** | Unselected radio border |
| `#E6E8EC` | **no token** | **no token** | Board-menu button, open state; monogram fill |
| `#3F4451` | **no token** | **no token** | Monogram letter |
| `#ECEDF0` | **no token** | **no token** | Lens track |
| `#0F172A2E` | `--color-ink` at 18% | **no code token** (code `--color-overlay` differs) | Scrim |
| `#0F172A0F`, `#0F172A1A` | `--color-ink` at 6%, 10% | **no code token** | Floating shadow |
| `#0F172A14` | `--color-ink` at 8% | **no code token** | Lens active segment shadow |

**Shadow · floating:** `box-shadow: 0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` (same as Add).

## 1. Publish state in the canvas top bar

The top bar (canvas area, 1176px wide) is unchanged from artboard 02; the parts this flow needs:

| Element | Spec |
| --- | --- |
| Top bar | Height 48px, fill `#FFFFFF`, bottom border 1px `#E0E2E7`, padding-left 20px, padding-right 12px, gap 8px, align center, `position: relative` |
| Page title | `Placing an order` Inter 13px / 20px weight 600 `#202126` |
| Publish state | Flex row, padding-left 4px, gap 6px, align center: dot 6×6, radius 999px, fill `#5862C0`; text Inter 12px / 16px `#656974` **`Published · 2 unpublished changes`** |
| Spacer | `flex: 1` |
| Share | Height 30px, padding-left 12px, padding-right 8px, gap 4px, radius 6px, fill `#FFFFFF`, border 1px `#E0E2E7`; `Share` Inter 13px / 16px weight 500 `#202126`; chevron 14×14 `viewBox 0 0 24 24`, `<path d="M6 9l6 6 6-6"/>`, stroke `#656974`, width 1.75, round |
| Publish | Height 30px, padding-inline 12px, radius 6px, fill `#5862C0`; `Publish` Inter 13px / 16px weight 500 `#FFFFFF` |
| Lens | Absolute, `left: 50%; top: 9px; translate: -50%`; height 30px, padding 2px, radius 8px, fill `#ECEDF0`. Active segment: height 26px, padding-inline 14px, radius 6px, fill `#FFFFFF`, shadow `0 1px 2px #0F172A14`, `Code` Inter 13px / 16px weight 500 `#202126`. Inactive: same without fill, `Flow` in `#656974` |

The state text pattern is `Published · <n> unpublished changes`. Only this state is drawn on artboard 12; `Draft` and
plain `Published` appear in the doc (§4.4) but not here.

## 2. The board menu

### 2.1 Trigger (sidebar board header)

The board header sits at the top of the sidebar scroll area (sidebar 264px, content inset 8px each side, header
247px wide).

- **Breadcrumb row**: height 28px, padding-left 6px, padding-right 8px, gap 6px, align center:
  - Back button 20×20, radius 6px; chevron 16×16 `viewBox 0 0 24 24`, `<path d="M15 18l-6-6 6-6"/>`, stroke `#656974`,
    width 1.5, round.
  - Monogram 14×14, radius 4px, fill `#E6E8EC`; letter Inter 9px / 14px weight 600 `#3F4451`.
  - `storefront-api / Checkout` (`<project> / <folder>`) Inter 12px / 16px `#656974`.
- **Title row**: height 32px, padding-left 8px, gap 8px, align center:
  - Board name `Order lifecycle` Inter 15px / 20px weight 600, letter-spacing `-0.01em`, `#202126`, `flex: 1`.
  - **Board menu button** 28×28, radius 6px, centered icon. **Resting: no fill. Open: fill `#E6E8EC`.**
    Icon 16×16, `viewBox="0 0 24 24"`: `<circle cx="5" cy="12" r="1.6"/>`, `<circle cx="12" cy="12" r="1.6"/>`,
    `<circle cx="19" cy="12" r="1.6"/>`, fill `#656974`.

In window coordinates the button is at x 227–255, y 76–104.

### 2.2 Menu

| Property | Value |
| --- | --- |
| Position | `left 36px; top 108px` in the window: **4px below the button**, right edge (256) aligned with the button's right edge (255, 1px off; treat as right-aligned) |
| Width | 220px (height fits content) |
| Padding | 4px |
| Radius | 10px (`--radius-md`; code `--radius-large`) |
| Border | 1px solid `#E0E2E7` |
| Fill | `#FFFFFF` |
| Shadow | Shadow · floating |

**Item:** height 28px, padding-inline 8px, gap 8px, radius 6px, align center. Label Inter 13px / 20px weight 400
`#202126`, `flex: 1`. Trailing slot 16×16 always present (empty for plain items) so labels and chevrons align.

**Submenu chevron** (in the trailing slot): 14×14 `viewBox 0 0 24 24`, `<path d="m9 18 6-6-6-6"/>`, stroke
`#656974`, width 1.75, round. On the hovered/open row the chevron stroke is `#202126`.

**Separator:** height 9px, padding-inline 4px, centered 1px line fill `#E2E8F0` (`--color-rule`).

**Hovered / submenu-open row:** fill `#EDF0FC`.

**Destructive item:** label color `#DC2626`; no other change.

Items, verbatim and in order (ellipses are the single character `…`):

| # | Item | Trailing | Notes |
| --- | --- | --- | --- |
| 1 | `Rename…` | (empty) | |
| 2 | `Move to folder` | `›` chevron | Submenu (not drawn) |
| 3 | `Duplicate` | (empty) | |
| 4 | `Open in new window` | (empty) | |
| — | separator | | |
| 5 | `Share` | `›` chevron | Drawn hovered/open: fill `#EDF0FC`, chevron `#202126` |
| 6 | `Fold all cards` | (empty) | |
| 7 | `Unfold all cards` | (empty) | |
| — | separator | | |
| 8 | `Discard changes…` | (empty) | Only when the board has unpublished changes (inferred) |
| 9 | `Move back to drafts` | (empty) | Only for a published board (inferred) |
| — | separator | | |
| 10 | `Delete…` | (empty) | Destructive: `#DC2626` |

**No shortcuts are drawn in the board menu.**

### 2.3 Share submenu

| Property | Value |
| --- | --- |
| Position | `left 258px; top 229px`: 2px right of the menu's right edge; top is 5px above the Share row so the first item lines up with Share (1px border + 4px padding) |
| Width | 196px |
| Padding / radius / border / fill / shadow | As the menu: 4px, 10px, 1px `#E0E2E7`, `#FFFFFF`, Shadow · floating |
| Items | Same item style, no trailing slot |

Items verbatim: `Export Mermaid…`, `Copy Mermaid`.

## 3. Publish dialog

The drawn variant is the **conflict** variant: a published board with 2 unpublished changes, whose published file
changed underneath (a pull). Only this variant is drawn.

### 3.1 Placement and scrim

| Property | Value |
| --- | --- |
| Scrim | Covers the **canvas area only**: `left 264; top 38; 1176×862`, fill `#0F172A2E`. The sidebar and tab strip are not dimmed |
| Dialog position | `left 592; top 255` in the window: centered horizontally and vertically on the canvas area (264 + 1176/2 = 852 = 592 + 520/2; 38 + 862/2 = 469 = 255 + 428.5/2) |
| Width | 520px; height fits content (428.5px in this variant) |
| Radius | 12px |
| Border | 1px solid `#E0E2E7` |
| Fill | `#FFFFFF` |
| Shadow | Shadow · floating |
| Layout | Flex column of four sections |

### 3.2 Header

Padding-inline 24px, padding-top 20px; flex column, gap 6px.

- Title Inter 15px / 22px weight 600 `#202126`: **`Publish changes to Order lifecycle`** (`Publish changes to <board>`).
- Destination line: flex row, `flex-wrap: wrap`, `align-items: baseline`, column-gap 4px, row-gap 2px:
  - `Writes` Inter 13px / 20px `#656974`
  - `.unfold/boards/order-lifecycle.json` JetBrains Mono 12px / 20px `#202126`
  - `in storefront-api. Commit it like code.` Inter 13px / 20px `#656974`

  (Full sentence: "Writes `.unfold/boards/order-lifecycle.json` in storefront-api. Commit it like code." It wraps to two
  lines at 520px.)

### 3.3 Changes list

Padding-inline 24px, padding-top 20px; flex column, gap 6px.

- Label: `2 changes` (`<n> changes`) Inter 11px / **14px** weight 600, letter-spacing `0.06em`, uppercase, `#656974`.
- List: border 1px `#E0E2E7`, radius 8px, flex column.
- Row: height 36px, padding-inline 12px, gap 10px, align center; every row but the last has a 1px `#E0E2E7` bottom
  border.
  - Icon slot 16×16; icon 16×16 `viewBox 0 0 24 24`, stroke `#656974`, width 1.5, round, no fill.
  - Description Inter 13px / 20px `#202126`, `flex: 1`.
  - Meta Inter 12px / 16px `#656974`, `flex-shrink: 0`.

| Change | Icon paths | Description | Meta |
| --- | --- | --- | --- |
| Edit | `M12 20h9` · `M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z` | `Description of createOrder edited` | `Placing an order` (the page) |
| Add | `M12 5v14` · `M5 12h14` | `Page “Refunds” added` (curly quotes) | `3 cards` |

### 3.4 Conflict box

Wrapper padding-inline 24px, padding-top 16px. Box: fill `#FFF3D6`, radius 8px, padding 12px block / 14px inline,
flex column, gap 12px.

- **Message row**: flex, `align-items: start`, gap 10px.
  - Icon slot 16×20 (centered); warning icon 16×16 `viewBox 0 0 24 24`, stroke `#D97706`, width 1.75, round, no fill:
    - `<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>`
    - `<path d="M12 9v4"/>`
    - `<path d="M12 17h.01"/>`
  - Text Inter 13px / 20px `#8A5A00`, `flex: 1`:
    **`The published file changed since you started (pulled 10 min ago). Publishing replaces it.`**
- **Choices** (radio group): padding-left 26px (aligns with the text), flex column, gap 4px. Each option: height
  28px, gap 8px, align center.
  - Radio 16×16, radius 999px, border 1.5px, fill `#FFFFFF`.
    - Selected: border `#5862C0`, centered inner dot 8×8 radius 999px fill `#5862C0`.
    - Unselected: border `#B6BCC8`, no dot.
  - Label Inter 13px / 20px `#202126`; the selected label is weight 500, the other 400.
  - Options verbatim, in order: **`Keep mine`** (selected by default), **`Take theirs and drop my changes`**.

### 3.5 Footer

Padding 20px block, 24px inline; flex row, gap 8px, `justify-content: end`.

| Button | Spec |
| --- | --- |
| `Cancel` | Height 30px, padding-inline 12px, radius 6px, fill `#FFFFFF`, border 1px `#E0E2E7`; Inter 13px / 16px weight 500 `#202126` |
| `Publish` | Height 30px, padding-inline **14px**, radius 6px, fill `#5862C0`; Inter 13px / 16px weight 500 `#FFFFFF` |

The Publish label doesn't change when "Take theirs and drop my changes" is chosen (as drawn).

## 4. Discard changes

Only the board-menu item **`Discard changes…`** is drawn (§2.2, item 8). The ellipsis means it asks first. Its
confirmation dialog is not drawn. Per the doc (§4.7, R-6), it returns the board to the published version.

## Open questions for the implementer

1. **Only the conflict variant is drawn.** The plain variant (no conflict) is presumably sections 3.2, 3.3 and 3.5
   only. The **first publish of a draft** (no changes list; title wording?) is not drawn. Suggest the title
   `Publish Order lifecycle` and no changes list.
2. When **Take theirs and drop my changes** is chosen, should the primary button relabel (for example "Discard my
   changes") and use the danger style? Not drawn.
3. **Discard changes…** confirmation is not drawn. Suggest a 400px dialog in this dialog's style: title
   `Discard changes to Order lifecycle?`, body naming the count, `Cancel` and a danger `Discard changes` button.
4. **Scrim scope differs from Add:** Publish dims only the canvas area and centers on it; Add dims the whole window
   and centers on the window. Pick one rule.
5. **Board menu shows no shortcuts**, though the doc gives Publish ⇧⌘E and the commands list shows it. Rename is
   double-click in the header (artboard 00). Decide whether menu items show shortcuts.
6. **Menu wording differs from the doc and from Add's commands**: menu `Move back to drafts` vs Add's command `Move
   board back to drafts`; menu `Open in new window` vs doc "Open in New Window" (tab menu) and artboard 00 "Move to
   New Window"; menu `Fold all cards` / `Unfold all cards` vs doc "Fold All" / "Unfold All". The doc's §4.7 list (Move
   back to drafts, Rename, Move to folder, Duplicate, Delete) lacks `Open in new window`, `Share`, `Fold all cards`,
   `Unfold all cards`, and `Discard changes…`, which the menu adds.
7. **Conditional items:** whether `Discard changes…` hides or disables when there are no unpublished changes, and
   whether `Move back to drafts` hides for a draft (which would need `Publish…` instead), is not drawn.
8. **Destructive red** is `#DC2626` (Paper `--color-failure`), not the code's `--color-danger` `#c42020`. Pick one.
9. The `Move to folder` submenu is not drawn.
10. The publish-state dot is accent indigo for "unpublished changes". The doc (§7) reserves amber for "needs review";
    confirm indigo is intended here, and what the dot looks like for `Draft` and `Published`.
