# v2 · 01 · Home — project

A compiled project's Home: the window tab strip with Home pinned first, the Home-mode sidebar
(see `home-sidebar.md`), and the main column with the project header, **How it starts**, **Boards**, and the
**Health** line. Values come from Paper (`get_jsx`, `get_computed_styles`). Sample data (storefront-api, routes,
board names, counts) is illustrative.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG with `fill="none"`, `stroke-linecap round`, `stroke-linejoin round` unless stated; path data is verbatim.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` / `--color-reader-light-surface` | `--color-surface` | Artboard and main background, cards, outline buttons, active tab |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Sidebar, published-board thumbnail ground |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, icons, methods |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Card and button borders, tab borders |
| `#E2E8F0` | `--color-rule` | — | Health strip top rule |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | New board button, links (Trace →, Filter, Review), thumbnail highlight line |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Hovered entrance row, selected sidebar row |
| `#16A34A` | `--color-success` | `--color-success` (`#15803d` in code) | "Graph current" dot |
| `#D97706` | `--color-decision` | `--color-pending` is `#c26a0a` in code | Review dot in Health |
| `#8A5A00` | `--color-reader-light-warn` | `--color-warn` | "1 card needs review…" text |
| `#F0F1F4` | **no token** | — | Tab strip background |
| `#E1E3E8` | **no token** | — | Monogram fill in tabs (cards and sidebar use `#E6E8EC`) |
| `#E6E8EC` | **no token** | — | Monogram fill (sidebar, board card subline) |
| `#3F4451` | **no token** | — | Monogram letter |
| `#EEF0F3` | **no token** | — | Active Boards filter pill, rule above "8 more routes" |
| `#B42318` | **no token** | — | `DELETE` method (the token red is `#DC2626`) |
| `#CBD2DB` | **no token** | — | Thumbnail edges |
| `#DCE0E6` | **no token** | — | Thumbnail card strokes and text lines |
| `#C3C9D2` | **no token** | — | Draft-board thumbnail dashed border |
| `#FF5F57` `#FEBC2E` `#28C840` | **no token** (macOS) | — | Traffic lights (drawn by the OS in Electron; not ours) |

## Artboard and regions

Artboard: 1440 wide, `height: fit-content` (renders 989.5; treat as a 1440×990 window), background `#FFFFFF`,
flex column, `overflow: clip`.

| Region | x | y | w | h | Layout |
| --- | --- | --- | --- | --- | --- |
| Window tabs | 0 | 0 | 1440 | 38 | see Tab strip |
| Body | 0 | 38 | 1440 | 952 | flex row, `flex: 1; min-height: 0` |
| Sidebar | 0 | 38 | 264 | 952 | see `home-sidebar.md` |
| Main | 264 | 38 | 1176 | 952 | flex column, `flex: 1; min-width: 0; overflow: clip`, **padding 36 top / 56 left-right / 40 bottom, gap 44px** |
| Project header | 320 | 74 | 1064 | 102 | |
| How it starts | 320 | 220 | 1064 | 366 | |
| Boards | 320 | 629.5 | 1064 | 228 | |
| Health strip | 320 | 901.5 | 1064 | 48 | |

The content column is 1064px (1176 − 2×56). Main has `overflow: clip`; the design does not show scrolling, but a
real Home with more content must scroll inside Main (the sidebar stays fixed).

## Tab strip (as on Home)

Container: height 38, `display: flex; align-items: flex-end`, gap 2px, padding-left 84px (room for traffic
lights), padding-right 12px, background `#F0F1F4`, border-bottom 1px solid `#E0E2E7`, position relative,
`flex-shrink: 0`. Base text 12/16.

**Home tab (active, pinned first)**

- Height 30, padding-inline 12px, flex, `align-items: center`, gap 8px.
- Background `#FFFFFF`; border 1px solid `#E0E2E7` on left, right, top (no bottom); top-left and top-right radius
  8px.
- Icon 14×14 (viewBox 0 0 16 16): `<path d="M2.5 7L8 2.5L13.5 7V13.5H9.5V10H6.5V13.5H2.5V7Z" stroke="#202126" stroke-width="1.4" stroke-linejoin="round"/>`
- Label "Home": `Inter 12/16 600 #202126`.

**Board tabs (inactive)** — "Order lifecycle", "Cart → API trace"

- Height 30, padding-inline 12px, flex, `align-items: center`, gap 8px, no background, no border.
- Monogram 14×14, radius 4px, background `#E1E3E8`, letter `Inter 9/12 600 #3F4451` centered ("S").
- Label: `Inter 12/16 500 #656974`.
- The active board tab, close buttons, and hover are documented in `components.md` (not drawn here).

**Traffic lights**: absolute, left 16px, top 13px, flex gap 8px; three 12×12 circles `#FF5F57`, `#FEBC2E`,
`#28C840`. In Electron these are native; keep the 84px left padding.

## Project header

Flex column, gap 6px. Children: path (16 high), title row (40), meta row (34).

1. **Path** "~/code/storefront-api": `JetBrains Mono 12/16 #656974`.
2. **Title row**: flex, `align-items: center`, `justify-content: space-between`, gap 24px.
   - **Name** "storefront-api": `Inter 32/40 700 -0.03em #202126`.
   - **Actions**: flex, `align-items: center`, gap 8px (231×30):
     - **Project menu `⋯`**: 28×28, radius 6px, no fill at rest. Icon 16×16:
       `<circle cx="3.5" cy="8" r="1.25" fill="#656974"/><circle cx="8" cy="8" r="1.25" fill="#656974"/><circle cx="12.5" cy="8" r="1.25" fill="#656974"/>`.
       Open state and menu: `07-project-states.md` → Project menu.
     - **Settings** (outline button): height 30, padding-inline 12px, background `#FFFFFF`, border 1px solid
       `#E0E2E7`, radius 6px; label `Inter 13/16 500 #202126`. Opens the Project settings sheet (v2 · 08).
     - **New board** (primary button): height 30, padding-left 10px, padding-right 12px, gap 6px, background
       `var(--color-reader-light-accent)` `#5862C0`, radius 6px. Icon 14×14 (viewBox 0 0 16 16)
       `<path d="M8 3.5V12.5M3.5 8H12.5" stroke="#FFFFFF" stroke-width="1.75" stroke-linecap="round"/>`; label
       `Inter 13/16 500 #FFFFFF`.
3. **Meta row**: flex, `align-items: center`, gap 16px, padding-top 6px.
   - **Branch and worktree switcher** (164×28): height 28, padding-inline 8px, gap 6px, background `#FFFFFF`,
     border 1px solid `#E0E2E7`, radius 6px.
     - Branch icon 14×14 (viewBox 0 0 16 16), stroke `#656974` 1.5:
       `<circle cx="4.5" cy="3.5" r="1.75"/><circle cx="4.5" cy="12.5" r="1.75"/><circle cx="11.5" cy="5.5" r="1.75"/><path d="M4.5 5.25V10.75M11.5 7.25C11.5 9.5 8 9.5 5.5 11" stroke-linecap="round"/>`
     - Branch "main": `JetBrains Mono 12/16 #202126`.
     - "· 2 worktrees": `Inter 12/16 #656974` (hidden, `display: none`, when the repo has one worktree — see
       v2 · 11).
     - Chevron down 10×10: `<path d="M2 3.5L5 6.5L8 3.5" stroke="#656974" stroke-width="1.5"/>`.
     - **Popover: not drawn.** The note "Drafts and the graph are per worktree." does not appear anywhere in the
       Paper file. Build the popover from the shared menu style (`07-project-states.md` → Project menu: 264 wide,
       radius 12, padding 6, 28px rows) with worktree rows and that note as a 12px muted footer line.
   - **Graph state**: flex, gap 6px. Dot 8×8, radius 999px, `#16A34A`; "Graph current" `Inter 13/16 #202126`.
   - **Detail** "1,284 files · TypeScript + Python · updated 2 min ago": `Inter 13/16 #656974`.
   - Every other state's wording and glyph: `07-project-states.md` (Home state line column).

## How it starts

Flex column, gap 16px. Children: section header (24), applications (254), disclosures (56).

### Section header

Flex, `align-items: baseline`, gap 12px, height 24.

- "How it starts": `Inter 15/20 600 #202126`.
- "19 entrances in 3 applications. Hover one to trace it on a new board.": `Inter 13/20 #656974`, `flex: 1`.
- **Add entrance** (`align-self: center`, flex, gap 6px): plus icon 14×14 (viewBox 0 0 16 16)
  `<path d="M8 3.5V12.5M3.5 8H12.5" stroke="#656974" stroke-width="1.5"/>`; label "Add entrance"
  `Inter 13/16 500 #656974`.

### Applications row

Flex, `align-items: flex-start`, gap 16px. One card per application; card widths come from `flex-grow`:

| Card | flex-grow | Rendered | x (in section) | Height |
| --- | --- | --- | --- | --- |
| server | 1.4 | 424px | 0 | 254 (6 entrances shown + "8 more routes" footer) |
| worker | 1 | 304px | 440.5 | 135 |
| scheduler | 1 | 304px | 760.2 | 107 |

All have `flex-basis: 0; min-width: 0`. (The 1.4 weight looks hand-tuned for the biggest application; the rule
for real data is not stated. Suggest: the application with the most entrances gets 1.4, or use equal widths and
let cards wrap.)

**Application card**

- Background `#FFFFFF`, border 1px solid `#E0E2E7`, radius 8px, `overflow: clip`, flex column, padding-bottom 6px.
- **Card header**: flex, `align-items: baseline`, gap 8px, padding 12px 14px 8px 14px.
  - Application name ("server", "worker", "scheduler"): `Inter 13/16 600 #202126`.
  - Launch command ("npm run start", "npm run worker", "python -m jobs"): `JetBrains Mono 12/16 #656974`,
    `flex: 1`.
- **Entrance list**: flex column, padding-inline 6px, padding-bottom 6px.
- **Entrance row**: height 28, radius 5px, padding-inline 8px, flex, `align-items: center`, no gap.

  | Slot | Width | Type |
  | --- | --- | --- |
  | Method / kind | 64px fixed | `JetBrains Mono 11/14 #656974` (`GET`, `POST`, `QUEUE`, `CRON`). `DELETE` is `#B42318` |
  | Route / name | `flex: 1; min-width: 0` | `JetBrains Mono 12/16 #202126` |
  | Trailing | 64px, right-aligned | `Inter 12/16 #656974`: function count "24 fn" |

- **Entrance row — hover** (drawn on `POST /orders`): background `var(--color-reader-light-selection)`
  `#EDF0FC`; the trailing count is replaced by "Trace →" `Inter 12/16 500 var(--color-reader-light-accent)`. The
  method and route keep their colors. Clicking Trace opens the entrance on a new page of a new draft board.
- **Overflow footer** (only on applications with more than the shown entrances): border-top 1px solid `#EEF0F3`,
  padding 6px 6px 0; inner row height 28, padding-inline 8px, gap 6px:
  "8 more routes" `Inter 12/16 #656974` · "·" `Inter 12/16 #656974` · "Filter"
  `Inter 12/16 500 var(--color-reader-light-accent)`.

Entrance rows drawn (verbatim):

| Application | Method | Route | Trailing |
| --- | --- | --- | --- |
| server | GET | /orders | 24 fn |
| server | POST | /orders | Trace → (hover) |
| server | GET | /orders/:id | 17 fn |
| server | DELETE | /orders/:id | 12 fn |
| server | POST | /webhooks/stripe | 31 fn |
| server | GET | /health | 3 fn |
| worker | QUEUE | send-receipt | 9 fn |
| worker | QUEUE | reconcile | 22 fn |
| worker | QUEUE | refund | 14 fn |
| scheduler | CRON | nightly-close | 19 fn |
| scheduler | CRON | expire-carts | 6 fn |

**Discrepancies with the redesign doc (§4.3):**

- The doc says "the first eight, a count, and a filter". The server card shows **six** rows + "8 more routes"
  (6 + 8 + 3 + 2 = 19, which matches the header count). Pick one number; the doc says eight.
- "Routes are grouped by their first path segment" is **not drawn**: there are no group headers. The only hint is
  ordering (`/orders…` rows together, then `/webhooks`, then `/health`). Decide whether grouping is just sort order
  or visible sub-headers.
- The filter itself (what "Filter" opens) is not drawn.

### Collapsed sections

Flex column, no gap. Each row: height 28, flex, `align-items: center`, gap 6px, no horizontal padding.

- Chevron slot 12px with collapsed chevron 10×10 `<path d="M3.5 2L6.5 5L3.5 8" stroke="#656974" stroke-width="1.5"/>`.
- Label `Inter 13/16 #656974`:
  - "Not reached from a launch · 2"
  - "Launches into code that isn't indexed · 1"
- Expanded state: not drawn. Suggest it reuses the application card's entrance rows.

## Boards

Flex column, gap 16px. Children: section header (24), grid (188).

### Section header

Flex, `align-items: center`, gap 16px, height 24.

- "Boards": `Inter 15/20 600 #202126`.
- **Filters** (flex, gap 2px, `flex: 1`). Each pill: height 24, padding-inline 8px, radius 6px, label
  `Inter 12/16 500`.
  - Active ("All"): background `#EEF0F3`, label `#202126`.
  - Inactive ("Published", "Drafts"): no background, label `#656974`.
- **Sort** (flex, gap 4px): "Last edited" `Inter 12/16 500 #656974` + chevron down 10×10
  `<path d="M2 3.5L5 6.5L8 3.5" stroke="#656974" stroke-width="1.5"/>`. The sort menu is not drawn.

### Grid

Flex row, gap 16px; four columns, each `flex: 1 1 0; min-width: 0` → 254px wide. (Wrapping to more rows is not
drawn; use a 4-column grid with 16px gaps.)

**Board card** (column, gap 10px):

1. **Thumbnail**: height 140, radius 8px, border 1px, `overflow: clip`, flex centered.
   - Published: background `#F7F8FA`, border solid `#E0E2E7`.
   - Draft: background `#FFFFFF`, border **dashed** `#C3C9D2`.
2. **Meta** (column, gap 2px):
   - Title: `Inter 13/20 500 #202126`.
   - Subline: `Inter 12/16 #656974`.
   - Cross-project subline (Cart → API trace): flex, `align-items: center`, gap 6px: "2 pages · Draft ·" +
     monogram 14×14 (radius 4px, background `#E6E8EC`, letter `Inter 9/12 600 #3F4451` "W") + "with storefront-web".

Copy (verbatim):

| Title | Subline | Thumbnail border |
| --- | --- | --- |
| Order lifecycle | 3 pages · Published · edited 2 h ago | solid |
| Payment webhooks | 2 pages · Published · edited yesterday | solid |
| Auth, end to end | 1 page · Draft · edited 3 d ago | dashed |
| Cart → API trace | 2 pages · Draft · [W] with storefront-web | dashed |

Hover, selection, and a card menu are not drawn.

### How thumbnails are drawn

Each thumbnail is one inline SVG, 252×138 (`viewBox="0 0 252 138"`, i.e. the 254×140 box minus its border),
centered. It is a schematic of the board's first page drawn from card positions, not a screenshot:

| Element | Drawing |
| --- | --- |
| Card | `<rect rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>` (1px stroke, on .5 coordinates for crisp lines). Sizes seen: 43–63 wide, 27–75 tall |
| Text lines inside a card | Horizontal `<path>` strokes, `stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"`, 8px apart, starting 7.5px from the card's left edge, varied lengths |
| The entrance / focused line | One line per board in the accent: `stroke="#5862C0" stroke-width="2"` (placed in the central, tallest card) |
| Edges between cards | `<path stroke="#CBD2DB" fill="none"/>` (1px). Straight `H` segments, or a stepped curve for branches: `M166 56H176C180 56 180 40 184 40H190` |
| Cross-project card | A 9×9 monogram square inside the card: `<rect width="9" height="9" rx="2.5" fill="#E6E8EC"/>` before the first line |

Verbatim example (Order lifecycle):

```svg
<svg width="252" height="138" viewBox="0 0 252 138" xmlns="http://www.w3.org/2000/svg">
  <path d="M78 68H102M166 68H186" fill="none" stroke="#CBD2DB"/>
  <rect x="22.5" y="48.5" width="55" height="39" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
  <path d="M30 60H62M30 68H54M30 76H58" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
  <rect x="102.5" y="30.5" width="63" height="75" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
  <path d="M110 42H150M110 50H142M110 70H152M110 78H136M110 86H146" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
  <path d="M110 60H140" fill="none" stroke="#5862C0" stroke-width="2" stroke-linecap="round"/>
  <rect x="186.5" y="51.5" width="45" height="33" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
  <path d="M194 62H222M194 70H214" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
</svg>
```

Payment webhooks (branching edges):

```svg
<path d="M82 72H104M166 56H176C180 56 180 40 184 40H190M166 88H176C180 88 180 100 184 100H190" fill="none" stroke="#CBD2DB"/>
<rect x="26.5" y="54.5" width="55" height="35" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
<path d="M34 66H66M34 74H58" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
<rect x="104.5" y="38.5" width="61" height="67" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
<path d="M112 50H150M112 58H140M112 78H152M112 86H134" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
<rect x="190.5" y="26.5" width="43" height="27" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
<path d="M198 36H222M198 44H214" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
<rect x="190.5" y="86.5" width="43" height="27" rx="5" fill="#FFFFFF" stroke="#DCE0E6"/>
<path d="M198 96H224M198 104H212" fill="none" stroke="#DCE0E6" stroke-width="2" stroke-linecap="round"/>
```

(Payment webhooks has no accent line.) Implementation: fit the page's card bounding box into the 252×138 box with
~22px margins, draw each card as the rect, 2–5 gray lines per card scaled to its height, and edges as 1px gray
lines. Thumbnails stay light in both themes as drawn; the dark variant is not drawn.

## Health strip

Flex, `align-items: center`, gap 24px, height 48, padding-top 4px, border-top 1px solid `#E2E8F0`
(`--color-rule`).

| Item | Content |
| --- | --- |
| Label | "Health" `Inter 13/16 600 #202126` |
| OK item | flex gap 6px: check icon 14×14 (viewBox 0 0 16 16) `<path d="M3.5 8.5L6.5 11.5L12.5 4.5" stroke="#656974" stroke-width="1.5"/>` + "All files parse" `Inter 13/16 #202126` |
| OK item | same icon + "Every entrance reaches indexed code" |
| Problem item | flex gap 6px: 14px slot with dot 8×8 radius 999px `#D97706` + "1 card needs review in Order lifecycle" `Inter 13/16 var(--color-reader-light-warn)` `#8A5A00` + "·" `Inter 13/16 #656974` + "Review" `Inter 13/16 500 var(--color-reader-light-accent)` |
| Spacer | `flex: 1` |
| Link | flex gap 4px: "Health and activity" `Inter 13/16 500 #656974` + chevron right 10×10 `<path d="M3.5 2L6.5 5L3.5 8" stroke="#656974" stroke-width="1.5"/>`. Opens the expanded view (v2 · 11) |

## Sidebar on this artboard

storefront-api selected and expanded (Checkout expanded with two boards, Background jobs collapsed, Auth, end to end
as a draft), storefront-web compiling (branch `feat/cart-v2`, progress ring), billing-worker with problems (amber
dot). Drafts count "4". Full spec: `home-sidebar.md`.

## Not drawn / open

- Branch and worktree switcher popover and the "Drafts and the graph are per worktree." note.
- Expanded "Not reached from a launch" and "Launches into code that isn't indexed".
- Entrance filter UI; sort menu; board card hover and menu; empty Boards state (a project with no boards).
- Route grouping by first path segment (see How it starts).
- Scroll behavior of Main when content exceeds the window.
