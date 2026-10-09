# v2 · 08 · Project settings

The project settings dialog open over the project's Home, on its **Scope** section: a left nav (General, Scope,
Detectors, Python projects), a check-box tree of the project's folders with ignored folders and their reasons, the
rules that will be written, and a footer with Cancel and Save. Behind it, the v2 · 01 Home (unchanged) under a
scrim.

Notation: type is `family size/line-height weight tracking color`; weight 400 and tracking 0 are omitted. Icons are
inline SVG; path data is verbatim. **This artboard's icons are Lucide-style (24×24 viewBox)**, unlike the custom
16×16 icons on the other Home artboards.

## Color map

| Hex | Paper token | Code token today | Used for |
| --- | --- | --- | --- |
| `#0F172A` @ 18% (`#0F172A2E`) | `--color-ink` with alpha | `--color-overlay` is `rgb(20 21 26 / 0.28)` in code | Scrim |
| `#0F172A` @ 6% / 10% | `--color-ink` with alpha | `--shadow-popover` differs | Dialog shadow |
| `#FFFFFF` | `--color-ground` | `--color-surface` | Dialog, unchecked box, Cancel button |
| `#F7F8FA` | `--color-reader-light-canvas` | `--color-canvas` | Tree header row |
| `#202126` | `--color-reader-light-text` | `--color-text` | Primary text |
| `#656974` | `--color-reader-light-muted` | `--color-muted` | Secondary text, icons |
| `#E0E2E7` | `--color-reader-light-border` | `--color-border` | Dialog border, tree border, badge, Cancel |
| `#E2E8F0` | `--color-rule` | — | Header/footer/nav rules, tree header rule, rules list lines |
| `#5862C0` | `--color-reader-light-accent` | `--color-accent` | Selected nav label, checked boxes, Save |
| `#EDF0FC` | `--color-reader-light-selection` | `--color-selection` | Selected nav item |
| `#8B93A1` | `--color-reader-light-contour` | `--color-contour` | Unchecked box border |
| `#94A3B8` | `--color-faint` | — | Folder icon of ignored rows |
| `#F1F3F5` | **no token** | — | Disabled box fill (v2 · 06 uses `#F1F2F4`) |
| `#D5D8DE` | **no token** | — | Disabled box border (v2 · 06 uses `#E0E2E7`) |

## Artboard and regions

Artboard 1440 wide, `height: fit-content` (renders 989.5; treat as 990). Window tabs, sidebar, and Main are an exact
copy of v2 · 01 (see `01-home.md`, `home-sidebar.md`). On top:

| Region | x | y | w | h | Notes |
| --- | --- | --- | --- | --- | --- |
| Scrim | 0 | 0 | 1440 | 990 | `position: absolute; inset: 0`; covers the **whole window including the tab strip and sidebar**; flex, centers its child |
| Settings dialog | 280 | 145 | 880 | 700 | Centered in the window |
| · Header | 281 | 146 | 878 | 87 | |
| · Body | 281 | 233 | 878 | 552 | |
| ·· Nav | 281 | 233 | 180 | 552 | |
| ·· Scope pane | 461 | 233 | 698 | 552 | |
| · Footer | 281 | 785 | 878 | 59 | |

**Sheet vs. dialog.** §4.11 calls it a "sheet". The drawing is a **centered modal dialog** with a scrim, not a panel
sliding in from an edge. No animation is drawn. Suggest: fade the scrim in and scale/fade the dialog from 98% over
the code's `--duration` (160ms); Escape and the close button dismiss it (with an unsaved-changes guard).

## Dialog container

| Property | Value |
| --- | --- |
| Size | 880×700, `flex-shrink: 0` |
| Background | `#FFFFFF` |
| Border | 1px solid `#E0E2E7` |
| Radius | 12px |
| Shadow | `0 1px 2px #0F172A0F, 0 8px 24px #0F172A1A` |
| Overflow | clip |
| Layout | flex column: header, body (`flex-grow: 1; min-height: 0`), footer |

Scrim: background `#0F172A2E` (`--color-ink` at 18%).

## Header

Flex, `align-items: flex-start`, `justify-content: space-between`, padding 20px 20px 18px 24px, border-bottom 1px
solid `#E2E8F0`.

- Left column, gap 6px:
  - Title "storefront-api settings": `Inter 15/22 600 #202126`.
  - Sub row (flex, `align-items: center`, gap 10px):
    - ".unfold/config.toml · shared with your team": `JetBrains Mono 12/16 #656974`.
    - **"File exists" badge**: height 20, padding-inline 7px, gap 5px, border 1px solid `#E0E2E7`, radius 5px, no
      fill. Check icon 12×12 (viewBox 0 0 24 24) `<path d="M20 6 9 17l-5-5" stroke="#656974" stroke-width="2"/>`;
      label `Inter 11/14 500 #656974`.
- **Close**: 28×28, radius 6px; icon 16×16 (viewBox 0 0 24 24)
  `<path d="M18 6 6 18M6 6l12 12" stroke="#656974" stroke-width="1.5"/>`.

## Nav

Width 180, flex column, gap 2px, padding 12px 8px, border-right 1px solid `#E2E8F0`.

Item: height 28, radius 6px, padding-inline 10px, flex, `align-items: center`.

| State | Background | Label |
| --- | --- | --- |
| Rest | none | `Inter 13/16 400 #202126` |
| Selected | `#EDF0FC` | `Inter 13/16 500 #5862C0` |

Items (verbatim): General · **Scope** (selected) · Detectors · Python projects.

## Scope pane

Flex column, gap 16px, padding 22px 28px 24px, `flex-grow: 1; min-width: 0; overflow: clip`. (Long trees must scroll
inside this pane; scrolling is not drawn.)

### Heading

Column, gap 4px.

- "Scope": `Inter 15/22 600 #202126`.
- "What gets graphed. Ignored files are skipped as your .gitignore says.": `Inter 13/20 #656974`.

### Folder tree

Box: border 1px solid `#E0E2E7`, radius 8px, `overflow: clip`, flex column.

**Header row**: height 32, flex, `align-items: center`, gap 8px, padding-left 12px, padding-right 14px,
background `#F7F8FA`, border-bottom 1px solid `#E2E8F0`.

- "Folders in storefront-api" (renders "FOLDERS IN STOREFRONT-API"): `Inter 11/14 600 0.06em uppercase #656974`,
  `flex-grow: 1`.
- "Files": same style, 84px wide, right-aligned.

**Rows container**: padding-block 4px. Each row: height 28, flex, `align-items: center`, gap 8px, padding-left 12px,
padding-right 14px. No hover drawn.

Slots, left to right (indent = one extra 16px spacer + 8px gap = **24px per level**):

| Slot | Width | Content |
| --- | --- | --- |
| Indent spacer(s) | 16px each | one per depth level below the top |
| Disclosure | 16×16 | 14×14 chevron, or an empty 16px spacer for leaf/ignored rows |
| Check box | 16×16 slot | 14×14 box (see states) |
| Folder icon | 16×16 | Lucide folder, stroke 1.5 |
| Name | `flex-grow: 1; min-width: 0` | **`Inter 13/16 #202126`** (v2 · 06 uses JetBrains Mono 12 for folder names) |
| Reason | auto | `Inter 12/16 #656974`, optional |
| Count | 84px, right-aligned | `Inter 12/16 #656974`; empty 84px spacer for ignored rows |

Icons (viewBox 0 0 24 24, stroke `#656974`, stroke-width 1.5, round caps/joins):

- Chevron down (expanded), 14×14: `m6 9 6 6 6-6`
- Chevron right (collapsed), 14×14: `m9 18 6-6-6-6`
- Folder, 16×16: `M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z`
  — stroke `#94A3B8` on ignored rows.

Check box states (14×14, radius 4px, centered in the 16px slot):

| State | Drawing |
| --- | --- |
| Checked | background `#5862C0`; 10×10 check (viewBox 0 0 24 24) `<path d="M20 6 9 17l-5-5" stroke="#FFFFFF" stroke-width="3.5"/>` |
| Mixed (some children excluded) | background `#5862C0`; centered bar 8×2, radius 1px, `#FFFFFF` |
| Unchecked (excluded) | background `#FFFFFF`, border 1px solid `#8B93A1` |
| Disabled (ignored by default or by .gitignore) | background `#F1F3F5`, border 1px solid `#D5D8DE`; name `#656974`; folder icon `#94A3B8` |

Rows (verbatim):

| Depth | Disclosure | Box | Name | Reason | Files |
| --- | --- | --- | --- | --- | --- |
| 0 | expanded | mixed | src | | 1,102 |
| 1 | collapsed | checked | api | | 640 |
| 1 | collapsed | unchecked | legacy | excluded | 184 |
| 1 | collapsed | checked | jobs | | 462 |
| 0 | collapsed | unchecked | scripts | excluded | 14 |
| 0 | collapsed | checked | tests | | 168 |
| 0 | none | disabled | node_modules | skipped by default | — |
| 0 | none | disabled | dist | skipped by default | — |
| 0 | none | disabled | .venv | ignored by .gitignore | — |

(The parent count is the included files: src 1,102 = api 640 + jobs 462.) Ignored folders are listed last, with
their reason and no count.

### Rules

Column, gap 6px, padding-top 8px.

- Heading row (flex, `align-items: baseline`, gap 10px): "Rules" (renders "RULES")
  `Inter 11/14 600 0.06em uppercase #656974`; "Written to the file from the boxes above." `Inter 12/16 #656974`.
- List: border-top 1px solid `#E2E8F0`. Row: height 32, flex, `align-items: center`, gap 8px, padding-left 2px,
  border-bottom 1px solid `#E2E8F0`.
  - Kind "exclude": `JetBrains Mono 12/16 #656974`, width 64px.
  - Pattern: `JetBrains Mono 12/16 #202126`, `flex-grow: 1`.
  - Remove: 28×28, radius 6px, icon 14×14 (viewBox 0 0 24 24) `<path d="M18 6 6 18M6 6l12 12" stroke="#656974" stroke-width="1.5"/>`.

Rows: `exclude  src/legacy/**` · `exclude  scripts/**`.

## Footer

Flex, `align-items: center`, `justify-content: space-between`, padding 14px 20px 14px 24px, border-top 1px solid
`#E2E8F0`.

- Note "Changes are saved together. Comments in the file are kept.": `Inter 12/16 #656974`.
- Buttons (flex, gap 8px):
  - **Cancel**: height 30, padding-inline 12px, background `#FFFFFF`, border 1px solid `#E0E2E7`, radius 6px,
    `Inter 13/16 500 #202126`.
  - **Save**: height 30, padding-inline 14px, background `#5862C0`, radius 6px, `Inter 13/16 500 #FFFFFF`.

## Not drawn (implementer decides)

Only the **Scope** section is drawn. From §4.11 and §6:

- **General**: name, path, background updates (on/off), and where the graph is stored ("using fallback location",
  "watching by scanning" notes). Not drawn.
- **Detectors**: "one switch per detector, grouped by language". **No switch component exists in any of these
  artboards.** Suggest a 28×16 track, radius 999px, off `#E0E2E7` / on `#5862C0`, 12px white knob, in 28px rows
  matching the folder tree rows.
- **Python projects**: not drawn.
- Save disabled state (no changes), saving/progress, and error (Rust write failed) states.
- Tree hover, keyboard focus, and scroll.
- Slide-in or open/close animation (see "Sheet vs. dialog").
- Icon set: this artboard uses Lucide 24px-viewBox icons; the rest of Home uses custom 16px icons with different
  folder/chevron shapes. Pick one set.
