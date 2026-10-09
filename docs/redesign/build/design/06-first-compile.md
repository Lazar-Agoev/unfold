# v2 · 06 · New project — first Compile

A project that was just added and has never been compiled. Home shows it in the **not compiled** state: what will be
read, what is skipped and why, the scope tree ("What to graph"), the promise, and **Compile** (⇧⌘B). The sidebar row
carries the hollow-circle state.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG, `fill="none"`, round caps and joins unless stated; path data is verbatim.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` | `--color-surface` | Main background, scope card, unchecked box |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Promise panel, sidebar |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, icons |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Scope card border, disabled-box border |
| `#E2E8F0` | `--color-rule` | — | Skipped table rules |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Checked box fill, Compile button |
| `#8B93A1` | `--color-reader-light-contour` | `--color-contour` | Unchecked box border |
| `#94A3B8` | `--color-faint` | — | Folder icon of an ignored row |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Selected sidebar row |
| `#F1F2F4` | **no token** | — | Disabled (ignored) check box fill |
| `#D9DCF5` | **no token** | — | "⇧⌘B" on the Compile button |

## Artboard and regions

Artboard 1440×990, background `#FFFFFF`, flex column, `overflow: clip`.

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Window tabs | 0 | 0 | 1440 | 38 | Home (active) + "Order lifecycle", "Cart → API trace" tabs; spec in `01-home.md` |
| Sidebar | 0 | 38 | 264 | 952 | see below |
| Main | 264 | 38 | 1176 | 952 | flex column, padding 36 top / 56 left-right / 40 bottom, **gap 32px**, `overflow: clip` |
| Project header | 320 | 74 | 720 | 90 | fixed width 720 |
| First Compile block | 320 | 196 | 720 | 538 | fixed width 720, flex column, **gap 28px** |
| · Summary | 320 | 196 | 720 | 22 | |
| · Skipped | 320 | 246 | 720 | 112 | |
| · Scope tree card | 320 | 386 | 720 | 194 | |
| · Promise | 320 | 608 | 720 | 68 | |
| · Compile row | 320 | 704 | 720 | 30 | |

The not-compiled Home uses a narrower 720px column, left-aligned (the compiled Home uses the full 1064px).

## Sidebar on this artboard

Rows: storefront-api (collapsed, current, not selected), storefront-web (`feat/cart-v2`, compiling ring),
billing-worker (amber dot), **payments-service** (selected: `#EDF0FC` background; collapsed chevron in `#656974`;
monogram "P"; state slot = hollow circle 12×12 `<circle cx="6" cy="6" r="4.25" stroke="#656974" stroke-width="1.5"/>`).
Drafts count "4". The new project is appended at the bottom of the list. Spec: `home-sidebar.md`.

## Project header (not compiled)

Flex column, gap 6px, width 720.

1. Path "~/code/payments-service": `JetBrains Mono 12/16 #656974`.
2. Title row: flex, `align-items: center`, `justify-content: space-between`, gap 24px.
   - Name "payments-service": `Inter 32/40 700 -0.03em #202126`.
   - Only the project menu `⋯` (28×28, radius 6px, three dots `r=1.25` at x 3.5/8/12.5, fill `#656974`). **No
     Settings button, no New board** in this state.
3. State line: flex, `align-items: center`, gap 6px, padding-top 6px.
   - Hollow circle 12×12 (viewBox 0 0 12 12): `<circle cx="6" cy="6" r="4.25" stroke="#656974" stroke-width="1.5"/>`.
   - "Not compiled yet": `Inter 13/16 #202126`.
   - "· branch": `Inter 13/16 #656974`.
   - "main": `JetBrains Mono 12/16 #656974`.
   - There is no branch/worktree switcher button here (07 shows the same line as "· branch main", all Inter).

## Summary

"Unfold will read 846 TypeScript and 212 Python files." — `Inter 15/22 600 #202126` (inside a column with gap 4px;
only one line is drawn).

## Skipped

Flex column, no gap.

- Label "Skipped" (renders "SKIPPED"): height 28, `align-content: center`,
  `Inter 11/14 600 0.06em uppercase #656974`.
- Each row: height 28, flex, `align-items: center`, gap 12px, border-top 1px solid `#E2E8F0`.

  | Slot | Width | Type |
  | --- | --- | --- |
  | Reason | 180px | `Inter 13/16 #202126` |
  | Examples | `flex: 1` | `JetBrains Mono 12/16 #656974` |
  | Count | 120px, right-aligned | `Inter 12/16 #656974` |

Rows (verbatim):

| Reason | Examples | Count |
| --- | --- | --- |
| Skipped by default | node_modules/ · dist/ | 14,206 files |
| Ignored by .gitignore | .venv/ | 2,931 files |
| Too large to read | fixtures/ledger-2023.json · 2 more | 3 files over 1 MB |

No bottom rule after the last row.

## Scope tree: "What to graph"

Card: background `#FFFFFF`, border 1px solid `#E0E2E7`, radius 10px, flex column.

**Card header**: flex, `align-items: baseline`, gap 10px, padding 14px 16px 10px 16px.

- "What to graph": `Inter 15/20 600 #202126`.
- "Top-level folders. Uncheck what you don't want on the graph.": `Inter 12/16 #656974`, `flex: 1`.
- "1,058 of 1,119 files": `Inter 12/16 #656974`, `flex-shrink: 0`.

**List**: flex column, padding 0 8px 8px 8px. Each row: height 28, radius 6px, flex, `align-items: center`, gap 8px,
padding-inline 8px.

| Slot | Width | Content |
| --- | --- | --- |
| Check box | 16×16 | see states |
| Folder icon | 16px | folder path (below), stroke `#656974` 1.5 (`#94A3B8` when ignored) |
| Folder | `flex: 1` | `JetBrains Mono 12/16 #202126` (`#656974` when ignored) |
| Count / reason | 200px, right-aligned | `Inter 12/16 #656974` |

Folder icon (viewBox 0 0 16 16): `<path d="M2 4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12.5C13.33 4.5 14 5.17 14 6V11.5C14 12.33 13.33 13 12.5 13H3.5C2.67 13 2 12.33 2 11.5V4.5Z" stroke-linejoin="round"/>`

Check box states:

| State | Box |
| --- | --- |
| Checked | 16×16, radius 4px, background `var(--color-reader-light-accent)` `#5862C0`, centered 10×10 check `<path d="M2 5.2L4.2 7.2L8 3" stroke="#FFFFFF" stroke-width="1.6"/>` (viewBox 0 0 10 10) |
| Unchecked | 16×16, radius 4px, background `#FFFFFF`, border 1px solid `#8B93A1` |
| Disabled (ignored) | 16×16, radius 4px, background `#F1F2F4`, border 1px solid `#E0E2E7`; row text muted |

Rows (verbatim):

| Box | Folder | Right column |
| --- | --- | --- |
| checked | apps/api | 512 files |
| checked | apps/web | 398 files |
| checked | packages/shared | 148 files |
| unchecked | scripts | 61 files |
| disabled | tools/legacy | ignored by .gitignore |

The tree is one level here (top-level folders; no disclosure chevrons). Settings → Scope (v2 · 08) shows the nested
version with a mixed state. Hover is not drawn.

## Promise

Panel: background `var(--color-reader-light-canvas)` `#F7F8FA`, radius 10px, padding 14px 16px, flex, gap 12px, no
border.

- Icon slot 16px wide, padding-top 2px. Shield 16×16 (viewBox 0 0 16 16):
  `<path d="M8 1.75L13 3.75V7.5C13 10.6 10.9 13.1 8 14.25C5.1 13.1 3 10.6 3 7.5V3.75L8 1.75Z" stroke="#656974" stroke-width="1.5" stroke-linejoin="round"/>`
- Text (`Inter 13/20 #202126`, `flex: 1`), verbatim:
  "Reads files only. Runs nothing and installs nothing. Creates .unfold/ in this folder for the graph and your
  boards. After this, the graph updates itself as files change."
  (§4.2's version is shorter and puts `.unfold/` in code style; the design uses plain text with the extra sentence.)

## Compile row

Flex, `align-items: center`, gap 12px.

- **Compile** button: height 30, padding-left 14px, padding-right 12px, gap 10px, radius 6px, background
  `#5862C0`. "Compile" `Inter 13/16 500 #FFFFFF`; "⇧⌘B" `Inter 12/16 500 #D9DCF5`.
- Caption "About a minute for a project this size. You can keep working.": `Inter 12/16 #656974`.

After Compile is pressed, the row's state slot becomes the progress ring and the Home state line becomes
"Compiling · 412 of 1,058 files · resolving calls  Cancel" (see `07-project-states.md`). The Home "fills in as the
compile finds entrances" (§4.2); that in-between Home is not drawn.

## Not drawn / open

- **Settings or discovery problems** (§4.2 lists them for this screen) — no problem rows are drawn. Suggest reusing
  the Health list row from v2 · 11 between Skipped and the scope tree.
- The Home while the first compile runs (partially filled How it starts).
- Disabled Compile (e.g. nothing checked).
- Check box style differs from v2 · 08 (16px box here; 14px box inside a 16px slot there; disabled colors differ).
  Pick one — see `08-project-settings.md`.
