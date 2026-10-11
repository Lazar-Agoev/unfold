# UI shell

Read this for renderer ownership, state, interactions, layout, keyboard, and theme. Start
with the feature folder in `ui/src/`; graph/board rules belong to [reader](reader.md), command
contracts to [IPC](ipc.md), and Electron ownership to [desktop](../desktop/README.md).

## Modules

| Concern | Module |
| --- | --- |
| Window frame, Home, navigation | `app/`, `home/`, `state/tabs.ts`, `state/window.ts` |
| Board documents, sessions, undo, actions | `state/boards/`, `state/session.ts`, `state/actions.ts` |
| Canvas/cards/arrows/frames/resources/notes | `canvas/` and its feature subfolders |
| Code/Flow/types | `code/`, `flow/`, `canvas/lines/` |
| Add and source peek | `add/`, `peek/` |
| Settings/preferences | `settings/`, `preferences/` |
| Typed engine data | `ipc/commands.ts`, `queries.ts`, `stream.ts` |
| Keyboard/menus/focus | `keyboard/` |
| Shared components/theme | `components/`, `icons/`, `theme/` |
| Agent edit operations | `state/boards/agent/`, `agent/` |
| Hidden render page | `render/` |

Module comments and nearby tests own local behavior. Search the needed generated type;
never read the entire IPC mirror just to change a component.

## State and engine data

Zustand stores keep workspace/project selection, per-window tabs/layout/overlays, sessions,
and board documents/undo. TanStack Query owns engine-derived data. Keep transient navigation,
local views, shared document edits, and committed graph data distinct.

Project IDs scope command arguments and query keys. Engine events invalidate the relevant
project's affected queries and any cross-project result that can include it. Host restart
resets/reconnects data through session code; unrelated projects should not refetch wholesale.

Pure document edits are Immer recipes; one user/agent operation batch is one undo step.
Preserve page/item ID rules, remove dangling uses when deleting items, and omit optional
fields rather than writing `undefined` into a persisted document. Save failures retain work.
Publish/Discard/restore/conflict behavior comes from reader/workspace, not ad-hoc UI copies.

## Screens

Home lists a project's entrances, boards, and health; a never-compiled project stages scope
until its first Compile. Add searches projects by kind/text and exposes commands. Peek shows
a verified source file with folds/call links. Windows share projects but keep their own tabs
and board sessions. The [user guide](user-guide.md) owns click-by-click instructions.

Cards use exact Code, written Flow, or Diagram/type data from the engine. The global lens is
transient; a card's explicit saved view takes precedence. Loading, missing, held, changed-source,
and unresolved states are deliberate states, not empty-success fallbacks.

Canvas geometry, measurement, arrange, label placement, and hit targets are TypeScript-owned.
Keep measured sizes separate from document positions. Auto-layout should not erase deliberate
placements or authored contents. Fonts must be ready before stable measurement.

Typed line labels use occurrence identity; the selected line replaces its compact label with
a contract. Declared types open inline, preserving written type text and explicit uncertainty.
Avoid overlap with cards/call sites; geometry helpers have focused tests. Connections/resources
and frames use the same page identity/undo model, with Rust grounding and health validation.

## Agents in the window

Main sends typed requests to the holding renderer. `state/boards/agent/` validates/applies
operations, checks expected revisions/people-text options, saves through the normal board
path, and returns the resulting page. Never mutate a different window's stale copy.

Agent batches are atomic undo steps with persistent author marks. Descriptions/labels retain
unchanged authorship; agent-only re-signing cannot make a person's explanation reviewed.
Working outlines are transient and clear at finish. Rejected claims must leave document and
undo history unchanged. Keep stale revisions, disconnected projects, held references, and
unsupported claims as actionable failures. See [grounding](reader.md#grounding).

## The render window

`render/` is a separate entry point used by desktop's hidden page renderer. It subscribes
to engine data for cards without claiming/opening a board. Wait for fonts, data, and geometry;
report completeness/issues rather than silently omitting failed items. Preserve scale/bounds,
PNG limits, and request deadlines from the shared render contract.

## Overlays

Keyboard/focus dispatch follows the active layer. Dialogs block window shortcuts; typing
blocks plain canvas keys. Menus/popovers/sheets must close/restore focus through the shared
components and keyboard scopes, not document-level listeners scattered through features.
Use theme layer tokens and respect Escape/click-outside behavior. Preserve the Add/peek/menu
layer rules in the existing tests when changing overlay interaction.

## Keyboard

The action registry and dispatcher own runtime bindings and focus scopes. The table is
checked against journey coverage; preserve its action IDs when updating shortcuts.

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
| Diagram, Flow, Code (the page's lens) | `board.lensDiagram`, `board.lensFlow`, `board.lensCode` | ⌘1, ⌘2, ⌘3 | Ctrl+1, Ctrl+2, Ctrl+3 |
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

Preferences can replace action defaults, including an empty binding. Parse/ignore invalid
hand-edited bindings conservatively. `preferences_changed` updates registry, menus/tooltips,
and Add commands in every window. Application-menu accelerators retain their specific OS rules.

## Theme and components

`theme/tokens.css` owns light, system-dark, and explicit-dark tokens. Components consume
those tokens rather than new hardcoded values. Bundled Inter and JetBrains Mono fonts support
stable text measurement; code disables ligatures. Reduced motion and OS theme preferences
flow through the shared theme helpers.

Use the kit in `components/` for dialogs, buttons, fields, sheets, menus, tooltips, trees,
state indicators, and toasts. Virtualize long lists. `theme/contrast.test.ts` checks contrast;
keep exceptions explicit and justified. Local styling changes need visual inspection and
focused checks, not automatic full app E2E.

## Where V1's parts went

This compact coverage map is retained because `e2e/journeys/coverage.spec.ts` matches its
rows to existing journey steps. It is optional context when changing those journeys.

| V1 | Now |
| --- | --- |
| Title bar folder switcher (recent folders, Open Folder, New Window, Folder Settings, Close Folder, Preferences) | Home's sidebar: the projects, Add project, the project menu, the gear; recent folders became Recents (recent boards) |
| Title bar board title, Code \| Flow, Compile, Publish | The board header (rename in place); the canvas top bar's lens and Publish; Compile on a new project's Home, then the project menu |
| Inspector, and its toggle | The board sidebar's Selected section, and editing on the card; ⌘B toggles the whole sidebar |
| Board menu `⋯` | The board header's menu; Fold all and Unfold all also in the canvas menu; Mermaid and PNG under Share |
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

## Checks

Run the affected Vitest files and UI typecheck. If changing either table above or mapped
journey steps, run the static coverage check from [testing](testing.md#end-to-end-journeys).
Use a focused Electron journey only when integrated interactions need the running app.
