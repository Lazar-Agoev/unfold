# UI shell

This document describes the renderer as it is built (`ui/src/`): how its modules are laid out, the state each
window keeps, how it talks to the engine and keeps its caches current, the keyboard, the theme, the shared
components, and the layers that can cover the canvas. What the window offers, part by part, is
[architecture §11](architecture.md#11-the-desktop-app); what cards, Code, Flow, and boards show is
[the reader](reader.md); the commands the window sends are [IPC](ipc.md); and the Electron side (processes, the
application menu, window restore, packaging) is [desktop/README.md](../desktop/README.md).

## Modules

```text
ui/src/
  main.tsx       boot: the query client, the theme, <App/>
  app/           the window frame: tab strip, sidebar container, content area, window-level layers,
                 wiring, the frame's actions, folder drops
  home/          Home: its sidebar, first launch, not compiled, the overview, boards and thumbnails,
                 health and activity, the compiled toast
  board/         a board tab: its sidebar (header, Pages, On this page, Selected), the top bar, the dock
  canvas/        the React Flow canvas: cards (block, entrance, placeholder), arrows, drawn links,
                 layout and sizing, Arrange, Show callers
  code/ flow/    the Code and Flow renderers
  add/           Add: the map, search across projects, the preview, commands
  agent/         agents apart from any one board: the agent mark and bylines, Copy link, view reports
  peek/          the source peek sheet
  settings/      project settings, the entrance editor, Copy Diagnostics
  preferences/   Preferences, with the keyboard section
  dialogs/       one host for every dialog: Publish, Discard changes, Delete, folder prompts
  menus/         the board, share, and project menus, project actions, the worktree switcher
  ipc/           the typed client, queries and invalidation, streams; generated/ (never edited)
  keyboard/      the action registry, keymap, dispatcher, bindings, command scopes, menu commands
  state/         per-window stores: workspace, window, tabs, layout, overlays, compile, author,
                 session, actions, and boards/ (open board sessions, and agents' edits of them in
                 boards/agent/)
  components/    the shared kit
  icons/         the 16px icon set and the project-state glyphs
  lib/           pure helpers: formatting, entrance and file text
  theme/         tokens.css, base.css, theme.ts (bundled fonts)
  test/          test helpers: a fake engine, rendering, a workspace fixture
```

Screens (`home`, `board`, `add`, `peek`, `settings`, `preferences`, `dialogs`, `menus`) import from `state/`,
`ipc/`, `components/`, `icons/`, `lib/`, and `keyboard/`, and from each other's `index.ts` only. Nothing imports
`app/`. `state/` imports no screen; it may use `canvas/`'s layout and card-data helpers. `canvas/`, `code/`, and
`flow/` render the reader's models ([the reader](reader.md)).

## The frame

```text
App
 ├ TabStrip                  Home, then one tab per open board (its project's monogram and title)
 ├ Body
 │   Sidebar                 Home mode → HomeSidebar          board tab → BoardSidebar(board)
 │   #content-area           Home → HomeMain                  board tab → BoardView(board)
 │                           PeekSheet (the last child, over the right of the content area)
 ├ AddLayer · DialogHost · Toaster
```

- **Tabs.** Home is implicit, always first, and never closes. A board tab exists exactly while its board is open in
  this window: closing it saves the board's edits and view, then `close_board`. A tab is as wide as its title up to
  220px, shrinking to 96px; past that the strip scrolls with the active tab kept in view. `×` sits in a fixed slot
  on the active and hovered tab; middle-click closes. A tab's menu has Close, Close other tabs, and Open in new
  window. ⌘W on Home closes the window only when Home is the only tab. At most 100 board tabs.
- **One mounted board.** Only the active board tab is mounted; its camera, card sizes, and selection live in the
  store, so switching back restores it without measuring again.
- **The sidebar** is 264px by default, resizable from 220 to 360px from its right edge, and ⌘B hides it. Home mode
  shows the workspace row (with Add's search), Recents, Drafts, and the projects; board mode shows the active board.
  A project row's click selects the project and shows its Home; its chevron only expands it; `⋯` (on hover and
  while selected) opens the project menu. In a project's tree, folders come first, then boards, each in natural
  order; ↑ ↓ move between rows and → ← expand and collapse. A folder's menu has New Board Here, New Folder…,
  Rename…, and Delete (only when empty); boards move between folders with the board menu's Move to folder. Home's
  sidebar ends with Add project and the Preferences gear; a board's sidebar has no footer.
- **The board sidebar** has the board header (back to Home, the project and folder, the title, which renames in
  place on double-click, and the board menu), **Pages** (each page with a review mark; a page's menu has Rename,
  Duplicate page, Copy link, Move up, Move down, and Delete page…), **On this page** (the cards in follow order indented by
  call depth, their groups, and drawn links after their `from` card; a project-initial column only when the page
  holds cards from more than one project; "N Flow steps unlabeled · Label them"), and **Selected**, which takes at
  most half the sidebar's height, scrolls, and collapses.
- **Selected** follows the page's selection. A card: its name, location, description, Called by and Reached from (up
  to three each, then "N more · Show callers"), the calls not on the page (up to five, each with **+ Open**), and its
  groups; an entrance card lists its handlers. A Flow node: its label, the code name it replaces, and its paths. A
  group ("Step group in" its card): its title, description, and Ungroup. A drawn link: its label, Edit label, and
  Remove link. Bylines show the author only, because authored text carries no time; an agent's read "Claude · agent"
  with the agent mark, and a group shows who titled it ([Agents in the window](#agents-in-the-window)).
- **The canvas** has the top bar (the page title and publish state, "Claude is working…" while an agent works on the
  page, the lens centered, Share and Publish) and the dock, both floating ([Overlays](#overlays)). Its context menu has Fit, Fold all cards, and Unfold all cards; a
  card's has Open source, Show callers, Draw link…, Add as entrance… (for blocks that can be handlers), Copy link,
  and Remove from page; a group's has Collapse or Expand group, Describe group…, Copy link, and Ungroup; a link's has
  Edit label, Copy link, and Remove link. A card that needs review shows a banner whose **Review** opens a popover of its problems, each repair with its
  effect and Apply (saved at once, undoable).
- **Groups in both views.** In Code a group is the magenta rail; in Flow it is a step group, a titled magenta region
  around its steps or, collapsed, one step ([reader: step groups](reader.md#step-groups)). Either view's chevron sets
  the group's one `collapsed` field (one undo step), so it folds in both. In Flow, the region's title or the collapsed
  step selects the group, and right-click (or Shift+F10 on the chevron) opens its menu.

## State

Each window is its own renderer with its own stores. Nothing is shared between windows except through Rust.

| Store | Kind | Holds | Saved to |
| --- | --- | --- | --- |
| `workspace` query | TanStack Query | The `WorkspaceView`: project rows, recent boards, folders opened before | Rust (`workspace.json`) |
| `useWindowStore` (`state/window.ts`) | Zustand | The window's id, key, and platform; its Home selection | `windows.json` |
| `useTabs` (`state/tabs.ts`) | Zustand with Immer | The board tabs in order and the active tab | `windows.json` |
| `useLayout` (`state/layout.ts`) | Zustand with Immer | Sidebar width and visibility, the Selected collapse, the peek pin, Home's view and its board filter and sort, expanded sidebar folders | `windows.json` (`layout`) |
| `state/overlays.ts` | Zustand | The open Add request, the peek target, the open dialog | Not saved |
| `state/boards/` | Zustand with Immer | Open board sessions | `save_board` (the document), `save_board_view` (the view) |
| `state/compile.ts` | Zustand | Each project's running Compile and its progress | Not saved |
| `state/boards/agent/working.ts` | Zustand | Agents' working marks: per board and agent, the items changed on each page | Not saved |
| Everything else from Rust | TanStack Query | [Data from the engine](#data-from-the-engine) | — |

- **Projects and names.** `state/workspace.ts` indexes the rows by id and by shared name (the first row in
  workspace order wins a name). A card's project is resolved against the **current** index on every render: absent
  `ref.project` means the board's own project, an unknown name means a placeholder. Connecting, renaming, or
  removing a project re-resolves every card at once, without reloading the board. Adding a card from another
  project writes that project's shared name into its `ref`; a project whose name equals the board's own project's
  can't be added ("Rename one of the projects in its settings").
- **The Home selection** is per window. A window starts on its saved selection, else the workspace's default, else
  the first project. It never changes because a board tab is active. `select_project` keeps the project live.
- **Board sessions.** An open board holds its working document, the revision it was loaded from, its publish state,
  health, and outlines, its undo history, its active page, and a view per page (lens, camera, selection, folds,
  inline expansions, the follow trail with Back and Forward, measured card sizes, pending focus or link
  drawing, and the types opened in a selected line's contract).
  - Every edit is one undo step of the board, whichever page it changed; bursts with the same coalescing key within
    1.2 s merge; at most 200 steps; undo and redo show the page they change. Reloads, discards, and repairs are
    undo steps too. An agent's edit is one step named for the agent and the tool ("Claude · Add cards"), saved at
    once ([Agents in the window](#agents-in-the-window)).
  - The board saves itself 600 ms after the last edit, never with two saves in flight. `save_board` sends `{title,
    pages}` only: Rust owns the folder, the other projects, and the file's framing. A save keeps the identities of
    untouched cards, so they don't render again.
  - A conflict (`board_conflict`) with no unsaved edits reloads; with unsaved edits it shows a banner, **Keep mine**
    (`save_board` with `force`) or **Take theirs** (`reload_board`), and stops autosave until it is resolved. A
    failed save shows a banner with **Retry**.
  - After `graph_updated` of any project the board's cards come from, its health is fetched again (250 ms later,
    once per board).
  - View changes (camera, lens, page) are never undo steps; they are saved with `save_board_view` a second after
    they stop. The lens is the active page's, shown as the lens or else the page's default view; a card's own switch
    overrides it.
  - Publish and Move back to drafts change a board's id; the session and its tab follow the new id.
- **Moving a board to a new window.** Open in new window saves what it can, opens a window
  (`move_tab_to_new_window`), moves the claim (`move_board`), and hands the session (the document, the undo history,
  the active page, and each page's camera and lens) to the new window (`transfer_board_state`), which adopts it once
  it has restored itself.
- **Compile.** `startCompile` streams `compile` (one per project per window); Cancel cancels the stream and sends
  `cancel_compile`. Progress comes from the stream or from `compile_progress`, and clears when the project's state
  leaves compiling or updating.
- **The session** (`state/session.ts`):
  - **Restore**, at launch or reload: `window_state {key}`, then the layout, the Home selection, the board tabs
    reopened on their pages (a board that can't be opened, or is held elsewhere, loses its tab), and the active tab.
    Only then are boards moved here from another window adopted, and agents' requests answered.
  - **Save:** `save_window_state` 400 ms after the selection, the tabs, a board's active page, or the layout
    changes, and at once when the page goes away (`pagehide` also sends every dirty board and pending view).
  - **Engine restart:** a toast ("Reconnecting to the engine…") while the engine host is away. Once it is back:
    `select_project` again, `open_board` for every board tab in order (a board held elsewhere loses its tab, with a
    toast; a changed revision goes through the conflict rule), then every dirty board and pending view is saved, the
    window's state saved, and every engine query reset except preferences and app info.
  - `focus_board` shows the tab of a board another window asked for.

## Data from the engine

`ipc/` is the window's only way to the engine: `call`, `callStream`, and `onEngineEvent` (typed by the generated
module), `windowCommand` and `onShellEvent` (typed by `desktop/src/shared/unfold-api.ts`), `addDroppedFolder`, and
`useConnectionState`. Errors are `UnfoldError`s with Rust's or the transport's code.

- **Keys.** Every engine read is a query keyed `["engine", command, args]`. Every project command's args carry
  `project`, so each event touches only the queries of its project (and the cross-project `search`, `drafts`, and
  `board_refs` that include it).
- **Defaults.** `staleTime: Infinity`, no retry, no refetch on focus: data changes only when an event says so. The
  `workspace` query also refetches on window focus (a branch checkout changes `.git/HEAD`, which nothing watches),
  and `git_info` is fetched fresh each time the worktree switcher opens.
- **Writes** are plain calls from `state/`. Their effects arrive as events, or are adopted from the result
  (`save_board` into the board store, `update_project_config` into its query).
- **Streams.** `createLatestStream` runs one request at a time: each run cancels the one before, and late messages
  are dropped. Add runs `search_text` this way on every query change, and runs it again 300 ms after a
  `graph_updated` or `files_changed` while it is open, because events don't invalidate streams.

**Invalidation** (`installEngineInvalidation`, `ipc/queries.ts`; P is the event's project):

| Event | Effect |
| --- | --- |
| `workspace_changed` | Replaces the `workspace` data (no refetch) |
| `project_state` | Patches the row's state in `workspace`; invalidates P's `project_summary` |
| `project_reset` | Removes every query of P; forgets P's remembered card renders |
| `graph_updated` with `everything` | Invalidates every query of P, `search` that includes P, and `workspace` |
| `graph_updated` | P's `file_view` by path; `card` by key or file, and every entrance card; `peek` by key or file; `relations`, `callers`, `preview`, `project_summary`, `index_log`, `board_thumbnails`; `line_labels`, `call_contract`, `type_definition`, `type_uses` (they read the callee's and the types' files too); `entrances` when entrances or inputs changed; `search` that includes P. The board store refreshes the health of open boards with cards from P |
| `files_changed` | P's `file_view` and `card` by path (all of them when truncated); `search` for files that includes P |
| `boards_changed` | P's `list_boards`, `project_summary`, and the `board_thumbnails` that name a listed board; `drafts`; every `board_refs` |
| `problems_changed` | P's `problems` and `project_summary` |
| `config_changed` | P's `project_config`, `compile_preview`, and `scope_dir` |
| `preferences_changed` | Replaces the `get_preferences` data; the window applies the theme and key bindings at once |
| `compile_progress`, `board_conflict` | Not the cache: the compile store and the board store |

No handler invalidates without a predicate. Invalidation refetches only the queries something shows; the others
refetch when next shown.

**Who reads what.** Lists use commands that don't wake a project (`list_boards`, `drafts`, `entrances`, `search`,
`board_thumbnails`, `board_refs`), and Home reads `project_summary` only for the selected project. Thumbnails are
fetched in chunks of 60 as their rows scroll into view and drawn as plain SVG with the canvas's fallback card sizes.
Every page and board shares one `card {project, key}` query per definition; the UI applies each page's Flow labels
itself, and keeps the last verified render of each card in memory.

## Screens

The screens follow [architecture §11](architecture.md#11-the-desktop-app). Details that matter when changing them:

- **Home** (`home/`) shows one of five views of the selected project, remembered per window: the overview, Health,
  Activity, Recents, or Drafts. How it starts lays out applications in rows of at most three; an application's "N
  more" expands it in place with a filter and every entrance (up to 500). Only manual entrances have a menu (Trace on
  a New Board, Edit…, Remove…). Board cards show a thumbnail, the title, and a subline with pages, publish state, and
  unpublished changes; the grid has All, Published, and Drafts and sorts by last edited or title. The not-compiled
  Home keeps scope changes in memory and writes them with `update_project_config` just before `compile`. "<project>
  compiled" is toasted only in the window that started the Compile, when its Home isn't showing that project.
- **Lines between cards** (`canvas/lines/`, [typed lines](proposals/typed-lines.md#7-ui)). Each call or
  passed-reference arrow carries its closed label in the edge-label layer: one `line_labels` query per project
  per page, in calls of at most 500 lines. `labelText.ts` writes it (three or more inputs fold to
  `(Cart, +2) → Receipt`; a parameter without a type is its name in gray; a missing output is `?`; unprovable
  inputs are `(…)`). A label never covers a card or another label: each reports its curve and the measured
  widths of its forms to the canvas's `LabelLayout` (`labelLayout.ts`), and `placeLabels.ts` (pure) puts it at
  the first free spot along its curve (the midpoint, then 0.4 and 0.6, …), then a little above or below, then
  hanging beside a point on it (right, then left, level or a little above or below), and only then in a shorter
  form (`(+2) → Gateway`, then `→ Gateway`). A label that fits nowhere isn't drawn at rest:
  its line takes the click itself (a wide invisible stroke, a focusable button named `Line <full label>`).
  Clicking a label selects the line (page selection kind `line`: the arrow, its `source` and `seq`, its two
  cards), and the label opens into the contract (`call_contract`): Sends, then Returns and Throws, caller on
  the left and callee on the right. It opens clear of the caller's calling line and the callee's marked lines
  where it can (`placeContract.ts`: along the line, then hanging below it), else centered where the label was;
  opening a type grows it downward, and widens it to fit the definition (316px to 480px). A `declared` type in
  it (`code/TypeText.tsx`) opens its definition under its row (`code/TypeDefinition.tsx`, `type_definition`),
  whose members' types line up in one column after the widest name (`readonly` a quiet prefix) and open
  nested up to four deep. The members it inherits follow in the same grid, under a quiet "from <Type>" for
  each base (`--color-muted`, UI type; a base with no declaration adds its module or reason after a `·`);
  "Peek source" opens the peek sheet at the declaration, and "Used in N places" a
  popover of `type_uses`, each opening the peek sheet at its line. Which types are open is page view state: never saved, never an undo
  step, and cleared when the selection leaves the line. Escape (`board.closeLine`, ahead of the peek sheet's)
  or a click on the canvas closes the contract. While a line is selected, an unfolded Code callee card tints
  the contract's `calleeLines` with `--color-step` and the caller's calling line keeps `--color-selection`;
  Selected shows the call as written and its two cards.
- **Health and activity** shows one list at a time. Health: the problems other than board health (Open shows the
  file in the peek sheet at its line; Settings opens project settings on the section the problem is about), then the
  boards that need review (Review opens the board on the page of its first item, with the card selected). Activity:
  every update, newest first, with Copy diagnostics.
- **Add** (`add/`) is a 1040 × 660 box over the window. Focus stays in its input: ↑ ↓ and Page Up/Down move the
  selection (← → too on the map), Tab and ⇧Tab switch the kind, Escape closes, and a click adds (⌘-click on a new
  page). Queries wait 120 ms; All shows up to four rows per kind and asks for 50 per kind, a kind's tab 500; text
  search starts at two characters; the preview waits 150 ms (a file's first 12 lines, a match with five lines around
  it). Before typing, the map shows 12 entrances per application, then "+N more"; hovering an entrance for 300 ms
  shows its preview and the first other board page it is on. In command mode, `>` lists every enabled action under
  its group, or, with a query, ranked by title prefix, word start, substring, every word, then letters in order.
  Functions lists every definition: a type declaration reads with its kind before its name (`interface Cart`,
  the kind in `--color-contour`) and `TYPE` in the kind column, and is added as a Code card.
- **The peek sheet** (`peek/`) loads the file in pages of 200 lines as they scroll into view. Definitions (functions,
  classes, methods, constructors, accessors, interfaces, type aliases, enums) get `⊕`, the on-page mark, and a fold
  chevron; folds start open. A type declaration's `⊕` adds it as a Code card. Its
  keys: Escape closes (`peek.close`, not while typing) and ⇧⌘O opens the file in your editor (`peek.openInEditor`).
- **Project settings** (`settings/`) is an 880 × 700 dialog over the window: General (the name, staged; background
  updates, applied at once; where the graph is stored), Scope (the tree loads one level at a time from `scope_dir`;
  folders excluded by a rule above them are locked; the rules list shows staged additions and struck-through staged
  removals), Detectors, and Python projects. Save sends every staged edit in one `update_project_config`; Rust's
  error, with its line and column, stays at the top of the section. When `.unfold/config.toml` doesn't parse, the
  sections show the error with Open in editor, and the name is read-only. Closing with staged edits asks first.
- **Publish** (`dialogs/`) has three forms: a draft's first Publish (the file name, editable when `name_taken`
  suggests another), a published board's changes (their count), and a conflict (keep mine, or take theirs and drop
  my changes). The conflict form shows before Publish when the board's `baseChanged` says so, else when
  `publish_board` answers `conflict`. Publish is disabled for a published board without changes. Discard changes and
  Delete ask first; Move back to drafts doesn't.
- **Menus** (`menus/`) share one style: 28px items, their shortcuts, separators, and submenus that open to the right.
  The project menu shows every item in every state, with the ones that don't apply disabled. The board menu works on
  boards this window doesn't hold too (Home and the sidebar right-click it): Share and folding are then disabled,
  Rename… opens the board first, and Open in new window opens it here first.
- **Toasts** sit at the bottom right, newest at the bottom, at most four; information hides after 5 s, errors after
  8 s, and the engine toast stays until the window has reconnected. While a dialog is open, the stack rises just
  above the dialog's footer when it would otherwise cover its buttons.

## Agents in the window

The main process routes an agent's requests to the window that holds the board, or opens the board in a
background tab first ([desktop/README.md](../desktop/README.md#the-agent-bridge)); the requests and answers are
typed in `desktop/src/shared/agent-api.ts`. The window owns placement, the document's rules, and the refusal texts;
the main process owns argument parsing, links, defaults, and page revisions.

- **Agent edits** (`state/boards/agent/`). For `agent_edit`, the window loads what the operations read (card
  models through the query cache, `relations` and `callers` for following calls), then applies them to the session's
  working document with the pure edits of `document.ts`, signing every new or changed text with the agent's identity
  (`agent:claude`). `saveStep` saves at once: the board's unsaved edits first, then the change, never two saves in
  flight. The change becomes one undo step, named "<agent> · <title>", only once the engine accepts it, so a refused
  edit leaves the board as it was and answers `{ok: false, code, message}` with the engine's code; edits the person
  made while it saved stay on top (the operations apply again over them, and they autosave). The answer is the page
  as saved, the board's title, publish state, and file revision, what each operation did (each item's id, name,
  and status: created, updated, kept, or removed), and warnings. Nothing applies when one operation is refused.
- **Operations.**

  | Operation | Does |
  | --- | --- |
  | `add_cards` | Adds a block or an entrance by key (from any connected project), or keeps the card already there (its view changes when asked). A new card goes beside the item it is `near`, else right of a card that calls it, level with the calling line, else left of a card it calls, else right of everything; it never covers a card. `follow` adds what it calls (up to 3 levels) and what calls it (up to 2), resolved calls and constructions only, at most 40 cards per card; each card's result lists where following stopped: an external or unresolved call (as written, with its line and library or reason), resolved calls past the depth asked for, or callers past the 20 listed |
  | `set_view` | A card's view (Diagram, Flow, or Code) and fold, a step group's collapse, and the page's default view |
  | `arrange` | Arrange by call depth, then moves: a position, or right of or below another card (moved clear of the rest) |
  | `remove_items` | Cards (with their step groups and the links that end at them), step groups, and links |
  | `group_steps` | Step groups of one card by line range or step keys, created or updated by id, under the gutter's rule (`groupSelection`): whole statements of one branch, never overlapping another group. The result names the statements each group took; a warning says when a range took more lines than asked; a refusal says why a range breaks the rule and lists the statements there |
  | `describe` | Descriptions of cards and step groups, and labels of links |
  | `label_steps` | Flow labels of a card's steps (keys of its Flow model) |

  Sizes come from what the canvas measured, else from the cards' models (`estimatedBlockSize`), and calling lines
  from the Code rows (`estimatedLineOffset`), so placement and Arrange work on a page no one has shown.
- **People's words are safe.** No operation replaces or removes a person's description, label, or group title or
  description unless the call sets `replacePeopleText` (`people_text` otherwise); a group title without an author
  counts as a person's. Text an agent writes again unchanged is kept, whoever wrote it.
- **Reads and background tabs.** `agent_read` answers with the page as the window holds it (unsaved edits included),
  the board's title, publish state, and file revision, and whether it has unsaved edits. `agent_open_board` opens a
  board with `openBoard(ref, {background: true})`: its tab is added without switching to it, a board open here keeps
  its page, and a window that holds the board is left alone (the answer names it). The channel is installed before
  the window renders, and requests wait until the window's own state is restored.
- **Working marks** (`state/boards/agent/working.ts`): while an agent works on a page, the top bar says "Claude is
  working…" (the agent glyph and muted text; "Claude and Codex are working…"), the cards it changed (a group's or a
  label's card too) have a soft outline (`--color-authored-line`, 2px, 3px off the card), and the links it labeled a
  soft band (`--color-authored-tint`). `agent_finish` clears that agent's marks on the board and toasts "Claude ·
  <summary>". Session state only: never saved, never an undo step, and dropped when the board leaves the window.
- **The agent mark** (`agent/AgentMark.tsx`): the `agent` glyph (a 10px spark) and the word "agent", in the color of
  the text around it. `authorName` reads an agent as "Codex · agent" in plain text; `<Author>` draws "Claude · agent"
  with the mark in Selected's bylines (descriptions, Flow labels, a link's "drawn by", a group's "titled by") and on a
  placeholder card. On the canvas, an agent's card description carries the mark (titled "Written by Claude, an
  agent"), and an agent's link label chip shows its name and the mark. Editing an agent's text makes it yours: the
  edit signs it with your name, and a group's new title is its writer's.
- **View reports** (`agent/viewReport.ts`): `report_view` 150 ms after the tabs or a board session change (only when
  the report differs from the last), at once when the window gains focus, and after a resize: the active board and
  page (none on Home), the selection (a card, a step: a card's Flow node or statement, a group, a link, or a line
  with its occurrence), the lens (the page's, else its default view), the ids in view (cards whose rectangle meets
  the canvas's visible area, all of them before the canvas has a camera; their groups; links with an end in view),
  and the board tabs.
- **Copy link** (pages, cards, step groups, and links): copies `unfold://<project's shared name>/<board id>?page=<page
  id>&item=<item id>` (each part URI-encoded) with `copy_to_clipboard`, and toasts "Copied the link". Only the main
  process reads links; clicking one opens nothing.

## Overlays

The overlay rule is [architecture §11.3](architecture.md#113-the-window). How it is built:

| Layer | Placement | Modal |
| --- | --- | --- |
| The dock and the top bar's lens | Float at the canvas's edges, centered on the uncovered canvas | No |
| Banners (conflict, failed save, drawing a link) | The top of the canvas | No |
| The peek sheet | Slides over the right of `#content-area`, 640px wide; pinned, it is in the flow and the content narrows | No |
| Add, project settings, Preferences | Centered on the window, over a scrim on the whole window | Yes |
| Board dialogs (Publish, Discard changes, Delete, Delete page, folder prompts) | Centered on the canvas, over a scrim on the canvas only | Yes |
| Menus, popovers, tooltips, toasts | Above everything, in that order | No |

- While the sheet floats unpinned, the lens, the dock, the banners, and the empty-page hint center on the canvas left
  of it (`--covered-right` is the sheet's width). On a narrow canvas the lens moves aside to clear the title and
  Share, and the dock sheds parts as it narrows: the trail keeps only its current step, then goes; then Add's key and
  Arrange's label; then Back and Forward; then Arrange. Zoom and Fit stay.
- A modal layer pushes itself on the keyboard's modal stack (`pushModal`), so no window shortcut fires under it.
  The peek sheet is not modal. Escape closes each layer, and each layer handles its own Escape.
- Layers stack by the `--z-*` tokens: resize handles, the sheet, popovers, dialogs, menus, tooltips, toasts.

## Keyboard

The map is `ui/src/keyboard/keymap.ts`. `Mod` is ⌘ on macOS and Ctrl on Windows and Linux; shortcuts are shown in
each platform's style (`⇧⌘P`, `Ctrl+Shift+P`).

| Action | Id | macOS | Windows and Linux |
| --- | --- | --- | --- |
| Add… | `add.open` | `A`, ⌘K, `/` | `A`, Ctrl+K, `/` |
| Commands… (Add with `>`) | `add.commands` | ⇧⌘P | Ctrl+Shift+P |
| Go to File…, Go to Function… | `add.files`, `add.functions` | ⌘P, ⌘T | Ctrl+P, Ctrl+T |
| Home | `tabs.home` | ⌘0 | Ctrl+0 |
| Next Tab, Previous Tab | `tabs.next`, `tabs.previous` | ⌃Tab or ⇧⌘], ⌃⇧Tab or ⇧⌘[ | Ctrl+Tab or Ctrl+Shift+], Ctrl+Shift+Tab or Ctrl+Shift+[ |
| Close Tab | `tabs.close` | ⌘W | Ctrl+W |
| New Board | `board.new` | ⌘N | Ctrl+N |
| New Page | `board.newPage` | ⌥⌘N | Ctrl+Alt+N |
| New Window | `window.new` | ⇧⌘N | Ctrl+Shift+N |
| Close Window | `window.close` | ⇧⌘W | Ctrl+Shift+W |
| Add Project… | `project.add` | ⌘O | Ctrl+O |
| Compile (the selected project, or the active board's) | `project.compile` | ⇧⌘B | Ctrl+Shift+B |
| Code, Flow (the page's lens) | `board.lensCode`, `board.lensFlow` | ⌘1, ⌘2 | Ctrl+1, Ctrl+2 |
| Toggle Sidebar | `view.toggleSidebar` | ⌘B | Ctrl+B |
| Back, Forward | `navigation.back`, `navigation.forward` | ⌘[, ⌘] | Ctrl+[, Ctrl+] |
| Fit | `board.fit` | ⇧1 | Shift+1 |
| Zoom In, Zoom Out | `board.zoomIn`, `board.zoomOut` | ⌘= (or ⇧⌘+), ⌘- | Ctrl+= (or Ctrl+Shift++), Ctrl+- |
| Arrange by Call Depth | `board.arrange` | ⇧A | Shift+A |
| Undo, Redo (board edit) | `edit.undo`, `edit.redo` | ⌘Z, ⇧⌘Z | Ctrl+Z, Ctrl+Shift+Z |
| Preferences… | `app.preferences` | ⌘, | Ctrl+, |
| Publish | `board.publish` | ⇧⌘E | Ctrl+Shift+E |
| Remove Project from Workspace… | `project.remove` | none | none |
| Copy Diagnostics | `help.copyDiagnostics` | none | none |

These are the actions a person can rebind. Every other registered action (folding, groups, describing, Show
callers, Share, the project menu's and board menu's items, and the peek sheet's keys) has no default shortcut, or a
fixed one, and is reached in Add's `>` mode or its menu. ⌘N makes a draft in the selected project on Home, and in
the active board's project in a board tab.

How keys are dispatched (`ui/src/keyboard/dispatcher.ts`):

- Events a component already handled (`preventDefault`: lists, menus, the canvas's own keys) are left alone, and so
  are keys while composing text.
- Plain keys (`A`, `/`, `⇧1`, `⇧A`) never fire while typing in a text field; ⌘/Ctrl shortcuts do, except Undo and
  Redo, which leave a text field its own native undo.
- While a modal layer is open (Add, any dialog), no window shortcut fires.
- Later registrations win: a mounted view's actions over the frame's. A disabled action lets the key through.
- A handled key is `preventDefault`ed, which also stops Electron from running the application menu's accelerator for
  it, so a key runs its action once. Auto-repeat is ignored, except for zoom.
- Undo and Redo go to the board you're working in: the innermost command scope around focus, or the one clicked last.
- `[`, `]`, `=`, `-`, and `,` match by physical key when Shift changes the character typed, so ⇧⌘] works on every
  keyboard layout; ⌃Tab works on macOS.
- The application menu's items run the same actions as their keys: the main process forwards them as
  `menu_command` events (`MENU_COMMAND_ACTIONS` maps each to its action id). Add Project…, New Window, and Close
  Window run in the main process itself.
- In lists and trees, arrow keys move, Enter opens, and typing letters jumps by name; Shift+F10 or the context-menu
  key opens a context menu; Escape closes menus, popovers, sheets, and dialogs.

### Custom key bindings

Every action in the table can be rebound in Preferences → **Keyboard**, which lists them by category with their
effective shortcut. Click a row's shortcut and press the new combination (modifiers alone keep it listening; Escape
cancels; Backspace leaves the action with no shortcut). **Reset** returns a row to its default and **Reset All**
every row. If two actions end up on the same combination (including an action's second default, such as `/` for
Add), both rows are flagged and Save stays disabled until one changes.

Bindings are stored in `preferences.json` as `keyBindings`, a map from action id to one shortcut in the keymap's
notation (`Mod+Shift+K`, case-insensitive; `Plus` and `Space` name those keys); `""` means no shortcut. A binding
replaces all of an action's default shortcuts. Entries for unknown ids and shortcuts that don't parse are ignored, so
a hand-edited file never breaks the keyboard (`ui/src/keyboard/bindings.ts`).

Saving sends `preferences_changed` to every window, which applies the bindings at once: the action registry
re-resolves every shortcut, so the dispatcher, Add's commands, menus, and tooltips show and use the effective ones.
The application menu keeps its default accelerators; for an action the menu also owns, the old default still works
through the menu, unless the new bindings give that combination to another action, which the window then handles
first.

## Theme

`ui/src/theme/tokens.css` defines every color, size, radius, layer, and duration as a CSS variable, in three blocks:
`:root` (light), `@media (prefers-color-scheme: dark) :root:not([data-theme="light"])`, and `:root[data-theme="dark"]`
(the last two identical). The theme follows the OS unless Preferences sets one, which `applyThemeOverride` writes as
`data-theme` on `<html>`. The dark set is designed, not inverted ([U-51](decisions-and-tradeoffs.md#u-51-the-studios-visual-system)).

| Group | What |
| --- | --- |
| Grounds | `--color-canvas`, `--color-home`, `--color-sidebar`, `--color-chrome` (top bar and active tab), `--color-tabstrip`, `--color-surface` (cards, dock, menus, dialogs, the sheet, toasts), `--color-control`, tracks, pressed |
| Rows | `--color-hover` (one step lighter than selection), `--color-selection`, a stronger selection, matched text |
| Text | `--color-text`, `--color-muted` (secondary text), `--color-subtle` (line numbers), `--color-faint` (rules, icons, and disabled states only, never text) |
| Lines | `--color-border`, `--color-border-shell`, card rules, placeholder lines, dashed borders, `--color-rule-soft` (the rules inside a line's contract and around a type's definition), `--color-type-underline` (a written type that opens) |
| Accent | The indigo `--color-accent` (light `#5862C0`, dark `#A0AAFF`) for selection, the primary button, links, and focus; nothing else in the chrome is colored unless it means something |
| Meaning | `--color-review` (amber: needs review), `--color-warn` and its fill (banners), `--color-success` (the "Graph current" dot only), `--color-danger` (destructive actions and `DELETE`) |
| Reader | Authored magenta, step rows, arrows, the unresolved underline, external call names, the v7 syntax palette, and the Flow node colors (the last two keep the reader's values in both sets) |
| Monograms | A neutral gray square with the project's initial, so it never clashes with the reader's colors |
| Type | Inter for the interface and JetBrains Mono for code, both bundled; sizes paired with leadings (`--text-13` with `--leading-20`); code never ligates |
| Shape and size | Radii from 4 to 12px; rows 28px, tab strip 38px, top bar 48px, the sheet 640px; shadows only for floating things |
| Layers and motion | `--z-*` (see [Overlays](#overlays)); 90 and 160 ms, and none when the OS asks for reduced motion |

`src/theme/contrast.test.ts` checks that text meets WCAG AA (4.5:1) and focus rings and marks 3:1 on the surfaces
they sit on, with four listed exceptions that are marks repeated by text elsewhere.

## Components

The shared kit is `ui/src/components/` (import it from `components/index.ts`), styled only with the tokens:
`Button` and `TextButton`, `IconButton`, `KeyChip`, `MenuButton` and `useContextMenu` (with submenus), `Popover`,
`Tooltip` and `HoverCard`, `Dialog`, `ConfirmDialog`, and `PromptDialog` (placed on the window or the canvas),
`Sheet`, `SegmentedControl` (the lens, a card's switch, the Health | Activity switch), `Switch`, `Checkbox` (with a
mixed state), `RadioGroup`, `Monogram`, `StateIndicator` (a project's state as a row glyph or before Home's state
line), `ProgressRing`, `Toaster` with `showToast` and `dismissToast`, `SectionLabel`, `ListRow`, `Badge`,
`KindGlyph`, `TreeView`, `ResizeHandle`, `TextField` and `SearchField`, and `EmptyState`. Icons are one 16px set
(`icons/`, `<Icon name>`), with the project-state glyphs beside them. Long lists are virtualized with TanStack
Virtual.

## Where V1's parts went

The window was rebuilt from V1's IDE-shaped workbench ([U-26](decisions-and-tradeoffs.md#u-26-a-studio-window)).
Every region and command had a new home; nothing was dropped.

| V1 | Now |
| --- | --- |
| Title bar folder switcher (recent folders, Open Folder, New Window, Folder Settings, Close Folder, Preferences) | Home's sidebar: the projects, Add project, the project menu, the gear; recent folders became Recents (recent boards) |
| Title bar board title, Code \| Flow, Compile, Publish | The board header (rename in place); the canvas top bar's lens and Publish; Compile on a new project's Home, then the project menu |
| Inspector, and its toggle | The board sidebar's Selected section, and editing on the card; ⌘B toggles the whole sidebar |
| Board menu `⋯` | The board header's menu; Fold all and Unfold all also in the canvas menu; Mermaid under Share |
| Activity bar | Gone |
| Files view (the disk tree and its marks and menu) | Finding a file: Add → Files. Scope actions: Project settings → Scope. Open in editor and Reveal in Finder: the peek sheet's menu and the project menu. Show callers: the card menu |
| Entrances view | Home → How it starts, and Add's map. Add Entrance: the project menu and How it starts; editing and removing a manual entrance: its row on Home |
| Search view (and match case, whole word, regex) | Add, with the same tabs and text options |
| Boards view (boards, drafts, outline, the Flow-label stepper, groups, health and repairs) | Home's Boards and the sidebar's tree; On this page; "Label them"; the card's review banner and Home's Health |
| Editor tabs; source tabs | Window tabs; the peek sheet |
| The board bar (Back, trail, zoom, Fit, Arrange) | The dock, with Forward added |
| Bottom panel: Problems, Index log, References | Home's Health line and Health and activity; Selected's Called by and Reached from, and Show callers |
| Status bar (graph state, notes, connection, problem count, zoom) and its menu | The project row and Home's state line; the notes in the state line's tooltip and Project settings → General; the connection as a toast; zoom in the dock; the menu's items in the project menu |
| Welcome screen; first-Compile screen | First launch on Home, Add project, and dropping a folder; a new project's Home with the scope tree |
| Close Folder | Remove from workspace, in the project menu (it never deletes `.unfold/`) |
| Folder Settings dialog | The Project settings dialog |
| Command palette, Go to File, Go to Symbol | Add: `>` for commands; ⌘P opens it on Files, ⌘T on Functions |
| Several windows on one folder | Several windows on the workspace; Open in new window on a tab or in the board menu |
