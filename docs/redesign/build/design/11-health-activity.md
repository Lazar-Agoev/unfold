# v2 · 11 · Health and activity

Home for **billing-worker** after clicking **Health and activity** (the link at the right of the Health line). The
overview sections (How it starts, Boards, Health strip) are hidden and replaced, below the unchanged header, by the
**Health and activity** view: a Health | Activity switch, the full problem list with actions, board health, a quiet
note, and an **Activity** preview (the index log) as a table.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG; path data is verbatim. Problem and note icons here are Lucide-style (24×24 viewBox); board and branch
icons are the custom 16×16 set.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` | `--color-surface` | Main, active segment, outline buttons |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Log table header, sidebar |
| `#F3F4F6` | `--color-reader-light-sidebar` | `--color-sidebar` | Segmented control track |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, icons, locations |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Buttons, active segment, log table border |
| `#E2E8F0` | `--color-rule` | — | List rules, log row rules |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Copy diagnostics, Show all, New board |
| `#D97706` | `--color-decision` | `--color-pending` (`#c26a0a` in code) | Problem icons, "2 problems" dot, sidebar dot |
| `#94A3B8` | `--color-faint` | — | Quiet-note info icon |
| `#00000000` | transparent | — | Inactive segment border (keeps size stable) |

## Artboard and regions

Artboard 1440×990, background `#FFFFFF`, flex column, `overflow: clip`. Tab strip as in `01-home.md`.

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Window tabs | 0 | 0 | 1440 | 38 | Home active + two board tabs |
| Sidebar | 0 | 38 | 264 | 952 | billing-worker selected and expanded |
| Main | 264 | 38 | 1176 | 952 | flex column, padding 36 / 56 / 40, gap 44 (same as 01) |
| Project header | 320 | 74 | 1064 | 102 | |
| Health and activity | 320 | 220 | 1064 | 300 | replaces How it starts (same y) |
| Activity preview | 320 | 564 | 1064 | 154 | |

How it starts, Boards, and the Health strip are present in the layer tree with `display: none`: **this is a mode of
Home, not a separate page**. "Back to overview" restores them.

## Sidebar on this artboard

storefront-api collapsed (its tree `display: none`), storefront-web compiling (`feat/cart-v2`, ring),
**billing-worker selected and expanded** (`#EDF0FC`, chevron down in `#202126`, amber dot), with two boards directly
under the project at depth 1 (padding-left 24px, no folder): "Monthly statements" 4, "Dunning retries" 2. Drafts
"4". Spec: `home-sidebar.md`.

## Project header (billing-worker)

Same component as `01-home.md` → Project header. Differences:

- Path "~/code/billing-worker"; name "billing-worker".
- Branch switcher shows only "main" + chevron; the "· 2 worktrees" text is `display: none` (one worktree), so the
  button is 83px wide.
- Graph state: dot 8×8 `#D97706` + "2 problems" `Inter 13/16 #202126`.
- Detail: "214 files · Python · updated 5 min ago" `Inter 13/16 #656974`.
- Actions unchanged: `⋯`, Settings, New board.

## Health and activity section

Flex column, gap 20px.

### Section header

Flex, `align-items: center`, `justify-content: space-between`, height 30.

- Left (flex, `align-items: center`, gap 16px):
  - "Health and activity": `Inter 15/22 600 #202126`.
  - **Segmented control**: track background `#F3F4F6`, radius 8px, padding 2px, gap 2px.
    - Active segment "Health": height 26, padding-inline 10px, gap 6px, background `#FFFFFF`, border 1px solid
      `#E0E2E7`, radius 6px. Label `Inter 13/16 500 #202126`; count "2" `Inter 12/16 500 #656974`.
    - Inactive segment "Activity": height 26, padding-inline 10px, border 1px solid transparent, radius 6px; label
      `Inter 13/16 500 #656974`.
- Right: **Back to overview** (flex, gap 4px): chevron-left 14×14 (viewBox 0 0 24 24)
  `<path d="m15 18-6-6 6-6" stroke="#656974" stroke-width="1.5"/>` + "Back to overview" `Inter 13/16 #656974`.

### Health list

Border-top 1px solid `#E2E8F0`; each row has border-bottom 1px solid `#E2E8F0`.

Row: flex, `align-items: center`, gap 12px, padding 12px 4px (63px tall with two text lines).

| Slot | Width | Content |
| --- | --- | --- |
| Icon | 20×20, centered | Warning circle 16×16 (viewBox 0 0 24 24), stroke `#D97706` 1.75, round: `<circle cx="12" cy="12" r="9"/><path d="M12 8v4.5M12 16h.01"/>` |
| Text | `flex: 1 1 0; min-width: 0`, column gap 2px | Title `Inter 13/20 500 #202126`; explanation `Inter 12/16 #656974` |
| Location | 240px | `JetBrains Mono 12/16 #656974` |
| Action | 96px, content right-aligned | Outline button: height 28, padding-inline 12px, background `#FFFFFF`, border 1px solid `#E0E2E7`, radius 6px, `Inter 13/16 500 #202126` |

Rows (verbatim):

| Title | Explanation | Location | Action |
| --- | --- | --- | --- |
| Doesn't parse — line 88: expected ':' | Showing its last version that parsed. | billing/tasks.py:88 | Open |
| [tool.unfold] python = "3.11" isn't supported | Unfold reads Python 3.12 to 3.14. | pyproject.toml | Settings |

("Open" presumably opens the file in the peek sheet at that line; "Settings" opens Project settings → Python
projects. Neither target is drawn.)

### Boards health

Column, gap 4px.

- Label (wrapper padding-left 4px): "Boards" (renders "BOARDS") `Inter 11/14 600 0.06em uppercase #656974`.
- Row: flex, `align-items: center`, gap 12px, padding 10px 4px, border-bottom 1px solid `#E2E8F0` (49px tall).

  | Slot | Width | Content |
  | --- | --- | --- |
  | Icon | 20×20, centered | Board icon 16×16 (viewBox 0 0 16 16), stroke `#656974` 1.5: `<path d="M5.5 2.5H11.5C12.6 2.5 13.5 3.4 13.5 4.5V10.5" stroke-linecap="round"/><rect x="2.5" y="4.75" width="8.75" height="8.75" rx="1.5"/>` |
  | Text | `flex: 1`, **row**, `align-items: center`, gap 8px | "1 card needs review" `Inter 13/20 #202126` + "Its code changed since it was described." `Inter 12/16 #656974` |
  | Board | 240px | "Monthly statements" `Inter 12/16 #656974` (Inter, not mono) |
  | Action | 96px, right | Outline button "Review" (same as above) |

  The board review item is neutral here (no amber), while the Health strip on v2 · 01 colors it amber
  (`#8A5A00` text + `#D97706` dot).

### Quiet note

Flex, `align-items: center`, gap 8px, padding-left 4px.

- Info icon 14×14 (viewBox 0 0 24 24), stroke `#94A3B8` 1.75, round:
  `<circle cx="12" cy="12" r="9"/><path d="M12 16v-4.5M12 8h.01"/>`.
- "Calls Unfold can't follow aren't problems; they're shown on the cards." `Inter 12/16 #656974`.

## Activity preview (index log)

Column, gap 10px.

### Header row

Flex, `align-items: center`, `justify-content: space-between` (16 high).

- Left (flex, `align-items: baseline`, gap 10px): "Activity" (renders "ACTIVITY")
  `Inter 11/14 600 0.06em uppercase #656974` + "Every update to the graph, newest first" `Inter 12/16 #656974`.
- Right (flex, gap 16px): "Copy diagnostics" and "Show all", both `Inter 13/16 400 #5862C0` (text links; note
  weight 400 here vs 500 for links elsewhere).

### Log table

Box: border 1px solid `#E0E2E7`, radius 8px, `overflow: clip`, flex column.

- **Head row**: height 30, flex, `align-items: center`, gap 16px, padding-inline 14px, background `#F7F8FA`,
  border-bottom 1px solid `#E2E8F0`. Labels `Inter 12/16 500 #656974`.
- **Body row**: height 32, same flex/gap/padding, border-bottom 1px solid `#E2E8F0` (none on the last row).

| Column | Width | Head label | Cell type |
| --- | --- | --- | --- |
| Time | 120px | Time | `Inter 13/16 #202126` |
| What | 120px | What | `Inter 13/16 #202126` |
| Files | `flex: 1 1 0; min-width: 0` | Files read / cached / failed | `JetBrains Mono 12/16 #202126` |
| Duration | 80px, right-aligned | Duration | `JetBrains Mono 12/16 #656974` |

Rows (verbatim):

| Time | What | Files read / cached / failed | Duration |
| --- | --- | --- | --- |
| 2 min ago | Update | 3 / 211 / 1 | 140 ms |
| 1 h ago | Update | 1 / 213 / 0 | 90 ms |
| Yesterday | Compile | 214 / 0 / 0 | 2.1 s |

## Not drawn / open

- **Health | Activity switch vs. both sections visible.** With "Health" selected, the view still shows the Activity
  preview below. Decide: the Health tab = problems + a 3-row activity preview, the Activity tab = the full log
  ("Show all" switches to it)? The Activity tab is not drawn.
- The "failed" files of a log row (e.g. "1" in 3 / 211 / 1): no drill-down drawn.
- Empty health ("No problems") state.
- Hover states for rows and buttons.
