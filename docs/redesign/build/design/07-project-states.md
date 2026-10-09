# v2 · 07 · Project states

A spec board, not a window. It lists every project state with (1) the sidebar project row, (2) the Home state line
under the project's name, and (3) the project menu actions offered in that state. Below the table: the two
**toasts** (engine reconnecting, project compiled) and the open **project menu**.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG; path data is verbatim.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` | `--color-surface` / `--color-popover` | Board, toasts, menu |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Sidebar-row cells, mock window corner |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text, table head rule, open-trigger dots |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, glyphs |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Row rules, toast/menu border, menu separators |
| `#E2E8F0` | `--color-rule` | — | Progress-ring track, mock panel borders |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Action links (Cancel, See Health, Resume, Open Home), eyebrow |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Highlighted menu item |
| `#16A34A` | `--color-success` | `--color-success` (`#15803d` in code) | "Graph current" dot |
| `#D97706` | `--color-decision` | `--color-pending` (`#c26a0a` in code) | Problems dot |
| `#DC2626` | `--color-failure` | `--color-failure` / `--color-danger` (`#c42020` in code) | Failed triangle, "Remove from workspace…" |
| `#94A3B8` | `--color-faint` | — | Resting moon in the sidebar row |
| `#E6E8EC` / `#3F4451` | **no token** | — | Monogram fill / letter |
| `#EEF0F3` | **no token** | — | Open (pressed) `⋯` trigger background |
| `#0F172A` @ 6% (`#0F172A0F`) and @ 10% (`#0F172A1A`) | `--color-ink` with alpha | `--shadow-popover` differs | Toast and menu shadow |

## Board layout

Artboard 1440 wide, `height: fit-content` (1675), background `#FFFFFF`, flex column, padding 72px top/bottom and 80px
left/right, gap 56px.

| Region | x | y | w | h |
| --- | --- | --- | --- | --- |
| Title | 80 | 72 | 1280 | 66 |
| States table | 80 | 194 | 1280 | 936 |
| Engine and menu | 80 | 1186 | 1280 | 417 |

### Title (board annotation, not app UI)

Flex, `align-items: flex-end`, `justify-content: space-between`, gap 48px.

- Eyebrow "Project states": `Inter 11/14 600 0.12em uppercase #5862C0`; headline "State lives on the project.":
  `Inter 36/40 800 -0.03em #202126` (column gap 12px).
- Right note (width 460): "One 16px slot on the project's sidebar row, the same state in words under the project's
  name on Home, and the actions in the project menu. There is no status bar." `Inter 13/20 #656974`.

### Table structure (annotation layout)

- Head: flex, gap 32px, padding-bottom 12px, border-bottom 1px solid `#202126`. Columns: State 224px · Sidebar row
  267px · Home state line `flex: 1` · Project menu actions 220px. Labels `Inter 11/14 600 0.06em uppercase #656974`.
- Each row: flex, `align-items: flex-start`, gap 32px, padding-block 14px, border-bottom 1px solid `#E0E2E7`.
  - State cell: padding-top 10px, column gap 2px; name `Inter 13/20 600 #202126`; description
    `Inter 12/16 #656974`.
  - Sidebar cell: background `#F7F8FA`, radius 8px, padding 6px 8px, width 267; contains a real 251×28 project row.
  - State-line cell: padding-top 12px.
  - Actions cell: padding-top 10px; `Inter 13/20 #202126`, `white-space: pre-wrap`, shortcut separated by two
    spaces.

## The states

### Sidebar row and Home state line, per state

All sidebar rows use the project row spec in `home-sidebar.md` (28 high, gap 6, padding-inline 6, 16×16 state
slot). None of the rows in this table is selected; none has a dimmed name.

**Home state line** layout (all states): flex, `align-items: center`, gap 6px.

| Part | Spec |
| --- | --- |
| Glyph slot | 12px wide, centered |
| Headline | `Inter 13/16 #202126` |
| Detail | `Inter 13/16 #656974`, starts with "· " |
| Action link | wrapper padding-left 6px; `Inter 13/16 500 var(--color-reader-light-accent)` |

| # | State | State description (verbatim) | Sidebar state slot | Home state line (verbatim) | Project menu actions (verbatim, in order) |
| --- | --- | --- | --- | --- | --- |
| 1 | Not compiled | Just added. Home shows the first-Compile setup. | Hollow circle | ○ **Not compiled yet** · branch main | Compile  ⇧⌘B / Settings… / Remove from workspace… |
| 2 | Compiling | The first Compile, or Compile again. Home fills in as entrances are found. | Progress ring | ◔ **Compiling** · 412 of 1,058 files · resolving calls  [Cancel] | Cancel / Settings… / Health and activity |
| 3 | Updating in background | Files changed on disk. The graph catches up on its own. | Progress ring (same as compiling) | ◔ **Updating** · 3 files changed | Pause background updates / Compile again  ⇧⌘B / Rebuild from scratch |
| 4 | Current | The slot stays empty. Quiet is the normal state. | Empty | ● **Graph current** · 1,284 files · updated 2 min ago | Compile again  ⇧⌘B / Rebuild from scratch / Pause background updates |
| 5 | Some files don't parse | A problem, not a failure. The count is in the dot's tooltip. | Amber dot | ● **1 file doesn't parse** · showing its last version that parsed  [See Health] | Health and activity / Compile again  ⇧⌘B / Copy diagnostics |
| 6 | Background updates paused | Turned off in the project menu. Changes wait until resumed. | Pause glyph | ❚❚ **Paused** · 12 files changed since  [Resume] | Resume background updates / Compile again  ⇧⌘B |
| 7 | Compile failed | The last compile stopped. Boards keep the previous graph. | Red triangle | ▲ **Compile failed** · the previous graph stays · [See Health] | Compile again  ⇧⌘B / Rebuild from scratch / Health and activity / Copy diagnostics |
| 8 | Resting | Not watched: no open tab and not selected. Catches up when opened. | Moon (faint) | ☾ **Resting** · last checked 2 h ago | Compile again  ⇧⌘B / Settings… |
| 9 | Using fallback location | A note, not a state. .unfold/ can't be written, so the graph is kept in app data. | Empty (current) | ● **Graph current** · using fallback location (i) | Settings… → General shows where / Reveal in Finder |

Bold = headline color `#202126`; the rest of the line is muted; [bracketed] = accent link. In row 7 the link follows
the detail directly (the detail ends with " ·" and there is no 6px wrapper), unlike rows 2, 5, 6.

Row 1's state line is all Inter ("· branch main"); v2 · 06 renders "main" in JetBrains Mono. Row 4's detail omits
"TypeScript + Python" which v2 · 01 includes ("1,284 files · TypeScript + Python · updated 2 min ago"). v2 · 11 uses
"2 problems" as the headline for a project with one parse error and one settings error, not row 5's wording.

### Glyphs: sidebar slot vs. Home state line

The two places draw the same state with slightly different sizes. Exact SVG:

| State | Sidebar slot (16×16 slot) | Home state line (12px slot) |
| --- | --- | --- |
| Not compiled | `<svg width="12" height="12" viewBox="0 0 12 12"><circle cx="6" cy="6" r="4.25" fill="none" stroke="#656974" stroke-width="1.5"/></svg>` | identical |
| Compiling / Updating | `<svg width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="5.25" fill="none" stroke="#E2E8F0" stroke-width="1.75"/><path d="M7 1.75A5.25 5.25 0 0 1 12.25 7" fill="none" stroke="#656974" stroke-width="1.75" stroke-linecap="round"/></svg>` | same paths, `width="12" height="12" viewBox="0 0 14 14"`, `stroke-width="2"` on both |
| Current | empty slot | `div` 8×8, radius 999px, `#16A34A` |
| Problems | `div` 8×8, radius 999px, `#D97706` | identical |
| Paused | `<svg width="12" height="12" viewBox="0 0 12 12"><rect x="2.75" y="2" width="2.25" height="8" rx="0.75" fill="#656974"/><rect x="7" y="2" width="2.25" height="8" rx="0.75" fill="#656974"/></svg>` | identical |
| Failed | `<svg width="14" height="14" viewBox="0 0 14 14"><path d="M6.13 2.0C6.52 1.33 7.48 1.33 7.87 2.0L12.6 10.5C12.98 11.17 12.5 12 11.73 12H2.27C1.5 12 1.02 11.17 1.4 10.5L6.13 2.0Z" fill="#DC2626"/><path d="M7 5V7.75" stroke="#FFFFFF" stroke-width="1.4" stroke-linecap="round"/><circle cx="7" cy="9.75" r="0.8" fill="#FFFFFF"/></svg>` | same paths at `width="12" height="12"` (viewBox 0 0 14 14) |
| Resting | `<svg width="14" height="14" viewBox="0 0 14 14"><path d="M11.5 8.6A4.75 4.75 0 1 1 5.4 2.5A3.75 3.75 0 0 0 11.5 8.6Z" fill="none" stroke="#94A3B8" stroke-width="1.5" stroke-linejoin="round"/></svg>` | `width="12" height="12" viewBox="0 0 14 14"`, stroke **`#656974`**, `stroke-width="1.6"` |
| Fallback note | — | trailing info icon in a 16×16 slot: `<svg width="14" height="14" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6" fill="none" stroke="#656974" stroke-width="1.5"/><path d="M8 7.25V11" stroke="#656974" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="5" r="0.9" fill="#656974"/></svg>` (hover shows the tooltip; §6 also puts "watching by scanning" there) |

The progress ring is drawn as a static quarter arc; animate it as a spinner (rotate) or as determinate progress
(412 of 1,058) — not specified.

### Resting: design vs. doc

§4.10 says a resting project "shows its last known state with a dimmed name". The design instead shows a **moon
glyph** in the slot and an **undimmed** name (`#202126`, 500). Decide which; the design is the only drawn version.

### Problems tooltip

Not drawn. Copy implied: the count (e.g. "1 file doesn't parse"). Use the app tooltip (`--color-tooltip`).

## Toasts ("Engine and windows")

Section header: flex, `align-items: baseline`, gap 12px. "Engine and windows" `Inter 15/22 600 #202126`; "Toasts,
bottom right of the window. They never block the canvas." `Inter 13/20 #656974`.

The mock window corner (annotation): background `#F7F8FA`, border 1px solid `#E2E8F0`, radius 10px, padding 24px,
flex column, `align-items: flex-end; justify-content: flex-end`, gap 10px, min-height 300 (728×379 as drawn). So:
**toasts stack in the bottom-right corner, 24px from the edges, 10px apart, newest at the bottom.**

**Toast**

| Property | Value |
| --- | --- |
| Width | 380px |
| Background | `#FFFFFF` |
| Border | 1px solid `#E0E2E7` |
| Radius | 12px |
| Shadow | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` (ink at 6% and 10%) |
| Padding | 12px 14px |
| Layout | flex, `align-items: flex-start`, gap 10px |
| Height as drawn | 64px |

Contents:

| Toast | Leading slot | Title (`Inter 13/20 500 #202126`) | Body (`Inter 12/16 #656974`) | Action |
| --- | --- | --- | --- | --- |
| Engine reconnecting | 16×20 slot, centered 14×14 progress ring (sidebar version, stroke 1.75) | Reconnecting to the engine… | Your boards stay open. Edits are saved once it's back. | none |
| Project compiled | 20×20 slot with project monogram 20×20 (radius 5px, `#E6E8EC`, letter `Inter 11/14 600 #3F4451` "W") | storefront-web compiled | 6 entrances found | "Open Home" `Inter 13/16 500 var(--color-reader-light-accent)`, height 20, vertically centered, `flex-shrink: 0` |

Title/body column: `flex: 1`, gap 2px. No close button and no auto-dismiss timing are drawn.

## Project menu

Section header: "Project menu" `Inter 15/22 600 #202126`; "⋯ on the project row and on its Home."
`Inter 13/20 #656974` (baseline, gap 12px).

Mock field (annotation): background `#FFFFFF`, border 1px solid `#E2E8F0`, radius 10px, padding 20px 24px 24px,
flex column, `align-items: flex-end`, gap 6px. So: **the menu opens below the trigger, right-aligned to it, 6px
gap.**

**Trigger, open (pressed) state**: 28×28, radius 6px, background `#EEF0F3`; the three dots turn `#202126`
(at rest they are `#656974` with no background — see `01-home.md`).

**Menu popover**

| Property | Value |
| --- | --- |
| Width | 264px |
| Background | `#FFFFFF` |
| Border | 1px solid `#E0E2E7` |
| Radius | 12px |
| Shadow | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` |
| Padding | 6px |
| Height as drawn | 299px |

- **Item**: height 28, radius 6px, padding-inline 8px, flex, `align-items: center`. Label `Inter 13/16 #202126`
  (`flex-grow: 1`). Shortcut `Inter 12/16 #656974`, right.
- **Highlighted item** (keyboard or hover): background `var(--color-reader-light-selection)` `#EDF0FC`. Label color
  unchanged.
- **Separator**: wrapper padding 5px 8px, 1px line `#E0E2E7` (11px tall in total).
- **Destructive item**: label `#DC2626`.

Items as drawn (state: Current), verbatim:

1. Compile again — ⇧⌘B (highlighted)
2. Rebuild from scratch
3. Pause background updates
4. — separator —
5. Settings…
6. Add entrance…
7. Health and activity
8. — separator —
9. Reveal in Finder
10. Copy diagnostics
11. — separator —
12. Remove from workspace… (red)

The table above lists only the most relevant actions per state; this full menu is the Current state. Implementer
must decide the full menu per state: the table's lists are partial (e.g. Compiling shows Cancel, Settings…, Health
and activity; the full menu would presumably also keep Reveal in Finder, Copy diagnostics, Remove from workspace…).
Suggested rule: the first group changes with the state (Compile / Compile again / Cancel / Pause / Resume /
Rebuild), the other three groups are always present.

## Not drawn / open

- Selected and dimmed (resting) variants of the project row.
- Problems-dot tooltip; fallback-location tooltip content.
- Toast dismissal (close button, timeout), and toasts for other events (compile failed, etc.).
- Exact per-state menus (see above).
