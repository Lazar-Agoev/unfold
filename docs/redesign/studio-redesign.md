# The studio redesign

This document describes the redesign of Unfold's window, from an IDE-shaped workbench to a studio that works like
the Paper desktop app with repositories and projects in place of design files. It says how the app should work
after the redesign, what has to be rebuilt in each layer, and which decisions must be made before anyone starts
building it. The designs are in Paper, on the **Unfold App** page of *Lazar's Playground*: the artboards named
`v2 · …` ([§5](#5-screens)).

Status: **ready to build**. Each decision in [§9](#9-decisions) takes its recommendation unless the build's brief
says otherwise. The decisions are recorded in [decisions and trade-offs](../decisions-and-tradeoffs.md), and the
reference documents ([UI shell](../ui-shell.md), [IPC](../ipc.md), [reader](../reader.md), [user
guide](../user-guide.md)) are rewritten as each step lands.

**Unfold has not shipped.** There are no users, no published boards, and no saved preferences to keep. So the
redesign replaces what it changes: no migration from the V1 board format, no compatibility shims, no deprecated
aliases or old action ids, and no code kept "for later". Code the new design doesn't use is deleted, and V1's
decisions are revised in place rather than superseded, as was done when the shell moved to Electron.

The shell stays **Electron**, with the Rust engine as a native addon in the engine host
([U-6](../decisions-and-tradeoffs.md#u-6-electron-for-the-shell-with-the-rust-engine-as-a-native-addon)). The
redesign is mostly a renderer change, and it does not change the engine's compilers or the graph.

## Contents

1. [Why redesign](#1-why-redesign)
2. [The model](#2-the-model)
3. [Principles](#3-principles)
4. [How the app works](#4-how-the-app-works)
5. [Screens](#5-screens)
6. [Where everything goes](#6-where-everything-goes)
7. [Visual system](#7-visual-system)
8. [Keyboard](#8-keyboard)
9. [Decisions](#9-decisions)
10. [What gets rebuilt](#10-what-gets-rebuilt)
11. [Build order](#11-build-order)
12. [Before implementation starts](#12-before-implementation-starts)

## 1. Why redesign

V1's window has seven regions around one canvas: a title bar, an activity bar, a sidebar with four views, editor
tabs, a right inspector, a bottom panel, and a status bar ([U-26](../decisions-and-tradeoffs.md#u-26-an-ide-shaped-window)).
The canvas, which is what people open Unfold for, gets about half of the window. The rest mirrors an IDE: a tree of
the folder on disk, five different ways to find something, and a status bar that describes "the" folder.

Three things don't fit how Unfold is used:

- **One folder per window.** Real systems are several repositories: a web app, an API, a worker. Today each needs
  its own window, its own sidebar, and its own search.
- **The work is boards, not files on disk.** The disk tree is the most prominent thing in the window, but people
  come to read a path and explain it.
- **The important things are hidden.** The inspector, where descriptions are written, is closed by default.
  Problems sit in a bottom panel. Entrances, the best starting point, are the second of four sidebar views.

The redesign gives the canvas about 80% of the window, makes projects (repositories) first-class, and moves every
job onto the thing it describes.

## 2. The model

| Thing | What it is | Where it's stored |
| --- | --- | --- |
| **Workspace** | Everything you've connected on this machine. One per person. Home lives here | App data: `workspace.json` |
| **Project** | A folder with its own graph, usually a repository. Shows its branch and switches worktrees in place | The folder itself, with its `.unfold/` |
| **Folder** | Yours to organize boards inside a project, like folders in Paper. Not the disk layout | A field on each board |
| **Board** | A story about the code, such as "Order lifecycle". A draft or published. Can hold cards from other projects | `.unfold/boards/` (published) or `.unfold/local/drafts/` |
| **Page** | One canvas inside a board: one trace, such as "Placing an order". What V1 calls a board | Inside the board file |
| **Card** | An entrance or a function, in Code or Flow. Unchanged from V1 | Inside a page |

The first direction called the story a **file**. This revision calls it a **board**, because "file" already means
source files everywhere in a code reader (Home shows "1,284 files" next to a "Files" section). This is decision
[R-1](#r-1-what-a-story-is-called), and the designs can be relabeled cheaply if you choose "file".

**Projects hold code. Boards hold stories.**

## 3. Principles

1. **One sidebar, and it follows you.** On Home it holds projects and their boards. Inside a board it holds pages,
   the cards on the page, and the selected card. Never two at once, and no right-hand inspector.
2. **State lives on the thing it describes.** Graph state on the project, health on the card, publish state on the
   board. No status bar.
3. **Finding code is adding it.** One **Add** finds entrances, functions, files, and text in every connected
   project and puts what you pick on a page. The same box runs commands.
4. **Home answers "how does it start?"** A project opens on its entrances, grouped by application, and then the
   boards people made about it.
5. **A trace doesn't stop at a repository.** A page can show the web app's checkout handler next to the API route
   it calls.
6. **Nothing covers cards for long.** The dock and the lens float at the edges. Sheets and dialogs cover the
   canvas only while you use them, and Escape always closes them.
7. **The engine guarantees don't change.** Unfold reads files only. It never runs the code it reads, installs
   nothing, and writes only inside `.unfold/` and its own app data.

## 4. How the app works

### 4.1 First launch

The window opens on Home with an empty workspace. It shows a short line about what Unfold does, **Add project…**
(⌘O), and a drop zone. Adding a folder connects it as a project. Nothing is written to the folder yet.

### 4.2 Connecting a project and the first Compile

A project that has never been compiled opens on its Home in the **not compiled** state. It shows what V1's
first-Compile screen showed: how many TypeScript and Python files would be graphed, what is skipped and why,
settings or discovery problems, the promise ("Reads files only. Runs nothing and installs nothing. Creates
`.unfold/` in this folder for the graph and your boards."), and the **scope tree**: the top-level folders with
check boxes, so a large monorepo can graph just part of itself. **Compile** (⇧⌘B) starts it. The Home fills in as
the compile finds entrances, with a progress ring on the project's row in the sidebar.

After the first Compile there is no Compile button in the chrome: the graph updates itself in the background
([U-13](../decisions-and-tradeoffs.md#u-13-compile-once-then-update-in-the-background)). Compile again, Rebuild
from scratch, Cancel, and Pause background updates are in the **project menu** (`⋯` on the project row and on its
Home).

### 4.3 Home

Home is the first, pinned tab of every window. It shows the project selected in the sidebar:

- **Header:** the path, the name, the branch and worktree switcher, the graph state in words ("Graph current ·
  1,284 files · TypeScript + Python · updated 2 min ago"), the project menu, and **New board**.
- **How it starts:** the project's applications (named after how they start, such as `server` or `worker`), each
  with its entrances. An application with more than eight entrances shows the first eight, a count, and a filter.
  Routes are grouped by their first path segment. Hovering an entrance shows **Trace**, which opens it on a new
  page of a new draft board. Below them: "Not reached from a launch" and "Launches into code that isn't indexed",
  collapsed.
- **Boards:** cards with a thumbnail of each board's first page, drawn from card positions (not a screenshot),
  with All, Published, and Drafts filters and sorting by last edited.
- **Health:** one line that sums up problems (files that don't parse, settings errors, boards that need review),
  each with an action. **Health and activity** expands into the full list and the index log (V1's Problems and
  Index log tabs).

### 4.4 Reading a board

Opening a board opens it as a tab at the top of the window, after Home. The sidebar switches to the board:

- **Header:** back to Home, the project and folder, the board's name, and the board menu.
- **Pages:** each page, with a health mark when something on it needs review. **+** adds a page.
- **On this page:** the cards in follow order, indented by call depth, like Paper's layers. Groups are nested under
  their card. A mark shows a card that needs review. "N Flow steps unlabeled · Label them" steps through Flow nodes
  that still show code names.
- **Selected:** the selected card's location, its description (editable here and on the card), Called by, Reached
  from, and calls not yet opened (with **+ Open**). For a Flow node: its label, the code name it replaces, and its
  paths. This section is collapsible and scrolls on its own.

The **top bar** of the canvas shows the page's name and the board's publish state ("Draft", "Published", or
"Published · 2 unpublished changes"), the window's **Code | Flow** lens in the middle, and **Share** (Export
Mermaid, Copy Mermaid) and **Publish** on the right.

The **dock** floats at the bottom of the canvas: **Add** (`A`), Back and Forward, the follow trail, **Arrange**,
and the zoom (−, percentage, +, Fit). It holds everything V1's board bar held.

Following calls, ⌥-click inline expansion, groups, folds, Code and Flow, descriptions and labels all work as in V1
([the reader](../reader.md)).

### 4.5 Adding code

**Add** (`A`, ⌘K, or `/`) opens one box over the canvas:

- **Before you type** it shows a map: every connected project's applications and their entrances. Entrances already
  on the page are marked. A **Look in** column limits it to one project.
- **When you type** it searches entrances, functions, files, and text in every connected project, with tabs and
  counts. The right side previews the selection: its location, the first lines of code, Reached from, Calls, Called
  by.
- **Enter** adds the selection to the current page, **⌘Enter** adds it on a new page, and **Tab** switches the kind.
  A file opens in the source peek sheet ([4.6](#46-reading-a-whole-file)); a text match opens the sheet at its line.
- **Typing `>`** switches to commands, which replaces V1's command palette. ⌘P and ⌘T open Add on Files and on
  Functions.
- From Home (no page open), Enter creates a new draft board in the selected project.

### 4.6 Reading a whole file

A source file opens in a **peek sheet** that slides in from the right over the canvas: the whole file, read-only,
with syntax colors, folds, calls as links, and a `⊕` beside each definition to add it to the page. Escape closes
it. A pin keeps it open while you work, and the canvas narrows instead of being covered. It replaces V1's source
tabs.

### 4.7 Drafts, publishing, and unpublished changes

- A **new board** is a draft: it lives only on this machine, under the project's `.unfold/local/drafts/`.
- **Publish** writes it to `.unfold/boards/<name>.json`, where it is committed like code.
- **Edits to a published board** are kept as **unpublished changes** on this machine until you press **Publish**
  again, so teammates never see half-finished edits in `git diff`. **Discard changes** returns to the published
  version. (In V1, edits to a published board save straight into its file. See
  [R-6](#r-6-edits-to-a-published-board).)
- **Move back to drafts**, **Rename**, **Move to folder**, **Duplicate**, and **Delete** are in the board menu.
- **Drafts** in the sidebar lists the drafts and unpublished changes across every project.

### 4.8 Boards that span projects

A page can hold cards from any connected project, for example `POST /api/cart/checkout` in `storefront-web` next to
`POST /orders` in `storefront-api`. The board lives in the project it was created in, and its file names the other
projects. A teammate who hasn't connected one of them sees those cards as placeholders ("From storefront-web, not
connected · Connect").

The graph does not link code across repositories (a `fetch('/api/orders')` in the web app is not an edge to the API
route). So a link between cards from two projects is **drawn by you**: a dashed, signed arrow with a label such as
"calls over HTTP". Detecting these links automatically is a later step. See [R-7](#r-7-boards-that-span-projects).

### 4.9 Several windows and tabs

- The top of every window is a tab strip, like Paper's: Home pinned first, then open boards. Each tab shows the
  board's name and its project's monogram.
- **New Window** (⇧⌘N) opens another window on the same workspace. **Open in New Window** on a tab moves that board,
  with its undo history, into a new window.
- A board is open in one tab of one window at a time (as in
  [U-37](../decisions-and-tradeoffs.md#u-37-several-windows-per-folder-one-window-per-board)). Opening it elsewhere
  focuses the tab that has it.
- Every window shows the whole workspace. Windows, their tabs, and their positions are restored on launch.

### 4.10 Background updates, health, and the engine

- Each project row in the sidebar carries **one** state indicator: nothing when the graph is current, a progress ring
  while compiling or updating, an amber dot for problems (with the count in its tooltip), a pause glyph when
  background updates are paused, a hollow circle when the project hasn't been compiled, and a red triangle when the
  last compile failed. A resting project ([R-8](#r-8-which-projects-stay-live)) shows its last known state with a
  dimmed name. Home shows the same state in words. Artboard v2 · 07 lists every state.
- **Health** on a card: a banner on the card ("Changed since it was described · Review"), a mark on its row under
  "On this page", and a mark on the page.
- **The engine restarting** shows a toast ("Reconnecting to the engine…"). Boards stay open and unsaved edits are saved
  after it reconnects.

### 4.11 Settings

- **Project settings** (project menu, or Settings on Home) open as a sheet with four sections: **General** (name,
  path, background updates), **Scope** (a check-box tree of the project's folders, with ignored folders shown and
  the reason given), **Detectors** (one switch per detector, grouped by language), and **Python projects**. It writes
  `.unfold/config.toml` as V1 does: staged, all or nothing, through Rust, keeping comments and formatting.
- **Entrances** that the detectors can't see are added with **Add entrance…** in the project menu, on an application
  on Home, or from a function's menu in Add or on a card. The editor is V1's, restyled.
- **Preferences** (⌘,, or the gear at the bottom of the sidebar) are app-wide and unchanged: theme, editor command,
  background defaults, thread limit, update checks, and key bindings.

## 5. Screens

The `v2` artboards in Paper, and what each shows:

| Artboard | Shows |
| --- | --- |
| v2 · 00 · Where everything goes | The old regions and every V1 command, and where each goes |
| v2 · 01 · Home — project | A compiled project's Home, with many entrances, boards, and health |
| v2 · 02 · Board — canvas | A board open in a tab: the board sidebar, the top bar, cards, the dock |
| v2 · 03 · Add — map | Add before typing: every project's entrances as a map |
| v2 · 04 · Add — search | Add while typing, across projects, with the preview |
| v2 · 04b · Add — commands | Add in command mode (`>`), which replaces the command palette |
| v2 · 05 · First launch | An empty workspace |
| v2 · 06 · New project — first Compile | A project that hasn't been compiled, with the scope tree |
| v2 · 07 · Project states | Every project-row state, Home's state line, toasts |
| v2 · 08 · Project settings | The settings sheet: General, Scope, Detectors, Python projects |
| v2 · 09 · Source peek | A whole file in the peek sheet over a board |
| v2 · 10 · Board across projects | Cards from two projects, a drawn link, and a placeholder |
| v2 · 11 · Health and activity | Home's expanded health list and the index log |
| v2 · 12 · Publish | The Publish dialog and the board menu, with unpublished changes |
| v2 · 13 · Board — dark | The board canvas in the dark theme |
| v2 · 14 · Model and decisions | The model, the principles, and the decisions still open |

## 6. Where everything goes

Every V1 region and command, and where it goes. Nothing is dropped without a new home.

| V1 | Goes to |
| --- | --- |
| **Title bar** folder switcher (Recent, Open Folder, Open Folder in New Window, New Window, Folder Settings, Close Folder, Preferences) | Sidebar: projects list, **Add project**, the project menu, and the gear. Recent folders become **Recents** (recent boards) in the sidebar |
| Title bar board title (rename) | Sidebar board header: double-click to rename |
| Title bar Code \| Flow | Canvas top bar, centered |
| Title bar Compile | First Compile on Home; then the project menu (Compile again ⇧⌘B, Rebuild from scratch, Cancel) |
| Title bar Publish | Canvas top bar |
| Title bar ◨ Toggle Inspector | Removed: the Selected section is in the sidebar. ⌘B toggles the sidebar |
| Board menu `⋯` (Rename, Export Mermaid, Copy Mermaid, Fold All, Unfold All, Move to New Window, Move Back to Drafts, Delete) | Board menu in the sidebar header. Fold All and Unfold All also in the canvas context menu. Mermaid under **Share** |
| **Activity bar** | Removed |
| **Files view** (disk tree, marks, context menu) | Finding a file: Add → Files. Marks: in Add's results. Scope actions (Exclude, Include, Graph only this folder): **Project settings → Scope**. Open in editor and Reveal in Finder: the peek sheet's menu and the project menu. Show callers: card menu |
| **Entrances view** | Home → How it starts, and Add's map. Add Entrance: project menu and application menu on Home. Edit and Remove a manual entrance: its menu on Home |
| **Search view** (All, Definitions, Entrances, Files, Text; match case, whole word, regex) | Add, with the same tabs and the same text options |
| **Boards view** (published, drafts, outline, Flow-label stepper, groups, health and repairs) | Boards: Home's Boards section and the sidebar tree. Outline and groups: **On this page**. Flow-label stepper: "Label them" under On this page. Health and repairs: the card banner and Home's Health |
| **Editor tabs** for boards | Window tabs |
| **Source tabs** | The peek sheet |
| Board bar (Back, trail, zoom, Fit, Arrange) | The dock, with Forward added |
| Canvas and card context menus | Unchanged, restyled |
| Banners (outside edit, failed save) | Unchanged, at the top of the canvas |
| **Inspector** (description, Calls, Called by, Reached from, Groups, Flow node label and paths) | The sidebar's Selected section, and editing on the card |
| **Bottom panel: Problems** | Home → Health, and Health and activity |
| **Bottom panel: Index log** | Health and activity |
| **Bottom panel: References** (callers as a tree, entrances that reach it) | Selected → Called by and Reached from; **Show callers** on a card adds the callers' cards to its left |
| **Status bar** (graph state, notes, connection, problem count, zoom) | The project row and Home's state line; "using fallback location" and "watching by scanning" in Home's state line tooltip and Project settings → General; the connection as a toast; zoom in the dock |
| Status menu (Compile, Rebuild, Cancel, Pause or Resume, Show index log, Copy Diagnostics) | The project menu; Copy Diagnostics also in the Help menu and Add's commands |
| **Welcome screen** (Open Folder, recent folders, drop a folder) | First launch Home, Add project, and dropping a folder on the window |
| **First-Compile screen** (counts, skipped, Choose folders, Compile) | The project's Home in the not-compiled state, with the scope tree |
| Close Folder | **Remove from workspace** in the project menu (asks first; it never deletes `.unfold/`) |
| Folder Settings dialog | Project settings sheet |
| Manual entrance editor | Unchanged, restyled |
| Preferences | Unchanged, restyled; reached from the sidebar gear and ⌘, |
| **Command palette** (⇧⌘P), Go to File (⌘P), Go to Symbol (⌘T) | Add: `>` for commands; ⌘P opens it on Files, ⌘T on Functions |
| Copy Diagnostics | Help menu, project menu, Add's commands |
| Several windows (New Window, Move to New Window, restore) | Same, per workspace instead of per folder |
| Application menu | Kept, with File → Add Project…, Remove Project; View → Home, Toggle Sidebar; Go → Add |

## 7. Visual system

The redesign keeps V1's design tokens (`ui/src/theme/`) and the reader's colors for cards, Code, and Flow, and
tightens the chrome:

- **Ground and type.** White ground, Inter for the interface and JetBrains Mono for code, both bundled. Interface text
  is 13px with 12px for secondary text; 11px only for all-caps section labels and code tags. Headings: 32px bold with
  tight tracking for a project name, 15px semibold for section titles.
- **One accent.** The reader's indigo (`#5862C0`, dark `#A0AAFF`) for selection, the primary button, links, and
  focus. Nothing else in the chrome is colored unless it means something.
- **Meaning only.** Amber for "needs review" and warnings, red for failures, green only for the "current" dot. HTTP
  methods and entrance kinds are shown in mono at the muted ink color, with DELETE in red as the only exception. The
  Flow node colors stay inside cards.
- **Neutral project monograms.** A project shows a gray rounded square with its initial and no hue, so it never
  clashes with the reader's colors. The monogram appears on project rows, tabs, Add's results, and cross-project
  cards.
- **Contrast floor.** No text lighter than `--color-subtle` (`#64748B`) on white. `--color-faint` (`#94A3B8`) is
  for rules, icons, and disabled states only.
- **Surfaces.** The sidebar is `#F7F8FA`, the canvas `#F7F8FA` with white cards, and hairlines are `#E2E8F0`. Shadows
  are used only for floating things: the dock, dialogs, sheets, and toasts.
- **Dark theme.** Every chrome token has a dark value, from the reader's existing dark set (`--color-reader-dark-*`).
  The dark theme is designed, not inverted (artboard v2 · 13).
- **Density.** Rows are 28px, sidebar sections are separated by 16px, the sidebar is 264px wide (resizable from 220
  to 360px), and the top bar is 48px.

## 8. Keyboard

| Action | macOS |
| --- | --- |
| Add (find and add code) | `A`, ⌘K, or `/` |
| Commands | ⇧⌘P (Add with `>`) |
| Go to File, Go to Function | ⌘P, ⌘T |
| Home | ⌘0 |
| Next tab, previous tab | ⌃Tab, ⌃⇧Tab (also ⌘⇧], ⌘⇧[) |
| Close tab | ⌘W |
| New board | ⌘N |
| New page | ⌥⌘N |
| New Window | ⇧⌘N |
| Add project | ⌘O |
| Compile the selected project | ⇧⌘B |
| Code / Flow lens | ⌘1 / ⌘2 (V1 used ⌘1–⌘4 for sidebar views, which are gone) |
| Toggle sidebar | ⌘B |
| Back, Forward | ⌘[, ⌘] |
| Fit, zoom in, zoom out | ⇧1, ⌘=, ⌘- |
| Arrange by call depth | ⇧A |
| Undo, Redo | ⌘Z, ⇧⌘Z |
| Preferences | ⌘, |
| Publish | ⇧⌘E |

The rules of V1's dispatcher are kept: plain keys never fire while typing, no window shortcut fires while a dialog is
open, and custom key bindings are stored by action id. The action ids are the new design's; V1's ids for removed
actions (the four sidebar views, Toggle Inspector, Toggle Panel) are deleted, not carried over.

## 9. Decisions

Each decision has a recommendation. **Needs your call** marks the ones that change what gets built and that only
you can make. The others are proposed defaults that the designs already follow; they need a yes, or a change.

| ID | Decision | Recommendation | Status |
| --- | --- | --- | --- |
| [R-1](#r-1-what-a-story-is-called) | What a story is called | **Board**, holding pages | Needs your call |
| [R-2](#r-2-one-window-many-projects) | How a window relates to projects | A window shows the whole workspace; commands name their project | Needs your call |
| [R-3](#r-3-what-a-project-is) | What a project is, and its shared name | A folder with its own `.unfold/`; `[project] name` in `config.toml` | Proposed |
| [R-4](#r-4-branches-and-worktrees) | Branches and worktrees | Read `.git` files, never run Git; one project row with a worktree switcher | Needs your call |
| [R-5](#r-5-the-board-format) | The board format | `unfold-board` with pages, a folder, and project-qualified refs; replaces the V1 format, no migration | Proposed |
| [R-6](#r-6-edits-to-a-published-board) | Edits to a published board | Kept as unpublished changes until Publish | Needs your call |
| [R-7](#r-7-boards-that-span-projects) | Boards that span projects | Allowed, with drawn links; automatic links later | Needs your call |
| [R-8](#r-8-which-projects-stay-live) | Which projects are watched | Projects with an open tab or selected on Home; the rest rest and catch up when opened | Proposed |
| [R-9](#r-9-where-the-workspace-is-stored) | Where workspace state is stored | App data `workspace.json` and `windows.json`; per-project state stays in `.unfold/local/` | Proposed |
| [R-10](#r-10-folders) | Folders | A `folder` field on the board; empty folders kept in `.unfold/local/state.json` | Proposed |
| [R-11](#r-11-the-selected-card-and-the-sidebar) | Where the selected card's details go | The sidebar's Selected section; no right inspector | Proposed |
| [R-12](#r-12-whole-source-files) | Whole source files | A peek sheet that can be pinned | Proposed |
| [R-13](#r-13-one-add-for-finding-and-commands) | Finding and commands | One Add box, with `>` for commands | Proposed |
| [R-14](#r-14-drafts-across-projects) | Drafts | Stored per project, listed together in the sidebar | Proposed |
| [R-15](#r-15-the-overlay-rule) | Can anything cover the canvas | Floating dock and lens; sheets and dialogs only while open | Proposed |
| [R-16](#r-16-revises-u-26-u-16-u-29-u-30-u-37) | Which V1 decisions change | Revise U-26, U-16, U-29, U-30, U-37 in place | Proposed |

### R-1: What a story is called

**Question.** The first direction named the story a **file**, as Paper does. In a code reader, "file" already means
source files: Home shows "1,284 files" beside a "Files" section and a "New file" button.

| Option | For | Against |
| --- | --- | --- |
| **Board, holding pages (recommended)** | Already the word in the docs, the format (`unfold-board`), and the code. No clash with source files | Less like Paper |
| File, holding pages | Exactly like Paper | Clashes with source files on every screen, in Add's "Files" tab, and in the docs |
| Trace, Story, or Map | A fresh word that says what it's for | A rename of the format, the commands, and the docs, and a word nobody knows yet |

### R-2: One window, many projects

**Question.** Today every command runs "against the folder that window shows" ([IPC](../ipc.md)). Home, Add, and
boards that span projects need one window to talk to several projects.

**Recommendation.** A window attaches to the **workspace**. Every command that reads a project takes a `project` id
(the engine host's workspace key), and events are tagged with their project and sent to every window that shows
the workspace. Path confinement stays per project: a path is relative to the project named in the command. This
changes the Rust command table, the generated types, the router, and the events, but not the engine or the store.

| Option | For | Against |
| --- | --- | --- |
| **Commands name their project (recommended)** | One port per window, one contract. Search across projects is one fan-out in Rust | Every command's arguments change; all callers in the UI change |
| One port per project per window | Each command keeps its shape | Many ports per window; cross-project search is stitched together in the UI |
| Keep one project per window | No contract change | Not the design: no workspace Home, no cross-project Add or boards |

### R-3: What a project is

A project is a folder Unfold has connected, with its own `.unfold/`, exactly what V1 calls a folder. A subfolder of
a monorepo can be its own project, as in V1. It gets a **shared name** for cross-project boards: `[project] name`
in `.unfold/config.toml`, defaulting to the folder's name and written only when changed. The workspace also stores
its path and a local id.

### R-4: Branches and worktrees

**Question.** The design shows the branch and a worktree switcher. V1 promises it "never uses Git"
([U-16](../decisions-and-tradeoffs.md#u-16-discovery-and-scope-without-git)).

**Recommendation.** Read `.git/HEAD` and the worktree list as files, and never run Git. One project row per
repository, with a switcher of its worktrees. Each worktree is its own folder with its own `.unfold/`, so switching
changes the graph, the published boards (which follow the branch, as they should), and the drafts (which belong to
that worktree folder). The switcher says so: "Drafts and the graph are per worktree."

| Option | For | Against |
| --- | --- | --- |
| **One row, a worktree switcher (recommended)** | One row per repository; matches how people think about a repo | Drafts seem to disappear when you switch, so the switcher must say why |
| One project row per worktree | No hidden switching; each row keeps its own drafts | Many near-identical rows; boards look duplicated |
| Show the branch only, no worktrees | Smallest change | Doesn't match how you work with worktrees |

### R-5: The board format

An `unfold-board` file is one board with **pages**. Each page has an id, a title, and what a V1 board holds today
(cards, groups, labels, guards). The board adds `folder` (a `/`-separated path, optional), `projects` (the shared
names of the other projects its cards come from), and `links` (drawn links, [R-7](#r-7-boards-that-span-projects)).
A card's `ref` gains an optional `project`, absent for the board's own project. This format **replaces** V1's: there
is one supported `version`, no reader for the old shape, and no migration. The writer stays deterministic, and the
JSON Schema in `docs/schemas/unfold-board.schema.json` is rewritten.

### R-6: Edits to a published board

**Question.** In V1, once a board is published, every edit saves straight into `.unfold/boards/<name>.json`. The
design shows "Published · 2 unpublished changes".

| Option | For | Against |
| --- | --- | --- |
| **Unpublished changes until Publish (recommended)** | Teammates never see half-done edits in `git diff`. Matches Paper and the design. Discard changes is easy | A second copy per board in `.unfold/local/drafts/`, and a conflict to handle when the published file changes underneath (pull) |
| Save straight into the published file (V1) | No new state | Every keystroke-level edit shows in Git; the design's label is wrong |

With the recommendation, a published board with changes is edited as a local copy (`drafts/<id>.json` with
`"publishes": "boards/<name>.json"` and the revision it started from). Publish writes it over the published file.
If the published file changed since (a pull), Publish asks: keep mine, or take theirs and drop my changes.

### R-7: Boards that span projects

**Question.** Principle 5 lets a page hold cards from several projects. The graph has no edges between
repositories, so nothing would join the cards.

| Option | For | Against |
| --- | --- | --- |
| **Cards from any project, links drawn by people (recommended)** | Works now. A drawn link is signed and labeled like a description | A drawn link can go stale; health checks only that both ends still exist |
| Cards only, no links | Least work | The page shows two unrelated islands |
| Detect HTTP links automatically (client call → route) | The real promise of principle 5 | New engine work: string matching of URLs across projects, with false positives. A later step |
| One project per board | Simplest model and storage | Drops principle 5 |

### R-8: Which projects stay live

Connected projects that are watched all the time would break the memory budget ([architecture
§17](../architecture.md#17-performance-targets), 600 MB with one repository). A project is **live** (open workspace,
watcher, background updates) while a tab shows one of its boards, while it is selected on Home, or for five minutes
after. Other projects **rest**: their row shows the last known state, and opening them catches up (opening a
compiled folder already takes under a second). Add searches resting projects from their stored graph; a result
from a resting project wakes it.

### R-9: Where the workspace is stored

- `workspace.json` in app data: the projects (local id, path, display name, order, collapsed or expanded), the
  selected project, and recent boards.
- `windows.json` in app data: each window's position, size, tabs, and active tab, replacing the per-folder window
  list in `.unfold/local/state.json`.
- `.unfold/local/state.json` keeps what belongs to a project: background updates on or off, empty folders, and each
  board's view state (zoom, scroll, lens per page).

Written by Rust, as `preferences.json` is today; the UI still uses no browser storage.

### R-10: Folders

A board's `folder` field places it in a folder; the sidebar builds the tree from the boards. An empty folder, which
has no board to carry it, is kept in `.unfold/local/state.json` until a board goes into it. A published board's
folder is shared with the team; a draft's folder is local.

### R-11: The selected card and the sidebar

The inspector moves into the sidebar's **Selected** section, under the pages and the cards on the page. It is
collapsible and scrolls on its own, and descriptions and Flow labels can also be edited on the card. Paper puts
properties on the right, but a right panel costs the canvas width this redesign is for, and V1's closed-by-default
inspector went unused.

### R-12: Whole source files

A peek sheet, 640px wide, over the right of the canvas; pinned, it narrows the canvas instead. The alternative, a
tall card on the page, mixes reading a file with the story on the page and grows the board file.

### R-13: One Add for finding and commands

Add replaces the command palette, Go to File, Go to Symbol, the Search view, and the Entrances view. `>` switches to
commands, as in V1's palette. Text search keeps match case, whole word, and regular expressions, streams its
results, and is cancelled by a new query, as in V1.

### R-14: Drafts across projects

Drafts stay where V1 keeps them, in each project's `.unfold/local/drafts/`. The sidebar's **Drafts** lists them
across every project, with each draft's project monogram, so it works like Paper's Drafts without moving any files.

### R-15: The overlay rule

V1's rule was "nothing overlays the canvas". The redesign changes it to: **the dock and the lens float at the
canvas's edges and never cover a card after Fit; sheets and dialogs cover it only while they are open, and Escape
closes them.**

### R-16: Revises U-26, U-16, U-29, U-30, U-37

These are revised in place, since nothing has shipped.

- **U-26** (an IDE-shaped window) becomes the studio window of this design.
- **U-16** (discovery without Git): Unfold reads Git's files to show the branch and worktrees; it still never runs
  Git, and discovery is unchanged.
- **U-29** (inspector and board outline): the outline is "On this page", the inspector is "Selected".
- **U-30** (search): search is Add, across projects.
- **U-37** (several windows per folder): several windows per **workspace**, still one window per board.

## 10. What gets rebuilt

### Stays as it is

The engine crates (compilers, linking, resolution, detectors), the graph store, compiling and background updates,
the reader's Code and Flow render models, the card components, the canvas (React Flow), layout and arrows, the
entrance editor's logic, Preferences, the engine host's process model and restart, window hardening, packaging, and
updates.

### Rust (`unfold-workspace`, `unfold-reader`)

- **Commands take a project** (R-2), and events carry it. New commands: the workspace (list, add, remove, rename,
  reorder projects; recent boards), a project summary for Home (state, counts, applications with entrance counts,
  health summary), Git info (branch, worktrees), board thumbnails (card rectangles per page), cross-project search,
  and Show callers.
- **The board format** (R-5): pages, folders, `projects`, drawn `links`, health for
  cross-project refs and links, and the new schema.
- **Unpublished changes** (R-6): the local copy, Publish over the published file, Discard, and the conflict check.
- **Live and resting projects** (R-8): opening and closing a project's workspace on demand.
- `workspace.json`, `windows.json`, and `[project] name` in `config.toml`.

### Engine host and main process (`desktop/`)

- The router attaches windows to the workspace instead of a folder, and routes events by project.
- Windows: restore from `windows.json`; New Window and Open in New Window per workspace; dropping a folder adds a
  project.
- The application menu: Add Project, Remove Project, Home, Add; the removed views and toggles go.
- Dialogs: Add Project (folder picker), Remove Project confirmation.

### UI (`ui/src/`)

| Area | Change |
| --- | --- |
| `shell/` | Rebuilt: window tabs, the sidebar's two modes, the canvas top bar, the dock, toasts. The activity bar, title bar, status bar, panes, and workbench go |
| `home/` (new) | Home: the header, How it starts, Boards with thumbnails, Health and activity, first launch, not compiled, compiling |
| `add/` (new) | Add: the map, search across projects with tabs and preview, commands. Replaces `palette/` and the Search and Entrances views |
| `peek/` (new) | The source peek sheet, from V1's source tab renderer |
| `settings/` | Project settings becomes a sheet with the scope tree; the entrance editor is restyled |
| `views/`, `panel/`, `inspector/`, `editor/`, `welcome/` | Removed or folded into the above. The inspector's sections are reused in the Selected section |
| `canvas/`, `cards/`, `code/`, `flow/` | Kept: restyled, plus drawn links, cross-project placeholders, and Show callers |
| `state/` | Per-tab board state; the workspace and the selected project; TanStack Query keys include the project |
| `keyboard/` | The new map (§8); old action ids dropped |
| `theme/` | The chrome tokens of §7, light and dark |

### Docs and tests

- U-38 onwards and the amendments of R-16 in the decision log; architecture §11 rewritten; UI shell, IPC, reader,
  user guide, and the board schema updated.
- The Playwright journeys in `e2e/` are rewritten for the new window, and the budgets measured again, including
  memory with three projects connected.
- Unit tests for the shell move with it; the canvas, cards, Code, and Flow tests are kept.

## 11. Build order

Each step ends with the workspace building and its tests, clippy, and lint passing. The old shell doesn't have to
keep working between steps: it is deleted as the new one replaces it.

1. **Decisions.** Record §9's outcomes in the decision log (revising U-16, U-26, U-29, U-30, U-37 in place).
2. **Engine and IPC.** Commands take a project; the workspace and its files; live and resting projects; the router.
   This is the riskiest step and goes first.
3. **The board format.** Pages, folders, unpublished changes, the schema. The V1 format's code is replaced, not kept.
4. **The shell.** Window tabs, the sidebar's two modes, the canvas top bar, the dock, the visual tokens, the dark
   theme. The board canvas works inside it.
5. **Home.** Every state: first launch, not compiled, compiling, current, problems, failed, paused.
6. **Add.** The map, search across projects, preview, commands. The palette and the old views are deleted.
7. **Peek sheet, project settings, entrance editor, Publish and unpublished changes.**
8. **Boards across projects.** Cross-project cards, placeholders, drawn links.
9. **Windows and restore, keyboard, the application menu.**
10. **Tests and budgets.** The e2e journeys, the budgets, the packaged smoke test; the reference docs.

## 12. Before implementation starts

These were the open questions. Each takes its §9 recommendation unless the build's brief overrides it, because each
one changes the contract an implementation session builds against:

1. **R-1** What a story is called.
2. **R-2** One window shows the whole workspace, and commands name their project.
3. **R-6** Unpublished changes on published boards.
4. **R-7** Whether the first release has cross-project boards, and with drawn links.
5. **R-4** Worktrees: one row with a switcher, or a row each.
6. A yes, or changes, to the proposed defaults (R-3, R-5, R-8 to R-16) and to the designs in Paper.
7. **Scope of the first release:** all of §11 at once, or steps 2–7 first, with cross-project boards (step 8) after.
