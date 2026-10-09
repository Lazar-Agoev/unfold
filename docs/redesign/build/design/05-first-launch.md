# v2 · 05 · First launch

The window on first launch: an empty workspace. Only the Home tab exists; the sidebar has no projects; the main area
is a centered welcome with **Add project…** (⌘O), a drop zone, and a list of folders opened before.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG, `fill="none"`, round caps and joins unless stated; path data is verbatim.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#FFFFFF` | `--color-ground` | `--color-surface` | Main background, drop zone fill, Home tab |
| `#202126` | `--color-reader-light-text` | `--color-text` | Headline, paths, drop-zone title |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Eyebrow, body copy, captions, icons |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Add project… button, "Add" links |
| `#E2E8F0` | `--color-rule` | — | Row rules in "Opened before" |
| `#D9DCF5` | **no token** | — | Shortcut text "⌘O" on the accent button |
| `#C9CDD6` | **no token** | — | Drop zone dashed border |
| `#F0F1F4`, `#E0E2E7`, `#F7F8FA` | see `01-home.md` | | Tab strip, sidebar |

## Artboard and regions

Artboard 1440×990, background `#FFFFFF`, flex column, `overflow: clip`.

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Window tabs | 0 | 0 | 1440 | 38 | Only the Home tab (active). Same spec as `01-home.md` → Tab strip |
| Sidebar | 0 | 38 | 264 | 952 | Empty workspace variant, see `home-sidebar.md` §7 |
| Main | 264 | 38 | 1176 | 952 | flex column, **`justify-content: center`**, padding 36 top / 96 left-right / 80 bottom, gap 44, `overflow: clip` |
| Empty state block | 360 | 248 | 600 | 488 | Fixed width 600, flex column, left-aligned in Main; vertically centered by Main (80px bottom padding pushes it slightly above center) |
| · Intro | 360 | 248 | 600 | 114 | |
| · Actions | 360 | 362 | 600 | 58 | 28px top padding + 30px button |
| · Drop zone wrap | 360 | 420 | 600 | 172 | 24px top padding + 148px zone |
| · Opened before | 360 | 592 | 600 | 144 | 32px top padding + 28 label + 3×28 rows |

## Sidebar (empty workspace)

Workspace row, Recents, Drafts (no count), "PROJECTS" header with `+`, then the hint row "No projects yet"
(`Inter 12/16 #656974`, 28 high, padding-inline 6px). Footer: Add project and the gear. See `home-sidebar.md`.

## Intro

Flex column, gap 10px.

| Element | Copy (verbatim) | Type |
| --- | --- | --- |
| Eyebrow | Unfold (renders "UNFOLD") | `Inter 11/14 600 0.06em uppercase #656974` |
| Headline | Read a codebase by how it starts. | `Inter 32/40 700 -0.03em #202126` |
| Body | Connect a repository. Unfold reads TypeScript and Python, finds the ways the code starts, and lets you follow each call on a board. It never runs your code. | `Inter 13/20 #656974`, width 520px (wraps to two lines) |

## Actions: Add project…

Row: flex, `align-items: center`, gap 12px, padding-top 28px. One button:

- Height 30, padding-left 12px, padding-right 10px, gap 10px, radius 6px, background
  `var(--color-reader-light-accent)` `#5862C0`.
- Label "Add project…" (with the ellipsis character `…`): `Inter 13/16 500 #FFFFFF`.
- Shortcut "⌘O": `Inter 12/16 500 #D9DCF5`.
- Opens the system folder picker.

## Drop zone

Wrapper: flex column, padding-top 24px. Zone:

- Height 148, full width (600), background `#FFFFFF`, border **1.5px dashed `#C9CDD6`**, radius 10px.
- Flex column, `align-items: center; justify-content: center`, gap 10px.
- Folder icon 24×24 (viewBox 0 0 16 16), stroke `#656974`, **stroke-width 1.1**:
  `<path d="M2 4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12.5C13.33 4.5 14 5.17 14 6V11.5C14 12.33 13.33 13 12.5 13H3.5C2.67 13 2 12.33 2 11.5V4.5Z" stroke-linejoin="round"/>`
- Text block: column, centered, gap 2px.
  - "Drop a folder here": `Inter 13/20 500 #202126`.
  - "A repository with TypeScript or Python files": `Inter 12/16 #656974`.
- Drag-over state: **not drawn**. §6 says dropping a folder anywhere on the window also works. Suggest: border
  `#5862C0` solid, background `--color-reader-light-selection` `#EDF0FC` while a folder is dragged over.

## Opened before

Not in the redesign doc's §4.1 (which mentions only Add project and the drop zone); it carries V1's recent folders.

Wrapper: flex column, padding-top 32px.

- Label "Opened before" (renders "OPENED BEFORE"): height 28, `align-content: center`,
  `Inter 11/14 600 0.06em uppercase #656974`.
- Row: height 28, flex, `align-items: center`, gap 8px, **border-top 1px solid `#E2E8F0`** (no bottom border on
  the last row).

  | Slot | Width | Content |
  | --- | --- | --- |
  | Icon | 16px, centered | Folder icon 16×16, same path as above, stroke `#656974` 1.5 |
  | Path | `flex: 1` | `JetBrains Mono 12/16 #202126` |
  | When | 140px | `Inter 12/16 #656974` |
  | Action | 32px, right-aligned | "Add" `Inter 13/16 500 var(--color-reader-light-accent)` |

Rows (verbatim):

| Path | When |
| --- | --- |
| ~/code/storefront-api | last opened 3 d ago |
| ~/code/storefront-web | last opened 3 d ago |
| ~/work/billing-worker | last opened 2 wk ago |

## Not drawn / open

- Hover states for the button, drop zone, and rows.
- What happens when there are no previously opened folders (presumably the section is omitted).
- Whether the "Opened before" list appears outside first launch.
