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

Cards use exact Code, written Flow, or Diagram/type data from the engine. The page lens
(Diagram | Code) is local view state; a legacy saved Flow lens reads as Diagram. In Diagram, a
function or entrance card's saved view says whether it is compact or expanded (its chevron), and
calls expanded in place are saved on the card; type declaration and resource cards keep their fields
in both lenses, and a class's card shows its fields compact and its detail expanded. Loading, missing, held, changed-source, and unresolved states are deliberate states, not
empty-success fallbacks.

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

The action registry and dispatcher own runtime bindings and focus scopes. Preserve action IDs
when updating shortcuts.

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

## Checks

Run the affected Vitest files and UI typecheck. Use a focused Electron journey only when
integrated interactions need the running app.
