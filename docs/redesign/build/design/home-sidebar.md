# Home-mode sidebar

The sidebar as it appears while the **Home** tab is active. Source artboards: v2 · 01 (Home — project),
v2 · 05 (First launch), v2 · 06 (first Compile), v2 · 08 (Project settings, behind the scrim), v2 · 11 (Health and
activity), and the row cells in v2 · 07 (Project states). The board-mode sidebar is documented in `components.md`.

All values come from Paper's `get_jsx` / `get_computed_styles`. Sample names (storefront-api, Checkout, ...) are
illustrative.

## Notation

- Type is written `family size/line-height weight tracking color`, e.g. `Inter 13/16 500 -0.01em #202126`.
  Weight 400 and tracking 0 are omitted.
- Every text node in the file sets `-webkit-font-smoothing: antialiased`, `-moz-osx-font-smoothing: grayscale`,
  `font-synthesis: none`.
- Icons are inline SVG. Path data is verbatim. Unless noted: `fill="none"`, `stroke-width 1.5`,
  `stroke-linecap round`, `stroke-linejoin round`.
- Token names are the Paper file's (`--color-*`). The code token (in `ui/src/theme/tokens.css`) is given where it
  differs in name. "No token" means no Paper token has that value.

## Color map (used in this file)

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Sidebar background |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Sidebar right border, footer top rule |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text, workspace monogram fill, selected-row chevron |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Icons, counts, section label, branch suffix |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Selected project row |
| `#FFFFFF` | `--color-ground` | `--color-surface` | Workspace monogram letter |
| `#E6E8EC` | **no token** | — | Project monogram fill |
| `#3F4451` | **no token** | — | Project monogram letter |
| `#D97706` | `--color-decision` | `--color-decision` (code also has `--color-pending #c26a0a` for this dot) | Problems dot |
| `#E2E8F0` | `--color-rule` | — | Progress-ring track |
| `#94A3B8` | `--color-faint` | — | Resting moon glyph |
| `#DC2626` | `--color-failure` | `--color-failure` is `#c42020` in code | Failed triangle |

Note the name clash: Paper's `--color-muted` is `#475569`, but the code's `--color-muted` is `#656974`. The
designs use `#656974` everywhere for muted text; nothing in these artboards uses `#475569`.

## Container

| Property | Value |
| --- | --- |
| Width | 264px (`flex-shrink: 0`; §7 says resizable 220–360, no handle is drawn) |
| Height | Fills the body (952px in a 990px artboard: window height − 38px tab strip) |
| Background | `var(--color-reader-light-canvas)` `#F7F8FA` |
| Border | right, 1px solid `#E0E2E7` |
| Layout | flex column, no gap |
| Padding | 12px top/bottom, 6px left/right → rows are 251px wide (264 − 12 − 1 border) |
| Base text | 12px / 16px |

## Vertical structure (artboard v2 · 01, y relative to the sidebar top)

| # | Block | y | Size | Notes |
| --- | --- | --- | --- | --- |
| 1 | Workspace row | 12 | 251×36 | |
| 2 | Nav (Recents, Drafts) | 48 | 251×76 | 4px top padding, 16px bottom padding (the "16px between sections") |
| 3 | Section header "Projects" | 124 | 251×28 | |
| 4 | Project row (selected, expanded) | 152 | 251×28 | |
| 5 | Tree under the expanded project | 180 | 251×146 | 2px top + 5×28 + 4px bottom |
| 6 | Project row | 326 | 251×28 | |
| 7 | Project row | 354 | 251×28 | |
| 8 | Spacer | 382 | flex: 1 | `flex-grow: 1; flex-basis: 0` |
| 9 | Footer | 899.5 | 251×40 | Pinned to the bottom |

## 1. Workspace row

- Row: height 36, flex, `align-items: center`, gap 8px, padding-left 6px, `flex-shrink: 0`.
- **Workspace monogram**: 20×20, radius 5px, background `#202126`, centered letter `L`
  (`Inter 11/14 600 #FFFFFF`). This is the only dark monogram; project monograms are gray.
- **Name** "Lazar's workspace": `Inter 13/16 600 #202126`, `flex: 1`.
- **Search button**: 28×28, radius 6px, centered 16px icon (no background at rest; hover not drawn).

  ```svg
  <svg width="16" height="16" viewBox="0 0 16 16">
    <circle cx="7" cy="7" r="4.5" stroke="#656974" stroke-width="1.5" fill="none"/>
    <path d="M10.5 10.5L13.5 13.5" stroke="#656974" stroke-width="1.5" stroke-linecap="round" fill="none"/>
  </svg>
  ```

## 2. Nav rows: Recents, Drafts

Container: flex column, padding-top 4px, padding-bottom 16px.

Each row: height 28, radius 6px, flex, `align-items: center`, gap 8px, padding-inline 6px.

| Slot | Width | Content |
| --- | --- | --- |
| Icon | 20px (`justify-content: center`) | 16×16 icon, stroke `#656974` 1.5 |
| Label | `flex: 1` | `Inter 13/16 400 #202126` |
| Count | 24px, right-aligned | `Inter 12/16 #656974` (Drafts only; "4") |

Icons (16×16, viewBox 0 0 16 16):

- **Recents** (clock): `<circle cx="8" cy="8" r="5.5"/>` + `<path d="M8 5V8L10 9.5"/>` (round cap).
- **Drafts** (dashed page): `<rect x="3" y="2.5" width="10" height="11" rx="1.5" stroke-dasharray="2 2"/>`.

Copy: "Recents", "Drafts", count "4". On First launch (v2 · 05) the Drafts row has no count slot at all (the
label simply fills).

## 3. Section header "Projects"

- Row: height 28, flex, `align-items: center`, padding-inline 6px, no gap.
- Label "Projects": `Inter 11/14 600 0.06em uppercase #656974`, `flex: 1` → renders "PROJECTS".
- Add button: slot 16px wide, 16×16 plus icon `<path d="M8 3.5V12.5M3.5 8H12.5"/>` stroke `#656974` 1.5 round
  cap. (Adds a project, same as the footer's Add project.)

## 4. Project row

Height 28, radius 6px, flex, `align-items: center`, **gap 6px**, padding-inline 6px, `flex-shrink: 0`.

| Slot | Width | Content |
| --- | --- | --- |
| Disclosure | 12px, centered | 10×10 chevron (see below) |
| Monogram | 20×20 | radius 5px, bg `#E6E8EC`, letter `Inter 11/14 600 #3F4451` centered |
| Name block | `flex: 1; min-width: 0` | Name, optionally followed by the branch |
| State slot | 16×16 (`flex-shrink: 0`, centered) | One state indicator or empty |

- **Name**: `Inter 13/16 500 -0.01em #202126`.
- **Branch suffix** (only on `storefront-web`, which is on a feature branch): the name block becomes
  `display: flex; align-items: baseline; gap: 5px`; name gets `flex-shrink: 0; width: max-content`; branch
  "feat/cart-v2" is `JetBrains Mono 11/14 -0.03em #656974`, `text-overflow: clip`. Rows on `main` show no branch.
- **Chevrons** (10×10, viewBox 0 0 10 10, stroke 1.5, round caps and joins):
  - collapsed (right): `M3.5 2L6.5 5L3.5 8`
  - expanded (down): `M2 3.5L5 6.5L8 3.5`
  - Color `#656974`; on the **selected** row the chevron is `#202126`.

### Row states drawn

| State | Background | Name | Chevron | State slot |
| --- | --- | --- | --- | --- |
| Rest | none | `#202126` 500 | `#656974` | per project state |
| Selected (the project Home shows) | `var(--color-reader-light-selection)` `#EDF0FC`, radius 6 | unchanged `#202126` 500 | `#202126` | per project state |
| Hover | **not drawn** | — | — | — |
| Focus | **not drawn** | — | — | — |

Selected rows in the artboards: storefront-api (01, 08), payments-service (06, collapsed — it has no boards),
billing-worker (11). Expansion is independent of selection: in 11 storefront-api is collapsed and the selected
billing-worker is expanded; in 06 nothing is expanded.

### State slot indicators (16×16 slot; full catalogue in `07-project-states.md`)

| State | Glyph | Exact SVG / element |
| --- | --- | --- |
| Current | nothing | empty 16×16 slot |
| Compiling / updating | progress ring, 14×14 | `<svg width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="5.25" fill="none" stroke="#E2E8F0" stroke-width="1.75"/><path d="M7 1.75A5.25 5.25 0 0 1 12.25 7" fill="none" stroke="#656974" stroke-width="1.75" stroke-linecap="round"/></svg>` (a static quarter arc; animation not specified — rotate it) |
| Problems | amber dot | `div` 8×8, radius 999px, bg `#D97706` |
| Paused | pause glyph, 12×12 | `<svg width="12" height="12" viewBox="0 0 12 12"><rect x="2.75" y="2" width="2.25" height="8" rx="0.75" fill="#656974"/><rect x="7" y="2" width="2.25" height="8" rx="0.75" fill="#656974"/></svg>` |
| Not compiled | hollow circle, 12×12 | `<svg width="12" height="12" viewBox="0 0 12 12"><circle cx="6" cy="6" r="4.25" fill="none" stroke="#656974" stroke-width="1.5"/></svg>` |
| Compile failed | red triangle, 14×14 | `<svg width="14" height="14" viewBox="0 0 14 14"><path d="M6.13 2.0C6.52 1.33 7.48 1.33 7.87 2.0L12.6 10.5C12.98 11.17 12.5 12 11.73 12H2.27C1.5 12 1.02 11.17 1.4 10.5L6.13 2.0Z" fill="#DC2626"/><path d="M7 5V7.75" stroke="#FFFFFF" stroke-width="1.4" stroke-linecap="round"/><circle cx="7" cy="9.75" r="0.8" fill="#FFFFFF"/></svg>` |
| Resting | moon, 14×14 | `<svg width="14" height="14" viewBox="0 0 14 14"><path d="M11.5 8.6A4.75 4.75 0 1 1 5.4 2.5A3.75 3.75 0 0 0 11.5 8.6Z" fill="none" stroke="#94A3B8" stroke-width="1.5" stroke-linejoin="round"/></svg>` |

The problems dot has a tooltip with the count (07's copy: "The count is in the dot's tooltip."). The tooltip is
**not drawn**; use the app's existing tooltip (`--color-tooltip`).

## 5. Tree under an expanded project (folders and boards)

Container: flex column, padding-top 2px, padding-bottom 4px. Every row: height 28, radius 6px, flex,
`align-items: center`, gap 6px, padding-right 6px.

Indentation by depth (padding-left):

| Depth | Example | padding-left |
| --- | --- | --- |
| 1 — directly under the project | folder "Checkout", folder "Background jobs", draft "Auth, end to end", billing-worker's "Monthly statements" | 24px |
| 2 — inside a folder | "Order lifecycle", "Payment webhooks" inside Checkout | 42px (+18 per level) |

Slots, left to right:

| Slot | Width | Content |
| --- | --- | --- |
| Disclosure | 12px | Folder: 10×10 chevron (same paths as project rows, `#656974`). Board: empty 12px spacer |
| Icon | 16px, centered | Folder / board / draft icon, 16×16, stroke `#656974` 1.5 |
| Label | `flex: 1; min-width: 0` | `Inter 13/16 400 #202126` |
| Trailing | 24px, right-aligned | Board: page count, `Inter 12/16 #656974` ("3", "2", "1"). Folder: empty 24px spacer |

Icons (viewBox 0 0 16 16):

- **Folder**: `<path d="M2 4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12.5C13.33 4.5 14 5.17 14 6V11.5C14 12.33 13.33 13 12.5 13H3.5C2.67 13 2 12.33 2 11.5V4.5Z" stroke-linejoin="round"/>`
- **Board** (published): `<path d="M5.5 2.5H11.5C12.6 2.5 13.5 3.4 13.5 4.5V10.5" stroke-linecap="round"/>` +
  `<rect x="2.5" y="4.75" width="8.75" height="8.75" rx="1.5"/>`
- **Draft board**: the same two shapes, dashed: path `stroke-dasharray="2 2"`, rect `stroke-dasharray="2 1.75"`.

Order as drawn: folders and boards interleaved as Checkout (expanded) → its two boards → Background jobs
(collapsed) → Auth, end to end (draft, at project level). The sort rule is not stated (folders first? last
edited?). Board rows on Home have no selected state (a board row is selected only in board mode).

Copy (v2 · 01): Checkout · Order lifecycle 3 · Payment webhooks 2 · Background jobs · Auth, end to end 1.
Copy (v2 · 11): Monthly statements 4 · Dunning retries 2.

## 6. Footer: Add project and the gear

- Row: height 40, flex, `align-items: center`, gap 8px, padding-left 6px, **padding-top 8px**, border-top 1px
  solid `#E0E2E7`, `flex-shrink: 0`. (The row's right edge has no padding; the gear sits flush at the sidebar's
  6px inner padding.)
- Plus icon in a 20px slot: `<path d="M8 3.5V12.5M3.5 8H12.5"/>`, stroke `#656974` 1.5.
- "Add project": `Inter 13/16 400 #202126`, `flex: 1`.
- **Gear** (Preferences, ⌘,): 28×28 button, radius 6px, 16×16 icon:

  ```svg
  <svg width="16" height="16" viewBox="0 0 16 16">
    <circle cx="8" cy="8" r="2" fill="none" stroke="#656974" stroke-width="1.5"/>
    <path d="M8 1.8V3.6M8 12.4V14.2M14.2 8H12.4M3.6 8H1.8M12.4 3.6L11.1 4.9M4.9 11.1L3.6 12.4M12.4 12.4L11.1 11.1M4.9 4.9L3.6 3.6" fill="none" stroke="#656974" stroke-width="1.5" stroke-linecap="round"/>
  </svg>
  ```

## 7. Empty workspace (v2 · 05)

The Projects header is followed by one hint row instead of project rows:

- Row: height 28, flex, `align-items: center`, padding-inline 6px.
- "No projects yet": `Inter 12/16 #656974`.

Recents and Drafts are still shown (Drafts without a count). The footer is unchanged.

## Not drawn (implementer decides)

- Hover, pressed, and keyboard-focus states of every sidebar row and icon button. Suggest the code's
  `--color-hover` (`rgb(32 33 38 / 0.06)`) for hover and `--color-focus-ring` for focus.
- Expanded **Recents** and **Drafts** (what they list and where: in place or in Home's main area).
- The project row's `⋯` menu trigger (§4.2 says the project menu is "`⋯` on the project row"); no `⋯` appears on
  any project row. Likely a hover-revealed button that replaces the state slot; not specified.
- Resize handle (264px default, 220–360px).
- Drag and drop of boards between folders, rename in place.
- Overflow: long project names have `min-width: 0` but no `text-overflow: ellipsis` set; the branch suffix uses
  `text-overflow: clip`.
