# UI shell

Read this for renderer ownership, state, screens, interactions, and theme. Start with the feature folder in
`ui/src/`. Graph and board rules belong to [reader](reader.md), command contracts to [IPC](ipc.md), and Electron to
[desktop](../desktop/README.md). The [user guide](user-guide.md) owns click-by-click behavior.

## Modules

| Concern | Module |
| --- | --- |
| The window: sidebar, content, session | `app/`, `state/session.ts`, `state/shownBoard.ts`, `state/window.ts` |
| Home: overview, How it starts, Boards, Health | `home/` |
| Writing entrances: Add as entrance, the code search | `entrances/` |
| Board screen: header, Pages, Selected, top bar, dock | `board/` |
| Board documents, sessions, undo, autosave, agent operations | `state/boards/`, `state/boards/agent/` |
| Cards from the engine | `state/cards.ts` |
| Canvas: cards, arrows, connections, frames, resources, notes | `canvas/` |
| Drawn flow layout and rendering | `flow/` |
| The code panel | `code/`, `state/codePanel.ts` |
| Explaining by hand: Combine…, proposals, Add resource… | `explain/`, `state/boards/explain.ts` |
| Add | `add/` |
| Menus and dialogs | `menus/`, `dialogs/` |
| Settings | `settings/` |
| Agents in the window: marks, links, view reports | `agent/` |
| Page checks, hidden render page | `board/checks/`, `render/` |
| Typed engine data | `ipc/` |
| Kit and theme | `components/`, `icons/`, `theme/` |

Module comments and nearby tests own local behavior. Search for the one generated type you need; never read the whole
IPC mirror to change a component.

## The window

There is one window. The sidebar (264 px, fixed) sits on the left with the macOS window buttons over its top; there
is no title bar, and the board's top bar moves the window too. Beside it, the content area shows Home or one board.
Closing the window keeps the app running (a `just dev` session quits).

- **The shown board.** Every board opened while the app runs keeps its session and undo history; `shownBoard` says
  which one is in front. The window reports it (`report_shown_board`) and comes back to it at launch.
- **Session** (`state/session.ts`). At launch the Show all calls switch and the shown board come back from
  `window_info`; only then are agents' requests answered. When the engine host restarts, a toast shows; once it is
  back, the selection is sent again, every open board reopens and saves its unsaved edits, and the query cache resets,
  in that order.
- **Nothing in browser storage.** What the window remembers lives in Rust's app-data files; view state (cameras,
  selection, the active page, Selected's collapse) lasts only while the app runs.

## State and engine data

Zustand stores hold the workspace selection, the shown board, open boards with their undo history, overlays, and the
code panel. TanStack Query holds engine data under `["engine", command, args]`. Keep navigation, view state, document
edits, and compiled data apart.

Every project command's arguments carry `project`, so an engine event invalidates only that project's queries (and a
`search` that includes it). Cards refresh when a file they draw changes.

Document edits are pure functions applied with Immer. Every edit is one undo step (typing bursts merge), one history per board
across its pages; undo shows the page a step changed. Optional fields are deleted, never set to `undefined`. Removing
an item takes what would dangle with it.

**Autosave** (`state/boards/store.ts`): edits save a moment later through `save_board`, built on the file's revision
last loaded or saved; saves never overlap. A failed save keeps the edits and shows "Not saved · Retry"; one the engine
host's restart cut off waits and saves once the engine is back. An outside change (`board_reloaded`) replaces the
board and starts its undo history over when nothing is unsaved. When it arrives while edits wait for the autosave, or
while a save is failing or in flight, saving stops and the bar on the board asks the person: "The board's file changed
on disk." with *Keep my edits* (written over the file) or *Load the file* (its version, with the undo history starting
over). A save the engine refuses because the file changed (`board_changed`) asks the same. A file that isn't a valid
board, such as a merge conflict, stops saving too: "The board's file on disk isn't a valid board, so saving stopped."
with *Save my version*; once the file is a valid board again, the board takes it by itself when nothing of the
person's is unsaved (edits from before the file broke count), and asks otherwise. A file back to the version the board
builds on lets saving go on, and a deleted file is written back by the next save. While saving is stopped, agents'
edits and repairs on the board are refused (`board_changed`). Remove from workspace, Rename, and Export PNG… stop when
a board can't be saved (`unsaved_edits`).

## Home

`home/` shows the project selected in the sidebar: the header (path, name, project menu, New board), the state line,
then the overview.

- **How it starts** (`home/starts/`): one bordered list in two columns, names left and `run` or `on` right in mono.
  Apps are bold rows with their entrances indented under them, in file order; there are no badges at rest. Hover shows
  a chevron, and a copy icon on an app's `run`. A problem is an amber dot; on hover the right column says what
  happened ("Moved to …", "Route edited", "Not found", "Not analyzed", "Not a start", or the entry's own problem) with
  Fix or Choose code…; for code the graph leaves out, Fix opens `unfold.toml`. Fixes are offered, never applied
  unasked; a Fix for moved code and Choose code… rewrite one `code` key (`replace_code`, which names the key shown, so
  a file changed since is refused and the toast says why). Search appears above 15 entrances. Empty: "No entrances
  yet" with Add entrance and Copy agent prompt.
- **Tracing** (`trace.ts`): clicking an app or entrance makes a new board named after it: the start's card, compact,
  and its code's cards expanded in a column to its right, in run order. A row the graph left out opens its lines in
  `unfold.toml`.
- **Boards**: one list, most recently edited first, with folders.
- **Health line**: files that don't parse, boards that need review, and entrances whose code isn't found, in amber;
  each leads to what needs a look. Health is also a view of Home: each problem with a short explanation (a broken
  `unfold.toml`: Unfold keeps the settings that last parsed; a configuration input: imports and calls may not
  resolve) and the file to open.

Everything comes from `entrances`, `project_summary`, `problems`, and `list_boards`; no key is read.

## The board screen

The board sidebar shows the board's header (back to Home, the project and folder, the title, the board menu), Pages,
and Selected pinned to the bottom. The content area holds the top bar (the page's name, "Claude is working…" while an
agent works, and the Show all calls switch), the canvas with its banners and the dock (Add, Arrange, zoom, Fit), and
the code panel beside both when open.

**Selected** (`board/selected/`) shows what is selected: a card (description, Called by, Reached from; a card whose
code is missing shows its key and "Missing from the graph", its banner's words, once), a block (its name, its part's
lines), a combined step, a connection (kind, label, a relationship's columns and cardinality), a frame, a resource
card, a note, a proposed step, or an arrow (titled by the call as written, then its contract: what it sends, returns,
and raises, with "caught" when a handler around it catches). A type name opens its definition in the code panel.
Bylines say who wrote what: "by you" for the person at the window, another person's name, or an agent's mark.

## The canvas

`canvas/` draws a page with React Flow, over the design pages' dot grid (a 2 px dot every 18 px, `--color-dotgrid`).
Cards come from `card` (`state/cards.ts`), asked for with the page's overlay, the card's open paths, and the Show all
calls switch. A card is cached under where the overlay's entries go and the words of those its drawing shows, so
typing a name redraws the cards that draw its block, not the whole page (the first letter of a new name, or clearing
one, redraws every card once).

- **The canvas computes geometry only.** `flow/layout.ts` turns a drawn flow into `FlowGeometry` (pieces, wires,
  every block's rect, page items, call anchors), pure and deterministic; `FlowView` renders it and decides nothing.
  What a block allows comes from its drawn-flow `actions`; there is no TypeScript copy of the overlay rules, and no
  UI code reads keys.
- **Cards**: modules, classes, functions, and inline functions (compact, or expanded to their drawn flow), type cards
  (fields and inherited fields from `type_definition`), and app and entrance cards, drawn as declared (a dark border).
  A card's title is its name, with `ASYNC` after it for an async function; an expanded card's signature row says what
  follows the name (`(order_id)`), and an inline function's, its whole head. A card's description shows all of it,
  wrapped (a type card's: one line, with "more"), in the authored color with no mark. A card whose code is missing is
  dashed, and its review banner alone says so. Code a start names that isn't found is its own dashed card with its
  fix. A card whose file changed since the last compile, or doesn't parse, says so in a line under its header, in the
  code panel's words and banner look ("Changed since the last compile", "Doesn't parse"), and keeps the words its
  facts' text gives (`header.state`); an editor's save refetches it at once (`files_changed`), and the compile's
  `graph_updated` again.
- **Arrows** come from call boxes whose callee has a card on the page: from the call on an expanded card (insets and
  collapsed combined steps included), from the edge on a compact card (one per target, carrying its calls). Start
  cards point at their code in order, dashed to code whose file doesn't parse; type fields point at their types. The
  board file holds no arrows. Which arrows a page draws, how a card shows, and the walk of a drawn flow (its blocks,
  project calls, and combined and proposed steps, in insets opened from any call box) are one module the MCP server
  imports too ([desktop/src/shared/drawing.ts](../desktop/src/shared/drawing.ts), `@unfold/desktop/drawing`), so
  agents read what the person sees; the canvas places the arrows' ports.
- **Connections, frames, resource cards, and notes** share the page's id space and undo history.
- **Sizes** are measured with the bundled fonts before anything renders; measured sizes stay apart from document
  positions, and Arrange never erases authored contents.

## Explaining by hand

People make what agents make, under the same rules (`state/boards/explain.ts`), except frames and notes, which only
agents create: people rename, describe, move, collapse, and remove frames, and edit, move, and remove notes.

- **A block's menu** (`canvas/cards/blockMenu.ts`): View code, Open/Close in place, Name…, Describe…, Combine…, Add
  proposed step after… or at the start…, and Draw link… on a call. Open in place, Combine…, and the proposed-step
  items show only when the block's actions allow them; any block can be named.
- **Combine…** lists the body's drawn steps and ticks a consecutive run; a title is required; it starts collapsed.
- **Add resource…** (the canvas menu), **Draw link…** (card, resource, and frame menus; an `other` connection with a
  label), **Kind ▸** on a connection, and the relationship editor in Selected. Draw link… on a call starts the link at
  that call, which becomes its step, so Kind ▸ offers `calls` for it; the save refuses a `calls` connection that
  doesn't start at an unknown call.
- **Review** (`ReviewBanner.tsx`): Accept, Reconnect… (a name search), and Remove from page.

What people make by hand (a combined step, a proposed step, a resource card, a drawn link, a connection's kind, a
relationship) saves at once as one undo step (`saveStep`); the board's save decides, and a refusal comes back inline
or as a toast with the page unchanged. Apart from requiring a title, no TypeScript copy of the board's rules checks
first: ids, how long a note or a proposed step may be, and how deep frames nest are the save's to refuse. Text edited
in place (a note, a proposed step) is an edit on each keystroke, so a save that refuses it shows its message under the
field while you type ("A note holds at most 400 characters; this one has 412."), and the folder dialogs show the
folder commands' refusals on their field.

## The code panel

`code/CodePanel.tsx` sits right of the board, 392 px, full height beside the top bar; source is shown nowhere else
(the [model](reader.md#the-code-panels-model) is the reader's).

- **Opening:** `</>` on a hovered or selected block and on card headers, View code in a block's menu, a gap tick, a
  file from Add, and a type name in Selected. App and entrance cards open their lines in `unfold.toml`. A resource
  card or a connection opens the first source it cites, in the reader's order: a table's type or a connection's step,
  then its first citation.
- **Following:** while open, the panel follows the selection; something without source leaves it where it is. A
  selection the panel makes itself (a line click, `⊕`) is quiet, so the panel doesn't move. Lines opened from a
  combined step, thin row, or gap tick are rebuilt from the card as drawn now.
- **Outline:** the selected block is outlined on the canvas on its own shape, or on what holds it (a collapsed
  combined step, a thin row, a gap tick). Hovering a line lights its block (a call's name lights that call); a line no
  drawn block covers lights its gap tick.
- **Path:** each step widens the view; the selection stays marked and selected. When it doesn't fit, it gives way in
  this order: the steps strictly between the definition (the step after the file) and the selection, deepest first,
  each run drawn as one `…`; then the file's folders, shortened in the middle so its name stays (`src/…/orders.ts`,
  then `…/orders.ts`); then the definition. The step showing never goes, and the selection's label shortens only past
  all of that. Hovering the path shows the selection's full key.
- **States** show as a banner, and nothing is marked unless the lines are current.
- **Actions:** Open in editor (errors as a toast) and `⊕`, in the gutter beside the line number of a line that starts
  a definition with no card on the page (the outermost one, when several start there). `⊕` adds the card where new
  cards go and leaves the canvas where it is. × or Escape closes the panel; it closes when the window shows another
  board or Home. Long views are read and rendered in pages of 200 lines as they scroll into view; the first answer
  holds the page around what the selection marks. A line the engine cut (over 1,000 bytes) ends in a muted `…`.

## Add

`add/` is one box over the board: a name search in the board's project (modules, definitions, types, apps, and
entrances as one ranked list, then files), each row with its kind and where it is (an entrance's app and what starts
it: `Shop API · POST /orders`). Enter adds a card (or focuses the one already
there) or opens a file in the code panel, once the list answers what was typed. Home has no Add.

## Agents in the window

The main process sends agent requests to the window ([IPC › Agents](ipc.md#agents)). `state/boards/agent/` applies
them with the same pure edits people's actions use, as one undo step named "<agent> · <title>", every new text signed
with the agent's identity, saved at once through the normal board path. A refused edit leaves the document and its
undo history unchanged.

- The window checks only what it draws and what an operation needs on the page: that a connection's step is drawn on
  its from card, that a connection doesn't repeat a compiled call, where a proposed step can go (the block's actions),
  and that a note's attachment and a frame's members and parent are there. The save checks the board's rules (ids,
  ends, kinds, lengths, frames), keys, the overlay rules, and grounding, and its refusal is the answer.
- People's words stay theirs unless the request says the person asked (`replacePeopleText`); unchanged text keeps
  its author. A repair that removes an item, a name, or a description is held to the same rule, what goes with the
  item included.
- An edit that names the page revision the agent read (`expectRevision`) is compared in the step `saveStep` saves, so
  a person's edit after the read refuses it (`revision_mismatch`).
- **Working marks** (`working.ts`): while an agent works on a page, the top bar says "Claude is working…" and the items
  it changed have a soft outline. `finish_work` clears them and shows a "Claude finished" toast with the summary.
  They are never saved and never undo steps.
- **View reports** (`agent/viewReport.ts`): the shown board and page, the selection, the code panel's block and
  lines, the items in view, and the held boards, about 150 ms after a change and at once on focus.
- The agent mark (a spark and the agent's name, `✦ Claude`, from its author id) is on a frame's title, a note, a
  resource card's header, a connection's chip, and in Selected's bylines ("by ✦ Claude"). A card's description
  carries none, as the design pages draw it.

## The render window

`render/` is a separate entry the desktop's hidden render window loads. It draws one page read-only, exactly as the
board view does, adopting it as a session that is never saved, with Show all calls as the person's window has it
(each `RenderJob` carries the switch). Once every card is measured, every query has loaded,
the fonts are in, and nothing moves, it answers with the image's bounds or the page checks (`board/checks/`:
overlap, crossings, density, missing descriptions, connections and resources that cite nothing, and strays).

## Overlays and focus

Keys stay standard: Escape closes, Enter confirms, and the Edit menu gives undo, redo, cut, copy, paste, and select
all. Edit › Undo and Redo reach the board as `menu_command` (a focused text field
keeps its own undo, and a modal dialog blocks them). Menus, popovers, and dialogs come from `components/`: they trap
or return focus, close on Escape and outside clicks, and a field that took Escape keeps it.

## Theme and components

### Theme

`theme/tokens.css` holds the tokens; Unfold is light only. Components use the tokens, never new hard-coded values.
Inter and JetBrains Mono ship with the app (`@fontsource-variable`), so text measures the same everywhere. Nothing
ligates: `theme/base.css` turns ligatures off on every element (`!important`, because the `font` shorthand resets
them), so `===`, `=>` and `->` show the characters in the file. Reduced motion is respected everywhere. `theme/contrast.test.ts` checks contrast; keep any exception
explicit.

### Components

Use the kit in `components/` for buttons, dialogs, fields, menus, popovers, tooltips, toasts, switches, list rows,
and state indicators. Virtualize long lists. Screens compose the kit and never restyle it from outside.

## Checks

Run the affected Vitest files and `pnpm --filter @unfold/ui typecheck`. Use a journey only when an interaction needs
the running app ([testing](testing.md)).
