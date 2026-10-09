# Where everything goes (artboard v2 · 00)

Transcription of the Paper artboard `v2 · 00 · Where everything goes` (file *Lazar's Playground*, page **Unfold App**,
1440 × 3341). It is a **reference board**, not a screen: nothing on it is built as UI. It maps every V1 region and
command to its new home. The copy below is verbatim; differences from §6 of
[the redesign doc](../../studio-redesign.md#6-where-everything-goes) are listed at the end.

## 1. Header

- Eyebrow: `Unfold · Studio redesign v2`
- Title: `From workbench to studio.`
- Lede: `Today the window has seven regions around one canvas. This page lists every V1 region and command, and where each one goes. Nothing is dropped without a new home.`

## 2. Schematics

### Today: `7 regions · canvas ≈ 52% of the window`

A wireframe of the V1 window with numbered regions:

| # | Label (verbatim) |
| --- | --- |
| ① | `Title bar: folder · board · lens · compile · publish · ⋯` |
| ② | (activity bar, 40px column, number only) |
| ③ | `Sidebar` / `Files · Entrances · Search · Boards` |
| ④ | `Tabs` |
| (center) | `Canvas` |
| ⑤ | `Panel: Problems · Index log · References` |
| ⑥ | `Inspector` |
| ⑦ | `Status bar: graph state · problems · zoom` |

### Proposed: `2 regions + floating controls · canvas ≈ 80%`

| Label | Text (verbatim) |
| --- | --- |
| Tab strip | Traffic lights, tabs `Home`, `Order lifecycle` (active), then `B · Window tabs` |
| A | `A · One sidebar` / `Home: projects & boards.` / `In a board: pages, on this page, selected.` |
| B | `B · Canvas top bar: page · publish state · Code|Flow · Share · Publish` |
| (center) | `Canvas` |
| C | `C · Dock: Add · back/forward · trail · Arrange · zoom` (floating, 320px wide, 16px above the canvas bottom, centered) |

## 3. The mapping

Title: `Every region and command` · `33 rows · 0 dropped`

Summary (right): `6 kept · 13 moved · 3 merged · 5 become Home · 3 become Add · 3 removed`

Columns: `Region today` (a name, with a muted line listing what it held) · `Goes to` · `Verdict`.

The counts check out against the rows: Kept 6, Moved 13, Merged 3, Becomes Home 5, Becomes Add 3, Removed 3 = 33.

### ① Title bar

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Folder switcher | Recent, Open Folder, Open Folder in New Window, New Window, Folder Settings, Close Folder, Preferences | The sidebar: the projects list, Add project, the project menu, and the gear. Recent folders become Recents (recent boards) in the sidebar. | Moved |
| Board title | Click to rename | The board header in the sidebar. Double-click to rename. | Moved |
| Lens: Code \| Flow | Switches every card on the board | The canvas top bar, centered. | Moved |
| Compile | Builds the folder's graph | First Compile on the project's Home. After that, the project menu: Compile again (⇧⌘B), Rebuild from scratch, Cancel. | Moved |
| Publish | Moves a draft into .unfold/boards/ | The canvas top bar, next to the board's publish state. | Moved |
| Inspector toggle | Opens the right-hand panel | Nothing to toggle: the Selected section lives in the sidebar. ⌘B toggles the sidebar. | Removed |
| Board menu ⋯ | Rename, Export Mermaid, Copy Mermaid, Fold All, Unfold All, Move to New Window, Move Back to Drafts, Delete | The board menu in the sidebar header. Fold All and Unfold All are also in the canvas context menu. Mermaid export moves under Share. | Moved |

### ② Activity bar · ③ Sidebar views

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Activity bar | Switches the sidebar: Files · Entrances · Search · Boards | Gone. The sidebar has one job at a time, so nothing switches it. Its four views go to the rows below. | Removed |
| Files view | The disk tree, marks, its context menu | Finding a file: Add → Files, with marks in the results. Exclude, Include, Graph only this folder: Project settings → Scope. Open in editor, Reveal in Finder: the peek sheet's menu and the project menu. Show callers: the card menu. | Becomes Add |
| Entrances view | Every way the code starts, Add Entrance, manual entrances | Home → How it starts, and Add's map. Add Entrance: the project menu and the application menu on Home. Edit and Remove a manual entrance: its menu on Home. | Becomes Home |
| Search view | All, Definitions, Entrances, Files, Text; match case, whole word, regex | Add, with the same tabs and the same text options, across every connected project. | Becomes Add |
| Boards view | Published, drafts, outline, Flow-label stepper, groups, health and repairs | Boards: Home's Boards section and the sidebar tree. Outline and groups: On this page. Flow-label stepper: "Label them" under On this page. Health and repairs: the card banner and Home's Health. | Merged |

### ④ Tabs and the canvas · ⑥ Inspector

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Editor tabs | One tab per open board | Window tabs at the very top, with Home pinned first. | Moved |
| Source tabs | Read-only source files beside the boards | The peek sheet over the right of the canvas. Pinned, it narrows the canvas instead. | Moved |
| Board bar | Back, trail, zoom, Fit, Arrange | The dock, floating at the bottom of the canvas, with Forward added. | Moved |
| Canvas and card context menus | Right-click on the canvas or a card | Unchanged, restyled. | Kept |
| Banners | Edited outside Unfold, failed save | Unchanged, at the top of the canvas. | Kept |
| Inspector | Description, Calls, Called by, Reached from, Groups, Flow node label and paths | The sidebar's Selected section, under Pages and On this page. Descriptions and Flow labels can also be edited on the card. No right-hand panel. | Merged |

### ⑤ Bottom panel

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Problems | Files that don't parse, settings errors, board health | Home → Health, and the full Health and activity view. | Becomes Home |
| Index log | What each compile and update did | Health and activity, opened from the project's Home. | Becomes Home |
| References | Callers as a tree, entrances that reach it | Selected → Called by and Reached from. Show callers on a card adds the callers' cards to its left. | Merged |

### ⑦ Status bar

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Status bar | Graph state, notes, connection, problem count, zoom | Graph state and problems: the project row and Home's state line. "Using fallback location" and "watching by scanning": the state line's tooltip and Project settings → General. The connection: a toast. Zoom: the dock. | Removed |
| Status menu | Compile, Rebuild, Cancel, Pause or Resume, Show index log, Copy Diagnostics | The project menu. Copy Diagnostics is also in the Help menu and Add's commands. | Moved |

### Screens and dialogs

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Welcome screen | Open Folder, recent folders, drop a folder | First-launch Home, Add project, and dropping a folder anywhere on the window. | Becomes Home |
| First Compile screen | Counts, skipped, Choose folders, Compile | The project's Home in its not-compiled state, with the scope tree inline. | Becomes Home |
| Close Folder | Leaves the folder and returns to Welcome | Remove from workspace, in the project menu. It asks first and never deletes .unfold/. | Moved |
| Folder Settings dialog | Scope, detectors, Python projects | The Project settings sheet: General, Scope, Detectors, Python projects. | Moved |
| Manual entrance editor | Adds or edits an entrance by hand | Unchanged, restyled. | Kept |
| Preferences | App-wide settings, such as the theme | Unchanged, restyled. Reached from the sidebar gear and ⌘,. | Kept |

### Commands, menus and windows

| Region today | What it held | Goes to | Verdict |
| --- | --- | --- | --- |
| Command palette · Go to File · Go to Symbol | ⇧⌘P, ⌘P, ⌘T | Add. Type > for commands. ⌘P opens Add on Files, ⌘T on Functions. | Becomes Add |
| Copy Diagnostics | From the status menu | The Help menu, the project menu, and Add's commands. | Moved |
| Several windows | New Window, Move to New Window, restore on launch | The same, per workspace instead of per folder. Still one window per board. | Kept |
| Application menu | File, Edit, View, Go, Window, Help | Kept, with File → Add Project…, Remove Project; View → Home, Toggle Sidebar; Go → Add. | Kept |

## 4. Board styling (for reference only)

Not built, but recorded in case the board is reproduced in docs.

- Artboard: 1440px wide, height fit-content, padding 72px block / 80px inline, flex column, gap 56px, fill `#FFFFFF`.
- Eyebrow Inter 11px / 14px weight 600, `0.12em` (`--tracking-caps`), uppercase, `#5862C0`. Title Inter 56px / 58px
  weight 800, `-0.04em`, `#202126`. Lede Inter 15px / 23px `#656974`, 420px wide. Header gap 48px, bottom-aligned.
- Schematics: two columns, gap 48px; frames 380px tall, radius 10px, border 1px `#C9CDD5` (no token), canvas fill
  `#F7F8FA`, selection fill `#EDF0FC` for the proposed canvas.
- Table: column widths 340px / flex / 120px, gap 40px. Head row: caps labels (11px / 14px, 600, `0.06em`, `#656974`),
  bottom border 1px `#202126`. Group heading: padding 24px top / 8px bottom, 11px / 14px 600 caps `#202126`, bottom
  border `#E0E2E7`. Rows: padding-block 12px, bottom border `#E0E2E7`; name Inter 13px / 20px 600 `#202126`; held
  list Inter 12px / 16px `#656974`; destination Inter 13px / 20px `#202126`.
- Verdict pills (Inter 11px / 14px weight 600, radius 999px):

  | Verdict | Fill | Border | Text | Padding |
  | --- | --- | --- | --- | --- |
  | Moved, Merged | `#F1F2F4` (no token) | none | `#3F4451` (no token) | 3px 8px |
  | Becomes Home, Becomes Add | `#EDF0FC` | none | `#5862C0` | 3px 8px |
  | Kept | `#FFFFFF` | 1px solid `#D4D7DD` (no token) | `#3F4451` | 2px 7px |
  | Removed | none | 1px **dashed** `#B9BEC8` (no token) | `#656974` | 2px 7px |

## 5. Differences from the redesign doc §6

The two tables have the same 33 rows in the same order and agree on every destination. The artboard adds:

1. **A verdict per row** (Kept, Moved, Merged, Becomes Home, Becomes Add, Removed) and the summary count. §6 has
   neither.
2. **Region numbering ①–⑦** and group headings, plus the "what it held" sub-line for every row (§6 folds some into
   parentheses and omits others: Board title, Lens, Compile, Publish, Inspector toggle, Editor tabs, Source tabs,
   Context menus, Banners, Index log, Close Folder, Folder Settings, Manual entrance editor, Preferences, Copy
   Diagnostics, Application menu).
3. **The canvas share**: "≈ 52%" today and "≈ 80%" proposed (§1 says "about half" and "about 80%").
4. Extra detail in some destinations, none contradicting §6:
   - Publish: "next to the board's publish state".
   - Activity bar: "The sidebar has one job at a time, so nothing switches it."
   - Search view: "across every connected project".
   - Editor tabs: "at the very top, with Home pinned first".
   - Source tabs: "Pinned, it narrows the canvas instead."
   - Board bar: "floating at the bottom of the canvas".
   - Inspector: "under Pages and On this page … No right-hand panel."
   - Index log: "opened from the project's Home".
   - First Compile screen: "with the scope tree inline".
   - Folder Settings dialog: names the four sheet sections.
   - Several windows: "Still one window per board."

Inconsistencies with other artboards that the implementer should know:

- **"The same tabs"** (Search view row) is not literal: V1's tabs are All, Definitions, Entrances, Files, Text; Add
  (artboard 04) shows All, Entrances, **Functions**, Files, Text, so Definitions becomes Functions. The **match case,
  whole word, regex** options the row promises are not drawn on any Add artboard.
- **Board menu**: V1's "Move to New Window" and "Move Back to Drafts" appear in artboard 12 as `Open in new window` and
  `Move back to drafts`; artboard 12 also adds `Move to folder`, `Duplicate`, `Discard changes…`, and a `Share`
  submenu, so Mermaid is under Share in **both** the top bar and the board menu.
- The proposed schematic labels both the window tabs and the canvas top bar **"B"**.
