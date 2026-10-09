# UI shell

This document describes the Unfold window as it is built: what each part shows, which commands it offers, and
the keyboard map. The design it implements is [architecture §11](architecture.md#11-the-desktop-app). What
cards, Code, Flow, and boards show is in [the reader](reader.md), the commands the window sends to the engine are in
[IPC](ipc.md), and the Electron side (processes, menu, window restore, packaging) is in
[desktop/README.md](../desktop/README.md). The code is `ui/src/` (React) and `desktop/src/main/` (the application
menu and windows).

## The window

```text
┌ title bar: folder ▾ · board title [Draft] ·  [ Code | Flow ]  · Compile · Publish · ◨ · ⋯ ┐
├──┬─────────────┬──────────────────────────────────────────────┬────────────┤
│A │ sidebar     │ editor: board tabs and source tabs           │ inspector  │
│c │ (one view)  │                                              │ (closed by │
│t │             │                                              │  default)  │
├──┴─────────────┴──────────────────────────────────────────────┴────────────┤
│ bottom panel: Problems · Index log · References                            │
├────────────────────────────────────────────────────────────────────────────┤
│ status bar: graph state · notes · connection          problem count · zoom │
└────────────────────────────────────────────────────────────────────────────┘
```

The sidebar, inspector, and bottom panel resize from their inner edges (a resize handle also takes the arrow keys,
Home, End, and Enter). A new window starts with the Files view, the sidebar and bottom panel open on Problems, and
the inspector closed. The layout, the lens, and the open tabs are remembered per window ([Restore](#restore)).

## Title bar

The title bar is also the window's drag region (`ui/src/shell/TitleBar.tsx`).

- **Folder switcher** (the folder's name, `▾`): up to eight recent folders, then **Open Folder…** (⌘O), **Open
  Folder in New Window…**, **New Window** (⇧⌘N), **Folder Settings…**, **Close Folder**, and **Preferences…** (⌘,). With no folder
  open the title bar shows "Unfold" instead.
- **Board title**, while a board tab is active: click it to rename (Enter saves, Escape cancels). A draft shows a
  **Draft** chip; a published board shows its path under `.unfold/boards/`.
- **Code | Flow**, the window's lens. Cards that have no view of their own follow it; a card's own switch (in its
  header) overrides it and is saved with the board. The lens is saved with the window, not with the board.
- **Compile.** Quiet while the graph is current (its tooltip says Compile checks every file again), a plain button
  when a Compile would change something, and "Compiling · 12 of 84 files" (then the stage in words: linking,
  resolving calls, finding entrances, checking the graph) with a **Cancel Compile** button while one runs.
- **Publish**, only for a draft board, opens the Publish dialog, which names the file it writes
  (`.unfold/boards/<name>.json`); afterwards edits save into that file ([reader: Files](reader.md#files)).
- **Toggle Inspector** (◨, ⌥⌘B or `]`).
- **Board menu** (`⋯`): Rename…, Export Mermaid…, Copy Mermaid, Fold All, Unfold All, Move to New Window, Move
  Back to Drafts (published boards only), and Delete….

Before the first Compile only the folder switcher is shown.

## Activity bar and sidebar views

The activity bar holds one icon per view, with Preferences at the bottom. Clicking the active view's icon hides the
sidebar (⌘B toggles it too).

| View | Shortcut | What it shows |
| --- | --- | --- |
| **Files** | ⌘1 | The folder as it is on disk. Indexed files expand into their definitions in source order. Quiet marks: `→N` entrances registered in the file, `●` on the open board, an amber dot for a changed or pending file, a warning for a file that doesn't parse, "not indexed", "ignored", and "N launches" on `package.json` and `pyproject.toml`. Header buttons show or hide ignored files and collapse all. Context menu: Open card, Open source, Open in editor, Show callers, Exclude from graph / Include in graph, Graph only this folder, and Reveal in Finder (Show in Explorer, Show in file manager) |
| **Entrances** | ⌘2 | Applications named after their startup, with their launches, then their entrances with a fixed method column and a language tag; then "Not reached from a launch" and "Launches into code that isn't indexed". Clicking an entrance opens its card and its handler cards. The `…` menu has Add Entrance… and Folder Settings…; a manual entrance's context menu has Edit… and Remove |
| **Search** | ⌘3 (`/` focuses the field) | One field with tabs All, Definitions, Entrances, Files, Text. Text search streams matches and has Match case, Match whole word, and Use regular expression; a new query cancels the running one. Definitions and entrances open cards; files and text matches open source tabs. A definition's context menu also offers Add as Entrance… |
| **Boards** | ⌘4 | Published boards (filled square) and "Drafts · on this machine" (open square), with a label only when something is wrong, and **+** for a new board. For the open board: its outline in follow order, indented by call depth (click focuses the card), "N Flow nodes still show code names" with a stepper, its groups, and its health problems with their repairs. A board's context menu has Move to New Window and Delete… |

## Editor tabs

Board tabs and source tabs share one tab strip (`ui/src/editor/`). A board tab shows the board's title and a Draft
chip; a source tab shows the file name. Delete or Backspace on a focused tab closes it. The tab context menu has
Close (⌘W), Close Others, and then **Move to New Window** for a board tab or Reveal in Finder for a source tab. With
no tab open the editor says how to start ("Open an entrance or a definition to start a board, or a file to read
it", with ⌘P), and ⌘W closes the window.

### Board tabs

A board tab is a canvas of cards (`ui/src/canvas/`). The bar at its bottom has Back (⌘[), the trail of the follow
path, Zoom Out (⌘-), Zoom In (⌘=), Fit Board (⇧1), and Arrange by call depth; Forward is ⌘]. The canvas context menu has
Fit Board, Fold All Cards, and Unfold All Cards; a card's context menu has Show in Inspector, Fold Card / Unfold
Card, Remove from Board, and, on a group, Expand / Collapse Group, Describe Group…, and Ungroup.

- **Following calls.** In Code, a resolved call is a link: hover shows the target's signature and location, a click
  opens (or focuses) the target's card, and ⌥-click expands the callee inline under the calling line. External
  calls show their module; unresolved calls have a dashed underline whose tooltip gives the reason.
- **Groups.** Drag over line numbers in a Code card's gutter to select consecutive statements, then type the
  group's title; or run **Make a Group in Selected Card…** and type the lines (`27-31`). A group shows on a rail
  with its title. F2 on a group renames it.
- **Banners.** Two quiet notices can sit above the canvas: an outside edit made while this window had unsaved
  changes (Keep mine / Take theirs), and a failed save (Retry). Neither is modal.

### Source tabs

A source tab shows one whole file, read-only, drawn by the same renderer as Code cards: syntax colors, folds, and
calls as links (hover peeks, a click opens the target's card). A `⊕` in the gutter beside each definition adds it to
the open board, and `●` marks what's already there. Long files are fetched in pages and virtualized by line. When
the file changed since the graph was built, a warning chip says so and no links are drawn until the next update.
Binary and very large files are not shown. The header's `…` menu has Open in editor and Reveal in Finder.

## Inspector

The inspector (`ui/src/inspector/`) is on the right, closed by default, and never shows code. It follows the
active board's selection:

- **A card:** its description (editable), then for a block card its **Calls** (on the board, **+ Open**, or external
  and unresolved with their module or reason), **Called by**, **Reached from**, and **Groups** with their
  descriptions; for an entrance card, its **Handlers**.
- **A Flow node:** its type and lines, its Flow label (editable, showing the code name it replaces), and its
  **Paths** (where the yes and no routes go).

Every edit is a board edit signed with your name (`person:<username>`), so ⌘Z undoes it as on the canvas.
**Describe Selected Card** in the command palette jumps to the description field.

## Bottom panel

Toggle it with ⌘J; its tabs are:

- **Problems:** files that don't parse or can't be read, settings errors, discovery and input problems (such as an
  unreadable `package.json` or `pyproject.toml`, an `extends` that names a package that isn't installed, or a Python
  import that more than one source root supplies), a failed compile, and the open boards' health. Unresolved calls
  are never problems ([U-28](decisions-and-tradeoffs.md#u-28-problems-lists-only-real-problems)). A file entry opens
  a source tab at its line; a board entry offers its repairs (or opens the board, or focuses the window that has it).
- **Index log:** each committed update, newest first: files read, taken from cache, or failed, time per stage, and
  counts by resolution, reason, entrance kind, and detector.
- **References:** for the selected definition, its callers as a tree (each caller expands into its own callers) and
  the entrances it is reached from.

**Show Problems** and **Show Index Log** in the command palette open the panel on that tab.

## Status bar

From left to right: the graph's state, quiet notes, the engine connection when it isn't connected, then the problem
count (click to show Problems) and the active board's zoom. The states (`ui/src/shell/status.ts`, the engine side
in [architecture §7.9](architecture.md#79-what-the-status-bar-says)):

| Glyph | Text | Meaning |
| --- | --- | --- |
| `○` | Not compiled yet | The folder has no graph yet |
| `●` | Graph current · 84 files · TypeScript + Python | The graph matches the files on disk |
| `◌` | Compiling · 12 of 84 files (then the stage) | A Compile is running |
| `◌` | Updating · 3 files changed (or "checking the folder") | A background update is running |
| `●` (amber) | 1 file doesn't parse · showing its last version that parsed | Some files are broken; the graph keeps their last good version |
| `●` (amber) | Background updates paused · N files changed since the last compile | Background updates are off for this folder |
| `▲` | Compile failed · see Problems | The last Compile failed; the previous graph stays |

Notes after the state: "using fallback location" (the folder is read-only, on a network volume, or can't hold the
database safely, so `.unfold/local/` lives in Unfold's app data; the tooltip says which) and "watching by scanning"
(file events aren't available, so the folder is checked every few seconds). The connection item reads "Connecting
to the engine…", "Reconnecting…" (the engine host is restarting; the windows keep their boards), or "Engine
unavailable".

Clicking the state opens a menu: Compile (⇧⌘B), Rebuild from scratch, Cancel Compile (while one runs), Pause or
Resume background updates, Show index log, and Copy Diagnostics.

## Welcome and the first Compile

With no folder, the window shows the welcome screen: **Open Folder…** (⌘O), the recent folders with when each was
last opened (each can be forgotten, and the list is read again each time the screen shows), and dropping a folder
anywhere on the window opens it.

**Close Folder** (File menu, folder switcher, or the palette) returns the window to the welcome screen: the window
writes its unsaved board edits, then the main process's `close_folder` window command re-attaches the window to the
engine host with no folder, which detaches it from the folder's workspace (the folder's last window closes it). The
window stays open on every platform, and other windows on the folder keep it.

A folder that has never been compiled shows the first-Compile screen instead of the workbench: the folder's name,
how many TypeScript and Python files would be graphed, what is skipped and why (grouped by reason, for example
"node_modules, dist (skipped by default)"), any settings error (the defaults are used then) or discovery problem,
**Choose folders…**, and **Compile** (⇧⌘B), with the promise: "Reads files only. Runs nothing and installs nothing.
Creates .unfold/ in this folder for the graph and your boards. After this, the graph updates itself as files
change." For a folder that can't be written, it names the fallback location instead. While compiling it shows a
progress bar and Cancel.

**Choose folders…** lists the top-level folders. Picking some writes `[scope] include` into `.unfold/config.toml`
(as **Graph only this folder** for the first and **Include in graph** for the rest) before the first Compile, which
helps with large monorepos.

## Folder settings and the manual entrance editor

**Folder Settings…** (folder switcher, Entrances view `…`, or the command palette) edits the open folder's shared
`.unfold/config.toml` ([settings and scope](settings-and-scope.md)). It shows whether the file exists, and has three
sections:

- **Python projects:** each project's folder, source roots, and Python grammar (3.12, 3.13, 3.14), with add and
  remove.
- **Detectors:** a switch per detector, grouped by language (package.json scripts and bin, Express routes, BullMQ
  workers, node-cron schedules; pyproject.toml scripts, FastAPI routes, Celery tasks, APScheduler jobs).
- **Scope:** the Graph only and Excluded patterns, each with a remove button. Patterns are added from the Files
  view's context menu.

Changes are staged and saved together, all or nothing, through Rust, which keeps the file's comments and formatting
and writes the file only when a setting differs from the defaults. An edit that would make the file invalid is
refused with the error shown in the dialog. The graph picks a saved change up in the background.

The **manual entrance editor** (Add Entrance… in the Entrances view or the palette, Add as Entrance… on a
definition, Edit… on a manual entrance) edits one `[[entrance]]` table: kind, trigger, method (HTTP only), label,
and either handlers (in order, with buttons to move one earlier or remove it) or, for a process, its startup.
Locators are checked while typing ([entrances](entrances.md#manual-entrances-and-locators)); Remove asks for
confirmation first.

## Preferences

Preferences (⌘,, the activity bar, or the folder switcher) are app-wide and stored by Rust in `preferences.json`:
theme (applied at once), the editor command used by Open in editor (for example `code -g {path}:{line}`; it runs
without a shell, and empty opens files as text in the system's editor), whether background updates start on for
newly compiled folders, the compile thread limit (applies to folders opened afterwards), and whether to check for
new versions of Unfold, and key bindings (the **Keyboard** section; see [Keyboard](#keyboard) below). Saving sends
`preferences_changed` to every open window, welcome windows included, so a new theme or key bindings apply
everywhere at once ([IPC: events](ipc.md#events)).

## Copy Diagnostics

**Copy Diagnostics** (Help menu, the status bar menu, or the palette) copies a redacted report for bug reports:
Rust's summary (engine version, platform, graph state and counts, problems by kind, watch mode, lock state, settings
counts) plus the app, Electron, and Chromium versions. It holds no code and no paths
([IPC: `diagnostics`](ipc.md#settings)). The main process writes the clipboard, because windows have no clipboard
permission.

## Several windows

- **One folder per window.** New Window (⇧⌘N) opens another window on the same folder; Open Folder in New Window…
  opens a different folder beside it. Each window has its own tabs, sidebar, inspector, panel, lens, and selection.
- **Windows on one folder share it:** one engine, one graph, one watcher. A save updates the cards in every window
  that shows them, and every window's status bar shows the same state.
- **A board is open in one window at a time.** Opening a board another window holds focuses that window instead.
  **Move to New Window** (tab menu, board menu, Boards view, or the palette) moves a board, with its undo history,
  into a new window on the same folder. Source tabs can be open in any number of windows.
- **Closing.** Closing the last window of a folder saves its drafts and closes its workspace. On macOS the app keeps
  running with no windows; on Windows and Linux closing the last window quits.
- **Engine restarts.** If the engine host stops, every window shows "Reconnecting…", keeps its boards, and
  reconnects when the main process has restarted it; unsaved board edits are saved after reconnecting.

## Restore

Each window has a stable key. Its tabs, active tab, layout, lens, and open boards are saved (debounced) into the
folder's `.unfold/local/state.json`, and its position and size by the main process. Reopening a folder recreates
each of its saved windows (at most 16), moved onto a visible screen if a display has gone, and reopens their boards;
a board that can no longer be opened loses its tab. A window you close (other than the folder's last) is forgotten.
Details: [desktop/README.md](../desktop/README.md#window-restore).

## Command palette

⇧⌘P opens the palette with every enabled command and its shortcut, fuzzy-filtered. ⌘P (Go to File…) and ⌘T (Go to
Symbol…) open it as quick-open; typing `>` first switches to commands. Commands that have no shortcut are reached
here, among them Open Folder in New Window…, Close Folder, Rebuild from Scratch, Cancel Compile, Show Index Log, Show Problems,
Add Entrance…, Folder Settings…, Copy Diagnostics, New Board, Publish Board…, Rename Board…, Export Board as
Mermaid…, Copy Board as Mermaid, Fold All Cards, Unfold All Cards, Move Board to New Window, Move Board Back to
Drafts, Delete Board…, Describe Selected Card, Make a Group in Selected Card…, Remove Selected Card from Board,
Arrange by Call Depth, and Next Flow Node Showing a Code Name.

## Keyboard

The map is `ui/src/keyboard/keymap.ts`. `Mod` is ⌘ on macOS and Ctrl on Windows and Linux; shortcuts are shown in
each platform's style (`⇧⌘P`, `Ctrl+Shift+P`).

| Action | macOS | Windows and Linux |
| --- | --- | --- |
| Open Folder… | ⌘O | Ctrl+O |
| New Window (same folder) | ⇧⌘N | Ctrl+Shift+N |
| Close Tab (closes the window when no tab is open) | ⌘W | Ctrl+W |
| Close Window | ⇧⌘W | Ctrl+Shift+W |
| Compile | ⇧⌘B | Ctrl+Shift+B |
| Go to File…, Go to Symbol… | ⌘P, ⌘T | Ctrl+P, Ctrl+T |
| Command palette | ⇧⌘P | Ctrl+Shift+P |
| Files, Entrances, Search, Boards | ⌘1, ⌘2, ⌘3, ⌘4 | Ctrl+1 … Ctrl+4 |
| Focus Search | `/` | `/` |
| Toggle Sidebar | ⌘B | Ctrl+B |
| Toggle Inspector | ⌥⌘B or `]` | Ctrl+Alt+B or `]` |
| Toggle Panel | ⌘J | Ctrl+J |
| Back, Forward (board) | ⌘[, ⌘] | Ctrl+[, Ctrl+] |
| Fit Board | ⇧1 | Shift+1 |
| Zoom In, Zoom Out (board) | ⌘= (or ⇧⌘+), ⌘- | Ctrl+= (or Ctrl+Shift++), Ctrl+- |
| Undo, Redo (board edit) | ⌘Z, ⇧⌘Z | Ctrl+Z, Ctrl+Shift+Z |
| Preferences… | ⌘, | Ctrl+, |
| Copy Diagnostics | none (menu and palette) | none |

How keys are dispatched (`ui/src/keyboard/dispatcher.ts`):

- Plain keys (`/`, `]`, `⇧1`) never fire while typing in a text field; ⌘/Ctrl shortcuts do, except Undo and Redo,
  which leave a text field its own native undo.
- While a dialog or the palette is open, no window shortcut fires.
- Undo and Redo go to the part of the window you're working in (the focused board, or the one clicked last).
- The application menu's items run the same actions as their keys (the main process forwards them as
  `menu_command` events), so a key runs its action once.
- In lists and trees, arrow keys move, Enter opens, and typing letters jumps by name; Shift+F10 or the context-menu
  key opens a context menu; Escape closes menus, popovers, and dialogs.

### Custom key bindings

Every action in the table except Preferences… can be rebound in Preferences → **Keyboard**, which lists the actions
by category with their effective shortcut. Click a row's shortcut and press the new combination (modifiers alone
keep it listening; Escape cancels; Backspace leaves the action with no shortcut). **Reset** returns a row to its
default and **Reset All** every row. If two actions end up on the same combination (including an action's second
default, such as `]` for Toggle Inspector), both rows are flagged and Save stays disabled until one changes.

Bindings are stored in `preferences.json` as `keyBindings`, a map from action id (`ui/src/keyboard/keymap.ts`,
e.g. `view.toggleSidebar`) to one shortcut in the keymap's notation (`Mod+Shift+K`, case-insensitive; `Plus` and
`Space` name those keys); `""` means no shortcut. A binding replaces all of an action's default shortcuts.
Entries for unknown ids and shortcuts that don't parse are ignored (a hand-edited file never breaks the keyboard),
and Save carries entries for ids this version doesn't know over untouched (`ui/src/keyboard/bindings.ts`).

The window applies the bindings when preferences load and whenever they're saved: the action registry re-resolves
every shortcut, so the dispatcher, the palette, and tooltips show and use the effective ones. There is no
cross-window preferences event: another open window picks the change up (theme included) when it's reopened or when
Preferences is opened in it, which always re-reads `preferences.json`.

The application menu keeps its default accelerators. For actions the menu also owns (Open Folder, New Window,
Compile, Close Tab, Close Window, Undo, Redo, Command Palette, the four views, and the three toggles), the old
default still works through the menu and is the one the menu shows, unless the new bindings give that combination
to another action, in which case the window handles it first and the menu doesn't fire.
