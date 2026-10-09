# UI plan: the studio renderer (`ui/src/`)

The plan the UI implementation agents build from (redesign step 4 onward). It says what of V1's renderer stays, what
goes, the target modules and their public exports, the state model, the IPC client, the tokens, the shared
components, every screen's tree, data, and events, the keyboard, performance, the work split with file ownership,
and the tests.

Sources, in order of precedence: [the redesign](../studio-redesign.md), [the contract](contract.md) (its §11
amendments win), [decisions](decisions.md) (V-1..V-39, A-1, P-1, and this plan's **V-40..V-80** under "UI plan
decisions"), and [design/](design/) for exact values. Where this plan cites a value, the design file is still the
source; the plan does not restate geometry the design files give.

Conventions: `ProjectId`, `BoardId`, `PageId`, `Key`, `ProjectName` as in contract §4.1. `BoardRef = {project, id}`.
`PageTarget = {board: BoardRef, page: PageId}`. "U1b" etc. are packages of [§10](#10-work-split).

## Contents

1. [Kept, restyled, rewritten, deleted](#1-kept-restyled-rewritten-deleted)
2. [Target module layout](#2-target-module-layout)
3. [State model](#3-state-model)
4. [IPC client](#4-ipc-client)
5. [Theme tokens](#5-theme-tokens)
6. [Shared components](#6-shared-components)
7. [Screens and their data](#7-screens-and-their-data)
8. [Keyboard](#8-keyboard)
9. [Performance](#9-performance)
10. [Work split](#10-work-split)
11. [Unit tests](#11-unit-tests)
12. [Decisions made here](#12-decisions-made-here)

---

## 1. Kept, restyled, rewritten, deleted

Verdicts: **keep** (unchanged apart from mechanical type and token renames), **restyle** (logic kept, visuals per
the designs), **adapt** (logic kept, changed for projects/pages), **rewrite** (new code, V1 logic named here may be
ported), **delete**, **move** (file kept, new home). Owner = the package that does it (§10).

The V1 tree is copied once, before U1 starts, to `docs/redesign/build/v1-ui/src/` (read-only reference so agents can
port code without git; deleted by U6). Nothing under it is built, linted, or tested.

| V1 path | Verdict | What changes / what is worth keeping | Owner |
| --- | --- | --- | --- |
| `main.tsx` | rewrite | Boot: theme, `window_info`, render `<App/>`. Transfer receiver install moves to `state/boards/transfer.ts` | U1c |
| `boards/` (BoardChrome, BoardDialogs, dialogs.ts, flows.ts, boards.css, boards.test) | delete | `flows.ts` pattern (run, toast on failure, never throw) is ported into `menus/actions.ts`; RepairDialog's per-repair copy already lives in `state/boards/health.ts` (`repairEffect`); `parseLines` → `board/groupFromLines.ts` | U1c deletes; U2, U5 port |
| `canvas/ArrowEdge.tsx` | restyle | Entrance→handler arrow in `--color-contour`, call arrows `--color-arrow`, no port dots (02 §4.7) | U2 |
| `canvas/BoardBanner.tsx` | restyle | Conflict (Keep mine / Take theirs) and failed-save banners, top of the stage | U2 |
| `canvas/BoardBar.tsx` | delete | Replaced by `board/Dock.tsx` (adds Forward, zoom %) | U2 |
| `canvas/BoardCanvas.tsx` | adapt | Node/edge identity caches, reflow, focus requests, pane menu kept. Reads one **page**; adds `LinkEdge`, link drawing mode, placeholder nodes | U1b (types), U2 |
| `canvas/BoardTab.tsx` (+test) | rewrite | → `board/BoardView.tsx`: ReactFlowProvider, actions, undo scope, Arrange, first-fit | U2 |
| `canvas/arrangeBoard.ts`, `placement.ts`, `measure.ts`, `sizing.ts`, `useBoardArrows.ts` | adapt | Per page; card keys namespaced by project; `sizing.ts` widths per V-57 | U1b, U2 |
| `canvas/arrows.ts` (+test) | adapt | Arrows only between same-project cards (the graph has no cross-repository edges) | U1b |
| `canvas/layout.ts` (+test) | keep | Pure geometry; constants per V-57 | U2 |
| `canvas/cardData.ts` | adapt | Key `["engine","card",{project,key}]`; last-verified render keyed by `project\0key`; forget a project on `project_reset` | U1b |
| `canvas/canvas.css` | restyle | Tokens; stage; no shadows | U2 |
| `canvas/cards/BlockCard.tsx`, `EntranceCard.tsx`, `CardNode.tsx`, `InlineCallee.tsx`, `parts.tsx`, `cards.css` | restyle | V-34: header 10/12 padding, ƒ glyph, mono title 13/16, location 11/14, mini lens, full-width review banner, 1.5px accent + ring when selected, no shadow, monogram on foreign cards (10 §5.1). Render models unchanged | U2 |
| `canvas/cards/cardMenu.tsx` | rewrite | New entries (§7, 02): Open source, Show callers, Draw link…, Add as entrance…, Fold/Unfold, Remove from page; group rail menu kept | U2 |
| `code/*` | keep | `CodeView`, `CodeText`, `outline`, `segments`, `text` unchanged; `CallLink` reads its project from `ProjectScope` context (§6); `code.css` restyle: 20px rows, `--color-subtle` numbers, ligatures off, external calls `--color-syntax-external` | U1b (context), U2 (css) |
| `flow/*` | keep | `layout.ts` unchanged (labels map input); `FlowView` icons → `icons/`; `flow.css` token renames only | U2 |
| `components/Button` | rewrite | Variants primary/secondary/ghost/danger, 30/28 heights (components §7) | U1a |
| `components/IconButton` | rewrite | 28/24/20 sizes, pressed state, `icon: IconName` | U1a |
| `components/Dialog` (+test) | rewrite | Window or canvas placement and scrim (V-9), widths 400/520/880; `ConfirmDialog`, `PromptDialog` kept | U1a |
| `components/Menu` (+test) | adapt | One menu style (V-43), submenus, trailing slot, danger items; `useContextMenu`, `MenuButton` kept | U1a |
| `components/Popover`, `Tooltip` (HoverCard) | restyle | Popover style (V-17) | U1a |
| `components/Toast` | rewrite | 07 toasts (title, body, leading, action, close), V-67 | U1a |
| `components/SegmentedControl` | rewrite | Variants `lens`, `mini`, `switch` | U1a |
| `components/TreeView` (+test) | restyle | Keep keyboard model and lazy children; 28px rows; used by the settings scope tree | U1a |
| `components/VirtualList` (+test) | restyle | Keep grouped virtual list and keyboard; used by Add results, Health list | U1a |
| `components/Resizable` (+test) | keep | Sidebar edge 220–360 | U1a |
| `components/TextField` | restyle | `TextField`, `SearchField` | U1a |
| `components/EmptyState` | restyle | Home's empty Boards, empty Health | U1a |
| `components/Kbd` | rewrite | → `KeyChip` (filled, outlined, keycap) | U1a |
| `components/Progress` | rewrite | → `ProgressRing` (14/12px, rotates) | U1a |
| `components/Chip`, `Tabs`, `TreeMarks` | delete | Badge → new `Badge`; tab strip is `app/TabStrip`; marks were the Files view's | U1a |
| `components/internal/*` | keep | Portals, dismissal, floating placement | U1a |
| `editor/EditorArea.tsx`, `useBoardTabs.ts` (+test) | delete | Tabs live in `state/tabs.ts`; rendering in `app/` | U1c |
| `editor/source/model.ts` (+test) | move | → `peek/model.ts` (pages of 200 lines, folds, segments) | U4 |
| `editor/source/SourceTab.tsx`, `.css`, `.test` | rewrite | → `peek/PeekSheet.tsx`: the virtualized line renderer and `⊕`/`●` logic are ported | U4 |
| `inspector/AuthoredField.tsx` | move+restyle | → `board/selected/AuthoredField.tsx` (one-line + more, author, typing coalesces) | U2 |
| `inspector/CardInspector.tsx` | rewrite | → `board/selected/SelectedCard.tsx`; port `uniqueTargets`, focus-on-page helpers | U2 |
| `inspector/NodeInspector.tsx` | move+adapt | → `board/selected/SelectedNode.tsx` (`flowNodeFor`, path words) | U2 |
| `inspector/useAuthor.ts` | move | → `state/author.ts` | U1b |
| `inspector/Inspector.tsx`, `IssueLine.tsx`, `inspector.css`, `inspector.test.tsx` | delete | (Selected tests are new, §11) | U1c |
| `ipc/client.ts`, `commands.ts`, `hooks.ts`, `unfold.ts` | keep | Types come from the regenerated `generated/` and D1's `unfold-api.ts` | U1b |
| `ipc/queries.ts` (+test) | rewrite | §4: keys by project, invalidation per contract §5, no reconnect refetch here (U6 orders it) | U1b |
| `ipc/generated/` | regenerate only | `cargo xtask gen-ipc`; never hand-edited | R7 |
| `keyboard/dispatcher.ts`, `registry.ts`, `scopes.ts`, `modal.ts`, `hooks.ts`, `focus-targets.ts`, `platform.ts`, `bindings.ts`, `menu.ts`, `shortcut.ts`, `index.ts` (+tests) | keep/adapt | Rules kept (§8). `registry`: `ActionSpec` gains `icon`, `group`, `showInAdd` (renamed from `showInPalette`). `shortcut`: physical-key fallback for `[ ] = - ,` (V-75). `menu`: new `MenuCommand` map | U1b |
| `keyboard/keymap.ts` | rewrite | §8 ids only; V1 ids deleted | U1b |
| `palette/` | delete | Add's `>` mode replaces it (`add/commands.ts`); `cmdk` dependency removed | U1c |
| `panel/IndexLogPanel.tsx` | delete | `factLines` → `home/health/activity.ts` (with its test) | U3 |
| `panel/ProblemsPanel.tsx` | delete | `problemLocation`, `boardHealthItem` → `home/health/problems.ts` | U3 |
| `panel/ReferencesPanel.tsx`, `BottomPanel.tsx`, `panel.css`, `panel.test.tsx` | delete | References = Selected + Show callers | U1c |
| `preferences/*` | restyle | Dialog logic, `KeyboardSection`, `mergeKeyBindings`, `parseThreadLimit`, theme and binding hooks kept | U5 |
| `settings/entrance.ts`, `locators.ts` (+tests) | keep | | U5 |
| `settings/EntranceEditor.tsx` (+test) | restyle | Opened through `openDialog({kind:"entranceEditor"})` | U5 |
| `settings/folderSettings.ts` | move+adapt | → `settings/projectSettings.ts` (drafts, `editsBetween`, detector groups, Python projects) | U5 |
| `settings/FolderSettingsDialog.tsx` (+test) | rewrite | → `settings/ProjectSettingsDialog.tsx` (V-16, 08) | U5 |
| `settings/store.ts`, `SettingsDialogs.tsx`, `actions.ts`, `index.ts`, `settings.css` | rewrite | Dialog state moves to `state/overlays.ts`; `applySettingsEdits` → `settings/edits.ts`; `settingsProblem` kept | U5 |
| `settings/SettingsProblemBox.tsx`, `diagnostics.ts` | restyle/adapt | Diagnostics take `project?` | U5 |
| `shell/*` (ActivityBar, Panes, Shell, Sidebar, StatusBar, TitleBar, Workbench, slots, closeFolder.test, useCompile, useShellActions, *.css) | delete | Global actions → `app/actions.ts`; Compile → `state/compile.ts` | U1c |
| `shell/status.ts` (+test) | move+adapt | `languageList`, `plural`, `progressText`, `fallbackReasonText` → `lib/format.ts` and `home/stateLine.ts` | U1a (format), U3 |
| `state/boards/types.ts`, `document.ts` | rewrite | Pages, project-qualified refs, links, labels/guards arrays (§3.4) | U1b |
| `state/boards/store.ts` (+test) | adapt | Open/claim, edit = one undo step, coalescing, autosave (600 ms, never overlapping), conflict rule, reload, server-state adoption, sessions — all kept; keyed by `BoardRef`; per-page view state; saves `BoardEdit` | U1b |
| `state/boards/api.ts`, `navigation.ts` (+test), `health.ts`, `files.ts` (+test), `transfer.ts` (+test), `engine.ts` | adapt | Per page and project; files ops per contract §4.6 | U1b |
| `state/boards/wiring.ts` | rewrite | → `state/boards/events.ts` (conflict, health refresh); reconnect → `state/session.ts` | U1b, U6 |
| `state/compile.ts` | rewrite | Per project (§3.11) | U1b |
| `state/layout.ts` (+test), `tabs.ts` (+test), `window.ts` | rewrite | §3.3 | U1b |
| `state/persistence.ts` (+test) | rewrite | → `state/session.ts` (restore, `save_window_state`, reconnect) | U6 |
| `state/lens.ts`, `selection.ts` | delete | Lens is per page (V-55); selection is per page view | U1b |
| `test/*` | keep | Fake engine reads re-blessed goldens; add `test/workspace.ts` builders | U1b |
| `theme/tokens.css`, `base.css`, `contrast.test.ts` | rewrite | §5 | U1a |
| `theme/theme.ts` | keep | Fonts imported here (already bundled) | U1a |
| `views/search/highlight.tsx` | move | → `add/highlight.tsx` | U4 |
| `views/search/textSearch.ts` | move+adapt | → `add/textSearch.ts` on `ipc/stream.ts` | U4 |
| `views/entrances/EntrancesView.tsx` | delete | `methodColumn`, `triggerText` → `lib/entrances.ts` (Home and Add share them) | U1a |
| `views/files/model.ts` | delete | `SKIP_REASONS` → `settings/scope.ts` | U5 |
| `views/boards/OpenBoardSections.tsx` | delete | `outlineRows` → `board/onThisPage.ts` | U2 |
| `views/glyphs.tsx` | delete | `KindGlyph` → `components/KindGlyph.tsx` | U1a |
| `views/actions.ts`, everything else in `views/` | delete | `openInEditor`, `revealInFileManager` → `lib/files.ts` | U1a |
| `welcome/time.ts` | move | → `lib/format.ts` `relativeTime` | U1a |
| `welcome/FirstCompile.tsx` | delete | `languageSentence`, `skippedClauses` → `home/notCompiled.ts` | U3 |
| `welcome/` (rest) | delete | | U1c |
| `package.json` | adapt | Remove `cmdk` (U1c) and `lucide-react` (U2, once canvas and flow use `icons/`). Keep every other dependency | U1c, U2 |

---

## 2. Target module layout

```text
ui/src/
  main.tsx            boot
  app/                window frame: tab strip, sidebar container, content, overlays host, global actions, drop
  theme/              tokens.css, base.css, theme.ts (fonts)
  icons/              the one 16px icon set (V-22) and <Icon>
  components/         shared kit (§6)
  lib/                pure helpers: format, entrances, files, keys
  ipc/                client, commands, queries (keys + invalidation), stream, hooks, generated/
  keyboard/           registry, dispatcher, keymap, bindings, scopes, menu routing
  state/              workspace, window, tabs, layout, overlays, compile, author, session, boards/
  home/               Home sidebar + Home main (every state), thumbnails, health and activity
  board/              board sidebar (header, Pages, On this page, Selected), top bar, dock, BoardView
  canvas/             React Flow canvas, cards/, links, placeholders, layout
  code/  flow/        reader renderers (kept)
  add/                Add box: map, search, preview, commands
  peek/               source peek sheet
  settings/           project settings dialog, entrance editor
  preferences/        preferences dialog
  dialogs/            DialogHost; publish, discard, delete, folder dialogs
  menus/              board menu, share menu, project menu, worktree switcher, project actions
  test/               test helpers
```

| Module | Responsibility | Public exports (import only these from outside the module) |
| --- | --- | --- |
| `app/` | `<App/>`: providers, window wiring (dispatcher, menu commands, invalidation install, shell events), frame (`TabStrip`, `Sidebar` container in home or board mode, `Content`, `Overlays`), drop-a-folder, global actions | `App` |
| `theme/` | Tokens, base styles, `applyThemeOverride` | `applyThemeOverride`, `ThemeOverride` |
| `icons/` | `Icon`, `IconName` | `Icon`, `IconName`, `ICON_NAMES` |
| `components/` | §6 | `components/index.ts` |
| `lib/` | `relativeTime`, `plural`, `languageList`, `displayPath`, `formatCount`; `methodColumn`, `triggerText`; `openInEditor`, `revealInFinder`; `pageKey`, `cardRefKey` | `lib/*.ts` named exports |
| `ipc/` | §4 | `call`, `callStream`, `onEngineEvent`, `windowCommand`, `onShellEvent`, `engineKey`, `engineQueryOptions`, `useEngineQuery`, `installEngineInvalidation`, `resetEngineQueries`, `createLatestStream`, `useConnectionState`, `queryClient`, `UnfoldError`, `toUnfoldError`, generated types |
| `keyboard/` | §8 | as V1 `index.ts`, plus `ActionIds` (new), `ADD_GROUPS` |
| `state/` | §3 | `state/workspace`, `window`, `tabs`, `layout`, `overlays`, `compile`, `author`, `session`, `boards/index` |
| `home/` | Home-mode sidebar and Home main | `HomeSidebar`, `HomeMain` |
| `board/` | Board-mode sidebar and the board view | `BoardSidebar`, `BoardView` |
| `canvas/` | Canvas, cards, links, placeholders, geometry | `BoardCanvas`, `layout.ts` constants and functions, `cardQuery`, `ProjectScope` |
| `add/` | Add box; registers `add.*` actions | `AddBox` |
| `peek/` | Peek sheet; registers `peek.*` actions | `PeekSheet` |
| `settings/` | Project settings, entrance editor, diagnostics | `ProjectSettingsDialog`, `EntranceEditorDialog`, `RemoveEntranceDialog`, `copyDiagnostics` |
| `preferences/` | Preferences | `PreferencesDialog`, `usePreferencesTheme`, `usePreferencesKeyBindings` |
| `dialogs/` | One host for every `DialogRequest` | `DialogHost` |
| `menus/` | Menus and the project-level actions | `BoardMenuButton`, `boardMenuEntries`, `shareMenuEntries`, `ProjectMenuButton`, `projectMenuEntries`, `WorktreeButton`, `useProjectActions` |

Import rules: screens (`home`, `board`, `add`, `peek`, `settings`, `preferences`, `dialogs`, `menus`) import from
`state/`, `ipc/`, `components/`, `icons/`, `lib/`, `keyboard/`, and each other's **public exports only**. Nothing
imports `app/`. `state/` imports no screen module (it may import `canvas/layout.ts`, `canvas/cardData.ts`).

---

## 3. State model

### 3.1 Stores at a glance

| Store | Kind | Holds | Saved to |
| --- | --- | --- | --- |
| `workspace` query | TanStack Query (`["engine","workspace",{}]`) | `WorkspaceView` | Rust (`workspace.json`) |
| `useWindowStore` | zustand | `info {windowId, key, platform}`, `selectedProject` | `windows.json` (`selectedProject`) |
| `useTabs` | zustand + immer | board tab refs in order, active tab | `windows.json` (`tabs`, `activeTab`) |
| `useLayout` | zustand + immer | `WindowLayout` (§3.9) | `windows.json` (`layout`) |
| `useOverlays` | zustand | Add request, peek target, dialog request | not saved |
| `useBoards` | zustand + immer | open board sessions (§3.4) | `save_board` (document), `save_board_view` (view) |
| `useCompile` | zustand | per-project running compile and progress | not saved |
| everything else from Rust | TanStack Query | §4.2 | — |

### 3.2 Workspace, projects, and name resolution (`state/workspace.ts`)

```ts
useWorkspace(): WorkspaceView | undefined            // useEngineQuery("workspace", {}, { refetchOnWindowFocus: true }) — D-4
useProjectRow(id: ProjectId | null): ProjectRow | undefined
getWorkspace(): WorkspaceView | undefined             // from the cache
interface ProjectIndex {                              // memoized per WorkspaceView identity
  rows: readonly ProjectRow[];
  byId: ReadonlyMap<ProjectId, ProjectRow>;
  byName: ReadonlyMap<ProjectName, ProjectId>;        // first row in workspace order wins (D-3)
}
useProjectIndex(): ProjectIndex
resolveRefProject(index, boardProject: ProjectId, refProject: ProjectName | undefined): ProjectId | null
  // undefined -> boardProject; a name -> byName.get(name) ?? null (null = not connected: placeholder)
refProjectFor(index, boardProject, cardProject): { ok: true; project?: ProjectName } | { ok: false; reason: "same_name" }
  // cardProject === boardProject -> {ok, project: undefined}; else its name, refused when it equals the board
  // project's name (V-62)
selectProject(id: ProjectId | null): Promise<void>    // sets useWindowStore.selectedProject, then select_project
addProjectFromPicker(): Promise<ProjectId | null>     // windowCommand("add_project"); selects the result
readdProject(path: string): Promise<ProjectId | null> // readd_project (A-1); selects the result
setProjectExpanded(id, expanded): Promise<void>        // set_project_expanded (workspace_changed replaces data)
```

- The board document's `projects` is read-only for the UI (Rust derives it); a card's project is always
  `resolveRefProject(...)` against the **current** index, so connecting or renaming a project re-resolves cards at
  once (`workspace_changed` replaces the index).
- The selected project is per window (D-25). Boot default: the window's saved `selectedProject` (U6), else
  `workspace.selected`, else the first row, else null. Selection never changes because a board tab is active.

### 3.3 Window, tabs, layout

```ts
// state/window.ts
useWindowStore: { info: WindowInfo | null; selectedProject: ProjectId | null }
useSelectedProject(): ProjectId | null

// state/tabs.ts — Home is implicit, always first, never closable
type TabKey = "home" | string                         // board tabs: boardKey(ref) = `${project}\u0000${id}`
interface TabsState { boards: BoardRef[]; active: TabKey }
useTabs(); activateHome(); activateBoard(ref); openBoardTab(ref, { activate?: boolean; index?: number })
closeTab(key): Promise<void>                          // board: flushBoard, save_board_view, close_board, then remove
closeOtherTabs(key): Promise<void>; nextTab(); previousTab(); moveTab(key, index)
renameBoardTab(from: BoardRef, to: BoardRef)          // publish / move back to drafts change the id
useActiveBoard(): BoardRef | null
// at most 100 board tabs (contract §2.2): a 101st open is refused with a toast

// state/layout.ts — WindowLayout, §3.9
useLayout(); setSidebarWidth(px); toggleSidebar(); setSelectedCollapsed(b); setPeekPinned(b); setHomeView(v)
setBoardsFilter(f); setBoardsSort(s); setFolderExpanded(project, path, b)
layoutSnapshot(): WindowLayout; applyLayoutSnapshot(json: unknown); subscribeToLayout(fn)

// state/overlays.ts
type AddKind = "entrances" | "functions" | "files" | "text"
interface AddRequest { mode: "find" | "commands"; kind?: AddKind; query?: string }
type AddTarget = { kind: "page"; target: PageTarget } | { kind: "home"; project: ProjectId | null }
openAdd(request?: AddRequest); closeAdd(); useAddRequest(): AddRequest | null
useAddTarget(): AddTarget                              // active board's active page, else Home + selected project
interface PeekTarget { project: ProjectId; path: string; line?: number; matches?: readonly TextMatch[]; current?: number }
openPeek(target); closePeek(); usePeekTarget(): PeekTarget | null       // pinned lives in layout
type DialogRequest =
  | { kind: "preferences" }
  | { kind: "projectSettings"; project: ProjectId; section?: "general" | "scope" | "detectors" | "python" }
  | { kind: "entranceEditor"; project: ProjectId; mode: "add"; handler?: DefinitionRef }
  | { kind: "entranceEditor"; project: ProjectId; mode: "edit"; entrance: ManualRow }
  | { kind: "removeEntrance"; project: ProjectId; entrance: ManualRow }
  | { kind: "publish"; board: BoardRef }
  | { kind: "discard"; board: BoardRef }
  | { kind: "deleteBoard"; board: BoardRef }
  | { kind: "newFolder"; project: ProjectId; parent: string | null }
  | { kind: "renameFolder"; project: ProjectId; path: string }
  | { kind: "groupFromLines"; target: PageTarget; cardId: string }
openDialog(request); closeDialog(); useDialog(): DialogRequest | null
```

### 3.4 Board sessions (`state/boards/`)

```ts
interface OpenBoard {
  readonly ref: BoardRef;
  place: "published" | "draft"; name: string;
  document: BoardDocument;               // working version (title, folder, projects, pages); folder/projects read-only
  revision: string;
  published: BoardState["published"];   // at last open/save/publish; the header reads list_boards (§7.2)
  health: BoardHealth; outlines: Record<PageId, PageOutline>; label: string | null;
  edits: number; savedEdits: number; saving: boolean; saveError: string | null;
  conflict: { diskRevision: string | null } | null;
  history: { past: HistoryEntry[]; future: HistoryEntry[]; coalesce: { key: string; at: number } | null };
  activePage: PageId;
  pages: Record<PageId, PageView>;       // created lazily per page shown
  viewDirty: boolean;                    // BoardView changed since the last save_board_view
}
interface HistoryEntry { document: BoardDocument; page: PageId }     // V-54: undo returns to that page
interface PageView {                     // V1 BoardViewState, per page
  lens: View | null;                     // V-55
  camera: Camera | null;
  selection: Selection;
  codeFolds: Record<string, string[]>; expanded: Record<string, string[]>; flowChains: Record<string, string[]>;
  lensExceptions: string[];
  trail: TrailItem[]; back: FocusMove[]; forward: FocusMove[];
  sizes: Record<string, CardSize>;       // measured; survives unmounting (V-46)
  focus: FocusRequest | null; pendingGroup: PendingGroup | null; placement: PendingPlacement | null;
  linking: { from: string } | null;      // V-58 link drawing mode
}
type Selection =
  | { kind: "none" }
  | { kind: "card"; cardId: string; key: Key | null }    // key = a Flow node or statement inside the card
  | { kind: "group"; groupId: string }
  | { kind: "link"; linkId: string };
```

Rules ported from V1 unchanged: one board is open in one window (claim on `open_board`/`new_board`; `held` →
`focus_window`); every edit is one undo step, bursts with a `coalesce` key within 1,200 ms merge; history ≤ 200
steps; `amendBoard` folds a placement into the step that opened the cards; `replaceEqualDeep` keeps identities so
untouched cards don't re-render; reload is undoable; repairs are adopted as one undo step (`applyServerState`).

Card, group, and link ids are unique together within their page (`uniqueId(page, base)`, slug ≤ 100 chars, contract
§1.4). Page ids: `page-<n>` with the smallest free `n`.

### 3.5 Edit → `BoardEdit` → autosave

```text
editBoard(ref, recipe, {coalesce?, page?})          produce(document) → push {document: before, page} → edits++
  └─ scheduleSave(ref) 600 ms after the last edit
saveBoard(ref, {force?})                             waits for any save in flight (never two at once)
  ├─ skip if clean, or if conflict and !force
  ├─ call("save_board", {project, id, board: {title: document.title, pages: document.pages}, force?})
  ├─ ok: applySaved (revision, health, outlines, label, place, name; adopt the signed document only if no edit
  │      happened meanwhile; published counts); savedEdits = covered
  ├─ board_conflict: conflict ??= {diskRevision: null}            (V1)
  └─ other errors: saveError + toast; disconnected/engine_unavailable stay dirty for the reconnect
```

- `BoardEdit` never carries `folder`, `projects`, `format`, `version`, `publishes`, or `baseRevision` (D-12).
  `labels`, `guards`, `links` are sent as the document holds them; label and link-label `signature` is `""` for new
  or changed text (Rust signs).
- `board_conflict {board, file, diskRevision}` (targeted): no unsaved edits → `reload_board`; else `conflict`
  → the banner's Keep mine (`save_board {force: true}`) or Take theirs (`reload_board`) — contract §6.6.
- A published board's first save creates its changes copy; its id stays (D-5). Tabs, claims, recents, and view
  state follow the id when Publish or Move back to drafts changes it: `adoptBoardState(state, {replaces})` +
  `renameBoardTab`.
- Health: after each save from the result; after `graph_updated` of any project the board's cards resolve to,
  `board_health {project, id}` (debounced 250 ms, per board) refreshes `health`, `outlines`, `label`.
- `pagehide`: `saveAllNow()` sends every dirty board and pending view (no awaiting), as V1.

### 3.6 View state (`save_board_view`)

- On `open_board`: `pages[p] = {camera: view.pages[p] ? {x,y,zoom} : null, lens: view.pages[p]?.lens ?? null, …}`;
  `activePage = windowTab.page ?? view.page ?? document.pages[0].id`.
- `viewDirty` is set by camera move end, lens change, and page switch; a debounced (1,000 ms) save sends
  `{page: activePage, pages: {id: {x, y, zoom, lens}}}` for pages that have a camera or lens (≤ 200). Flushed before
  `close_board` and on `pagehide`. View changes are never undo steps.

### 3.7 Cross-project cards (D-3)

- Rendering: card `c` on a board of project `P` → `resolveRefProject(index, P, c.ref.project)`. `null` → placeholder
  node (V-35, V-61), outline row muted with "not connected". Otherwise `card {project, key}` for that project.
- Adding: `addBlock`/`openEntrance` take the result's `ProjectId`; `refProjectFor` turns it into `ref.project`
  (omitted for `P`), refusing a same-name project (V-62).
- Relations, callers, peek, preview, `onPage` keys: always with the card's resolved project. On-page marks use
  `cardRefKey = ${projectId}\u0000${key}`.
- Arrows (graph follow edges) are computed only between cards resolving to the same project; links are drawn.
- `workspace_changed` (connect, rename, remove) → index changes → cards re-resolve without a reload.

### 3.8 Engine restart (contract §3.6, owned by U6 in `state/session.ts`)

```text
connection: reconnecting/unavailable → persistent toast "Reconnecting to the engine…" (V-78); saves stay dirty
connection: connected (after a loss) →
  1. select_project(selectedProject)
  2. for each board tab, in order: open_board → held: drop the tab + toast; revision differs: handleOutsideChange
  3. save every dirty board (saveBoard), pending save_board_view, then save_window_state
  4. resetEngineQueries(client)   // every engine query except get_preferences, app_info: reset + refetch active
  5. dismiss the toast
```

`ipc/queries.ts` installs no reconnect refetch of its own (V1's `refetchAfterRestart` is deleted): only this
sequence orders it.

### 3.9 `windows.json` (`save_window_state`, U6)

```ts
call("save_window_state", { state: {
  key: info.key,
  selectedProject,                                  // ProjectId | null
  tabs: boards.map(ref => ({ project: ref.project, board: ref.id, page: openBoard(ref)?.activePage ?? null })),
  activeTab: active === "home" ? null : index of the active board tab,
  layout: WindowLayout,
}})
interface WindowLayout {                            // V-79; ≤ 2 KiB in practice (64 KiB cap)
  v: 1;
  sidebar: { width: number; visible: boolean };     // width 220–360, default 264
  selectedCollapsed: boolean;
  peek: { pinned: boolean };
  home: { view: "overview" | "health" | "activity" | "recents" | "drafts";
          boardsFilter: "all" | "published" | "drafts"; boardsSort: "edited" | "title" };
  folders: string[];                                // expanded sidebar folders, `${project}\u0000${path}`, ≤ 500
}
```

Saved 400 ms after any change of window store, tabs, layout, or an open board's `activePage`; flushed on `pagehide`.
Restore (launch, reload): `window_state {key}` → `applyLayoutSnapshot` → `selectProject(saved.selectedProject ??
workspace.selected)` → `open_board` per tab (in parallel; `held`/error drops the tab) with its `page` → activate
`activeTab`. Transfers wait until restore is done (V1 `setTransfersReady`). No lens in `layout` (V-55).

### 3.10 Move to New Window (`state/boards/transfer.ts`, ported in U1b)

```ts
interface BoardSession {                          // transfer_board_state payload, plain JSON
  state: BoardState;                              // as last saved (project, id, place, name, document, revision, …)
  document: BoardDocument;                        // unsaved working version
  history: { past: HistoryEntry[]; future: HistoryEntry[] };
  activePage: PageId;
  pages: Record<PageId, { camera: Camera | null; lens: View | null }>;
}
moveBoardToNewWindow(ref): flushBoard (failure kept as dirty) → windowCommand("move_tab_to_new_window") →
  call("move_board", {project, id, toWindow}) → windowCommand("transfer_board_state", {toWindow, payload}) →
  forgetBoard(ref) (tab removed)
receiver: onShellEvent("board_state_transferred") → parseSession (structural checks) → queue until restore is
  done → adoptSession (dirty → scheduleSave) → openBoardTab(ref, {activate: true})
```

### 3.11 Compile (`state/compile.ts`)

```ts
startCompile(project, mode: "check" | "from_scratch" = "check"): Promise<void>
  // callStream("compile", {project, mode}); one running per project per window; cancelled is not an error
cancelCompile(project): Promise<void>            // handle.cancel() + call("cancel_compile", {project})
setBackgroundUpdates(project, on): Promise<void>
useCompileProgress(project): { stage: string; done: number; total: number } | null
  // from this window's stream or `compile_progress` events; cleared when project_state leaves compiling/updating
useCompileStartedHere(project): boolean          // V-67 compiled toast
```

---

## 4. IPC client

### 4.1 Keys and calls

- Every engine read is a query keyed `["engine", command, args]` (`engineKey`). Args objects are built with
  stable shapes; TanStack hashes keys with sorted object keys. Every project command's args carry `project`, so a
  predicate tests `args.project === event.project`.
- Defaults (kept): `staleTime: Infinity`, `retry: false`, `refetchOnWindowFocus: false` — data changes only when an
  event says so. Exceptions: `workspace` refetches on window focus (D-4); `git_info` has `staleTime: 0` (fetched when
  the switcher opens, V-80).
- Mutations are plain `call(...)` from `state/` functions; their effects arrive as events (`boards_changed`,
  `workspace_changed`, `config_changed`) or are adopted from the result (`save_board` → store). No mutation
  invalidates by hand except `update_project_config` (adopts the returned `ProjectConfig` into its query).

### 4.2 Queries (who reads what)

| Command | Liveness | Read by | Notes |
| --- | --- | --- | --- |
| `workspace` | — | sidebar, tabs, Add, everything resolving names | replaced by `workspace_changed`; rows patched by `project_state` |
| `drafts` | rests | Home sidebar count, Home Drafts view | |
| `project_summary` | wakes | Home header, state line, Health strip, settings General notes | selected project only |
| `compile_preview` | wakes | Home not-compiled | |
| `scope_dir` | wakes | first-compile scope, settings Scope | one level per expanded folder |
| `entrances` | rests | Home How it starts (`perApplication: 8`; expanded app: `application`, `filter`, 500); Add map (`perApplication: 12`; expanded lane 500) | |
| `problems`, `index_log` | wakes | Home Health / Activity | |
| `project_config` | rests | settings, entrance editor, manual entrance menus | |
| `git_info` | rests | worktree switcher (on open) | |
| `list_boards` | rests | sidebar tree (expanded projects), Home Boards, board header publish state and folder, Move to folder | |
| `board_thumbnails` | rests | Home Boards, Recents, Drafts | chunks of ≤ 60 ids (§9) |
| `board_refs` | rests | Add map popover "Already on …" | |
| `card` | wakes | cards | `{project, key}`; never `board` (V-56) |
| `peek` | wakes | call-link hover cards | |
| `relations` | wakes | Selected (card) | `onPage` = keys of that project's cards on the page |
| `preview` | wakes | Add preview pane, map popover | debounced (V-63, V-64) |
| `file_view` | wakes | peek sheet pages (`range` of 200 lines), Add file/text previews (`range`) | |
| `search` | rests | Add search | `kinds`, `limit`, `projects` per V-63 |
| `get_preferences`, `app_info` | — | theme, bindings, author, workspace name | app-wide |
| `diagnostics` | — | Copy diagnostics (one-shot `call`, not a query) | |

Streams (never queries): `compile`, `search_text`.

### 4.3 Invalidation (`installEngineInvalidation`, exactly contract §5)

`P` = `event.project`; "(P)" = queries whose `args.project === P`; "incl. P" = `search` queries whose
`args.projects` is absent or contains `P`.

| Event | Cache effect |
| --- | --- |
| `workspace_changed {workspace}` | `setQueryData(workspace)` (no refetch) |
| `project_state {project, state}` | patch the row's `state` in `workspace` data; invalidate `project_summary` (P) |
| `project_reset {project}` | `removeQueries` (P); forget last-verified renders of P |
| `graph_updated` with `everything` | invalidate every query (P), `search` incl. P; refetch `workspace` (D-4) |
| `graph_updated` (files, keys, entrances) | (P): `file_view` by `path` ∈ changed∪removed; `card` by `key` ∈ keys or `keyFile(key)` ∈ files, plus every entrance `card`; `peek` by key/file; `relations`, `callers`, `preview`, `project_summary`, `index_log`, `board_thumbnails`; `entrances` when `changedEntrances` non-empty or `inputsChanged`. `search` incl. P. Store: debounced `board_health` of open boards with a card resolving to P |
| `files_changed {paths, truncated}` | (P): truncated → every `file_view` and `card`; else `file_view` by path, `card` by file. `search` incl. P whose `kinds` is absent or has `files` |
| `boards_changed {boards, folders}` | `list_boards` (P); `board_thumbnails` (P) whose `args.boards` ∩ `boards` ≠ ∅; `drafts`; every `board_refs`; `project_summary` (P) |
| `problems_changed` | `problems` (P), `project_summary` (P) |
| `config_changed` | `project_config` (P), `compile_preview` (P), `scope_dir` (P) |
| `compile_progress` | not the cache: `useCompile` progress for P |
| `preferences_changed {preferences}` | `setQueryData(get_preferences)` |
| `board_conflict` | not the cache: board store (§3.5) |

No handler calls `invalidateQueries()` without a predicate. Invalidation refetches only active queries; inactive ones
refetch when next observed.

### 4.4 Streams and cancellation (`ipc/stream.ts`)

```ts
createLatestStream<C extends StreamCommandName>(command: C): {
  run(args: CommandArgs<C>, onMessage: (m: StreamMessage<C>) => void): Promise<CommandResult<C>>;
  cancel(): void;            // cancels the running one; its late messages are dropped (request generation check)
}
```

- `search_text`: one `createLatestStream` per Add box; each query change cancels the previous request
  (`cancel(requestId)` through the handle) before starting; messages carry the generation they belong to.
- `compile`: `state/compile.ts`; Cancel = handle cancel + `cancel_compile {project}` (also cancels one started
  elsewhere).

### 4.5 Window commands and shell events

`windowCommand` (contract §7.3): `new_window`, `focus_window`, `close_window`, `add_project`,
`confirm_remove_project`, `export_mermaid`, `move_tab_to_new_window`, `transfer_board_state`, `copy_to_clipboard`,
`window_info`. Drop: `window.unfold.addDroppedFolder(file)` from `app/` on a drop anywhere (first launch's zone
included).

| Shell event | Handler (owner) |
| --- | --- |
| `menu_command {command}` | `keyboard/menu.ts` → action id (§8.3) (U1b) |
| `board_state_transferred` | transfer receiver (U1b) |
| `focus_board {project, board}` | activate that board's tab (U6) |
| `project_added {project}` | `selectProject(project)`, `activateHome()` (U1c) |

---

## 5. Theme tokens

`theme/tokens.css` keeps V1's three blocks (`:root` light, `@media (prefers-color-scheme: dark)
:root:not([data-theme="light"])`, `:root[data-theme="dark"]`, the last two identical). Chrome values are the
designs' (V-13, V-14, V-24, V-28..V-31, V-40); reader tokens (syntax, Flow, `--color-failure`,
`--color-decision`) keep their values (V-28). Fonts: Inter and JetBrains Mono are already bundled
(`@fontsource-variable/inter` 5.3.0, `@fontsource-variable/jetbrains-mono` 5.3.0, imported in `theme/theme.ts`); keep
them. Code text sets `font-feature-settings: "calt" 0, "liga" 0` (09 §5.2).

### 5.1 Color tokens

| Token | Light | Dark | Used for |
| --- | --- | --- | --- |
| `--color-canvas` | `#F7F8FA` | `#151619` | Canvas, window root |
| `--color-home` | `#FFFFFF` | `#151619` | Home main area (V-42) |
| `--color-sidebar` | `#F7F8FA` | `#191B1F` | Sidebar (V-29) |
| `--color-chrome` | `#FFFFFF` | `#191B1F` | Top bar, active tab |
| `--color-tabstrip` | `#F0F1F4` | `#111214` | Tab strip (V-29) |
| `--color-surface` | `#FFFFFF` | `#1D1F23` | Cards, dock, menus, popovers, dialogs, sheet, toasts |
| `--color-control` | `#FFFFFF` | `#2D3036` | Secondary buttons, active segments, card mini-lens active |
| `--color-track` | `#ECEDF0` | `#111214` | Window lens track |
| `--color-track-soft` | `#F3F4F6` | `#151619` | Card mini-lens track, Health/Activity switch track |
| `--color-pressed` | `#EEF0F3` | `#2D3036` | Open/pressed icon button, active filter pill, fold pill |
| `--color-hover` | `#F3F5FD` | `#22263A` | Row and item hover (V-8, V-41) |
| `--color-selection` | `#EDF0FC` | `#2A304B` | Selected rows, menu highlight, selected link chip |
| `--color-selection-strong` | `#DCE1F5` | `#353B5C` | Monogram on a selected result row |
| `--color-selection-line` | `#D5D9F2` | `#3B4166` | Keycap border on a selected row |
| `--color-match` | `#DDE1F7` | `#353C66` | Matched text in Add text rows |
| `--color-text` | `#202126` | `#EDEEF2` | Primary text |
| `--color-muted` | `#656974` | `#A4A8B3` | Secondary text, resting icons (V-13) |
| `--color-subtle` | `#64748B` | `#8B8F99` | Line numbers |
| `--color-faint` | `#94A3B8` | `#5F636D` | Disabled icons, chevrons, Draft dot, moon |
| `--color-border` | `#E0E2E7` | `#353840` | Controls, cards, dock, menus, dialogs, inner rules (V-31) |
| `--color-border-shell` | `#E0E2E7` | `#2B2E34` | Tab strip, sidebar edge, top bar, Selected rule, active tab |
| `--color-card-rule` | `#E0E2E7` | `#2E3138` | Card header divider |
| `--color-card-rule-soft` | `#F0F1F4` | `#2A2C32` | Description divider, application overflow rule |
| `--color-placeholder` | `#F1F2F5` | `#1A1C20` | Placeholder card fill |
| `--color-placeholder-line` | `#B6BCC8` | `#4A4E57` | Placeholder dashed border, unselected radio |
| `--color-dashed` | `#C9CDD6` | `#3A3D45` | Drop zone, draft thumbnail border |
| `--color-accent` | `#5862C0` | `#A0AAFF` | Primary fills, checked boxes, switch on, selected card border (V-30) |
| `--color-accent-text` | `#5862C0` | `#A0AAFF` | Links, selected icons, focus |
| `--color-on-accent` | `#FFFFFF` | `#151619` | Text and icons on accent fills |
| `--color-on-accent-soft` | `#D9DCF5` | `rgb(21 22 25 / 0.62)` | Shortcut text on primary buttons |
| `--color-on-accent-chip` | `rgb(255 255 255 / 0.18)` | `rgb(21 22 25 / 0.14)` | Key chip on the Add button |
| `--color-accent-ring` | `rgb(88 98 192 / 0.12)` | `rgb(160 170 255 / 0.16)` | Selection ring |
| `--color-accent-soft` | `#A0A8E0` | `#5A62A8` | Other-match ticks on the peek scrollbar |
| `--color-match-current` | `rgb(88 98 192 / 0.22)` | `rgb(160 170 255 / 0.22)` | Current peek match (ring = same at 0.45) |
| `--color-arrow` | `#5862C0` | `rgb(160 170 255 / 0.70)` | Arrows to opened calls |
| `--color-focus-ring` | `#5862C0` | `#A0AAFF` | `:focus-visible` |
| `--color-review` | `#D97706` | `#F59E0B` | Review dots and icons, problems dot (V-27, V-39) |
| `--color-warn` | `#8A5A00` | `#F5C66A` | Review banner text, Health strip item, conflict box |
| `--color-warn-fill` | `#FFF3D6` | `rgb(217 119 6 / 0.16)` | Review banner, conflict box |
| `--color-success` | `#16A34A` | `#4ADE80` | "Graph current" dot only |
| `--color-danger` | `#DC2626` | `#F87171` | Destructive labels, `DELETE`, failed triangle (V-11) |
| `--color-on-danger` | `#FFFFFF` | `#151619` | Text on danger buttons |
| `--color-authored` | `#A8346F` | `#F2A9CC` | Descriptions, labels |
| `--color-authored-tint` | `#FBEFF5` | `rgb(168 52 111 / 0.18)` | Description box in Selected |
| `--color-authored-line` | `#E7B1CC` | `#7A3D5C` | Group rail (kept) |
| `--color-step` | `#F4F5F8` | `#24262C` | Rows that send an arrow |
| `--color-contour` | `#8B93A1` | `#5F636D` | Entrance→handler arrow, teammate links, unchecked box |
| `--color-unresolved` | `#8B93A1` | `#6B6F7A` | Dashed underline of unresolved calls |
| `--color-syntax-external` | `#DB2777` | `#D59BE8` | External call names in code (V-34) |
| `--color-monogram` / `-ink` | `#E6E8EC` / `#3F4451` | `#2D3036` / `#C9CCD4` | Project monogram (V-24) |
| `--color-workspace-mark` / `-ink` | `#202126` / `#FFFFFF` | `#EDEEF2` / `#151619` | Workspace monogram |
| `--color-scrim` | `rgb(15 23 42 / 0.18)` | `rgb(0 0 0 / 0.45)` | Dialog and Add scrim |
| `--color-scroll-thumb` | `rgb(15 23 42 / 0.14)` | `rgb(255 255 255 / 0.16)` | Scrollbars |
| `--shadow-float` | `0 1px 2px rgb(15 23 42 / .06), 0 8px 24px rgb(15 23 42 / .10)` | `0 1px 2px rgb(0 0 0 / .40), 0 12px 32px rgb(0 0 0 / .55)` | Dock, menus, popovers, sheet, dialogs, toasts |
| `--shadow-segment` | `0 1px 2px rgb(15 23 42 / .08)` | `0 1px 2px rgb(0 0 0 / .40)` | Active lens segment |
| `--shadow-ring` | `0 0 0 3px var(--color-accent-ring)` | same | Selected card, selected link chip |

Unchanged reader tokens: `--color-syntax-{plain,keyword,function,property,string,type,number,comment}`,
`--color-decision(-fill)`, `--color-data(-fill)`, `--color-guard(-fill)`, `--color-loop(-line)`, `--color-failure`,
`--color-failure-fill`. Deleted V1 tokens: `--color-popover`, `--color-accent-strong`, `--color-pending`,
`--color-danger-strong`, `--color-tooltip`, `--color-on-tooltip`, `--color-muted-on-tooltip`, `--color-overlay`,
`--shadow-raised`, `--shadow-popover`, `--radius-small`, `--radius`, `--radius-large`, `--font-size-*` (renamed below;
U1a renames every use in every CSS file).

### 5.2 Type, shape, size, layers, motion

| Group | Tokens |
| --- | --- |
| Fonts | `--font-ui: "Inter Variable", system-ui, sans-serif`; `--font-code: "JetBrains Mono Variable", ui-monospace, monospace` |
| Sizes / leading | `--text-11/14`, `--text-12/16`, `--text-13/16`, `--text-13/20`, `--text-15/20`, `--text-15/22`, `--text-16/24` (Add input), `--text-17/24` (preview title), `--text-32/40` (project name, first-launch headline): each a `font-size` + `--leading-*` pair |
| Tracking | `--tracking-caps: 0.06em`, `--tracking-title: -0.01em`, `--tracking-display: -0.03em` |
| Radii | `--radius-chip: 4px`, `--radius-mono: 5px`, `--radius-control: 6px`, `--radius-tab: 8px`, `--radius-card: 10px`, `--radius-dialog: 12px`, `--radius-full: 999px` (V-14) |
| Sizes | `--row: 28px`, `--row-result: 32px`, `--tabstrip: 38px`, `--tab: 30px`, `--topbar: 48px`, `--sidebar-default: 264px`, `--sidebar-min: 220px`, `--sidebar-max: 360px`, `--peek-width: 640px` |
| Layers | `--z-resize: 5`, `--z-sheet: 30`, `--z-popover: 40`, `--z-dialog: 50`, `--z-menu: 60`, `--z-tooltip: 70`, `--z-toast: 80` |
| Motion | `--duration-fast: 90ms`, `--duration: 160ms`; none under reduced motion (base.css, kept) |

`contrast.test.ts` pairs added: `muted`/`tabstrip`, `text`/`chrome`, `on-accent`/`accent`, `muted`/`home`,
`accent-text`/`selection`, `warn`/`warn-fill`, `danger`/`surface`, `review`/`sidebar` (3:1), `success`/`home` (3:1),
`faint`/`surface` (3:1 icons). Values with alpha are not checked (as V1).

---

## 6. Shared components

All in `components/` (U1a) unless noted; data-independent; styled only with §5 tokens; exported from
`components/index.ts`. Icons are `IconName` from `icons/`.

| Component | Props (beyond standard HTML attributes) | Spec |
| --- | --- | --- |
| `Icon` (`icons/`) | `name: IconName; size?: 16 \| 14 \| 12 \| 10; className?` | V-22 set: `home, chevron-left, chevron-right, chevron-down, chevron-right-small, chevron-down-small, dots, plus, page, entrance, group, link, search, clock, drafts, folder, board, board-draft, branch, check, gear, arrange, external-link, reveal, pin, close, add-circle, warning-circle, warning-triangle, info, shield, publish, move-back, copy, activity, edit, layers, minus, back, forward`. `stroke=currentColor`, 1.5 |
| `Button` | `variant: "primary" \| "secondary" \| "ghost" \| "danger"; size?: "md" \| "sm"; icon?: IconName; trailingIcon?: IconName; shortcut?: string; busy?: boolean` | components §7.1–7.3; `shortcut` renders platform label in `--color-on-accent-soft` on primary |
| `IconButton` | `icon: IconName; label: string; size?: 28 \| 24 \| 20; pressed?: boolean; shortcut?: string; tooltipSide?: Side` | §7.4; tooltip = label + shortcut |
| `TextButton` | `children; weight?: 400 \| 500; tone?: "accent" \| "muted"` | "Label them", "+ Open", "Review" |
| `KeyChip` | `shortcut?: string; glyph?: string; variant: "filled" \| "outlined" \| "keycap"; selected?: boolean` | §7.6, Add §0 keycap |
| `Menu`, `MenuButton`, `ContextMenu`, `useContextMenu` | V1 API; `MenuItem` gains `icon?: IconName; submenu?: MenuEntry[]; trailing?: ReactNode`; `width?: number` | V-43; submenu opens right, aligned to its row, 2px gap; ←/→ navigate |
| `Popover` | V1 API + `offset?: number; width?: number; placement?: "anchor" \| "fixed"` | float style |
| `Tooltip`, `HoverCard` | V1 API | popover style, 12/16 text (V-17) |
| `Dialog` | `open; onClose; title; description?; width: 400 \| 520 \| 880; height?: number; placement: "window" \| "canvas"; footer?; initialFocus?; role?; children` | V-9: window = scrim over the window, centered; canvas = scrim over `#content-area`, centered on it. Pushes a modal layer. Escape closes |
| `ConfirmDialog`, `PromptDialog` | V1 API + `placement` | 400 wide |
| `Sheet` | `open; label; width?: number; pinned?: boolean; onClose; children` | Slides from the right over `#content-area` (09 §2); pinned → in flow, content narrows; Escape closes (not modal) |
| `SegmentedControl` | `options: {value, label, count?}[]; value; onChange; label; variant: "lens" \| "mini" \| "switch"` | components §7.5, 02 §4.1, 11 header |
| `Switch` | `checked; onChange; label; disabled?` | 28×16 track, 12px knob (08) |
| `Checkbox` | `checked: boolean \| "mixed"; onChange; label; disabled?` | 16px (V-23), 06 states, mixed bar (08) |
| `RadioGroup` | `options: {value, label}[]; value; onChange; label` | 12 §3.4 |
| `Monogram` | `name: string; size: 14 \| 20; tone?: "default" \| "selected" \| "workspace"` | §3 of components; letter = first char uppercased |
| `StateIndicator` | `state: ProjectState; variant: "row" \| "line"` | 07 glyph table; problems dot tooltip "N problems"; `resting` shows moon (V-20); `unavailable` shows the failed triangle with the reason in its tooltip |
| `ProgressRing` | `size: 14 \| 12; strokeWidth?` | rotates (V-68) |
| `Toaster`, `showToast`, `dismissToast` | `ToastOptions = {title; body?; leading?: ReactNode; action?: {label; run}; duration?: number \| null; tone?: "info" \| "danger"; id?: string}` | 07 toast; V-67; `id` replaces a toast with the same id (engine toast) |
| `SectionLabel` | `children; trailing?: ReactNode; as?: "h2" \| "h3"` | caps 11/14 |
| `ListRow` | `leading?; label; trailing?; depth?: number; indent?: number; selected?; muted?; onClick?; onDoubleClick?; onContextMenu?; menu?: ReactNode` | 28px row; hover `--color-hover`; selected `--color-selection`; trailing slot fixed 16px unless content given |
| `Badge` | `icon?: IconName; children` | "File exists" (08) |
| `KindGlyph` | `kind: "function" \| "entrance" \| "group" \| "link"; selected?` | ƒ text glyph / icons (components §4.5) |
| `TreeView` | V1 API | restyled 28px rows (settings scope tree) |
| `VirtualList` | V1 API + `rowHeight` | Add results, Health list |
| `ResizeHandle` | V1 API | sidebar edge |
| `TextField`, `SearchField` | V1 API | restyled; Add has its own 16px input |
| `EmptyState` | `title; body?; action?` | restyled |

Context provided by `canvas/` (U1b): `ProjectScope` (`<ProjectScope.Provider value={projectId}>`, `useScopeProject()`),
used by `code/CallLink` for `peek {project, key}`; provided by each card and by the peek sheet.

---

## 7. Screens and their data

Notation: `Cmp(props)` component tree; **reads** = queries; **calls** = commands run on interaction; **refreshed by**
= events (through §4.3 unless noted).

### 7.1 Frame (all artboards; U1c)

```text
App
 ├ TabStrip (role=tablist "Window tabs")
 │   HomeTab · BoardTab(ref)* (Monogram 14 of the board's project, title, × slot V-33, context menu V-44)
 ├ Body
 │   Sidebar container (width from layout; ResizeHandle; hidden by ⌘B)
 │     home mode → HomeSidebar            board mode → BoardSidebar(ref)
 │   #content-area
 │     home → HomeMain                    board → BoardView(ref)   (only the active tab is mounted, V-46)
 │     PeekSheet (U4)
 ├ AddBox (U4) · DialogHost (U5) · Toaster
```

Reads `workspace`, `app_info`, `get_preferences` (theme and bindings through `preferences/` hooks). Drop of a folder
anywhere → `addDroppedFolder` → select it.

### 7.2 Artboards

**01 · Home — project** (U3)

```text
HomeSidebar
  WorkspaceRow(Monogram workspace, "<username>'s workspace", IconButton search → openAdd())
  NavRow Recents → setHomeView("recents") · NavRow Drafts(count) → setHomeView("drafts")   (V-65)
  SectionLabel "Projects" + IconButton plus → addProjectFromPicker
  ProjectList (virtual, flattened)
    ProjectRow(chevron → setProjectExpanded, Monogram 20, name + branch, StateIndicator row, ProjectMenuButton V-18)
      click → selectProject + setHomeView("overview") + activateHome (V-52)
    ProjectTree (when expanded): FolderRow(chevron, folder icon, name) · BoardRow(board/draft icon, title, pages)
      BoardRow click → openBoard; context menu → boardMenuEntries; FolderRow context menu (V-51)
  SidebarFooter(Add project → addProjectFromPicker, IconButton gear → openDialog preferences)  (Home mode only, V-45)
HomeMain(project = selectedProject)
  ProjectHeader(path, name, ProjectMenuButton, Button Settings → projectSettings, Button New board → newBoard,
                WorktreeButton (U5), StateLine)
  HowItStarts(V-48; header + "Add entrance" → entranceEditor add,
              ApplicationCard*(EntranceRow*(hover: Trace → traceEntrance), OverflowFooter "N more · Filter"),
              Disclosure "Not reached from a launch · N", Disclosure "Launches into code that isn't indexed · N")
  BoardsSection(V-49; FilterPills, SortMenu, BoardGrid(BoardCard(Thumbnail V-50, title, subline)))
  HealthStrip(OK items, review items with "Review", "Health and activity ›" → setHomeView("health"))
```

Reads: `workspace`, `drafts`, `list_boards` (each expanded project), `project_summary`, `entrances
{perApplication: 8}`, `list_boards` (selected), `board_thumbnails` (chunks). Calls: `select_project`,
`set_project_expanded`, `new_board`, `open_board`, `windowCommand add_project`. Refreshed by: `workspace_changed`,
`project_state`, `boards_changed`, `graph_updated`, `problems_changed`, `compile_progress` (state line).
Entrance row menu (manual entrances only, `project_config` decides): Edit…, Remove… (dialogs).

**02 · Board — canvas** (U2)

```text
BoardSidebar(ref)
  BoardHeader(Breadcrumb(back → activateHome, Monogram 14, "project / folder"), Title (double-click → inline
              rename V-74), BoardMenuButton (U5))
  PagesSection(SectionLabel + add → addPage, PageRow*(page icon, title, review dot; double-click rename; menu V-53))
  OnThisPage(SectionLabel "follow order", LayerRow*(monogram column V-36, KindGlyph, label, review dot / "not
             connected"), LinkRow* (V-59), LabelThemLine → step through unlabeled)
  SelectedSection(collapsible V-60: SelectedCard | SelectedNode | SelectedGroup | SelectedLink | hint)
BoardView(ref)
  TopBar(page title, PublishState (dot + text from list_boards), Share MenuButton (shareMenuEntries), Publish →
         openDialog publish, Lens(SegmentedControl lens) centered on the uncovered canvas V-38)
  Stage
    BoardBanner(conflict | save error)
    BoardCanvas(page) → CardNode: BlockCard | EntranceCard | PlaceholderCard; ArrowEdge; LinkEdge + LinkLabel
    Dock(Add → openAdd(), Back, Forward, Trail, Arrange, Zoom −, %, +, Fit)  centered on the uncovered canvas
```

Reads: `list_boards` (header folder, publish state, "uses …"), `card`, `peek`, `relations`, `callers` (Show
callers), `app_info` (author). Calls: `open_board`, `save_board`, `save_board_view`, `board_health`,
`reload_board`, `repair_board`, `export_mermaid_text`, `windowCommand export_mermaid`, `copy_to_clipboard`.
Refreshed by: `board_conflict` (store), `graph_updated` (cards, health), `files_changed` (cards),
`boards_changed` (header), `workspace_changed` (resolution).

Card menu: Open source (peek at the definition), Show callers, Draw link…, Add as entrance… (blocks that can be
handlers), Fold/Unfold card, Remove from page. Canvas menu: Fit, Fold all cards, Unfold all cards. Review banner
"Review" → RepairPopover (the item's repairs, each with `repairEffect` text, Apply → `repairBoard`).

**03 · Add — map**, **04 · Add — search**, **04b · Add — commands** (U4)

```text
AddBox (Dialog-like layer, placement window, 1040 wide, 660 tall; commands mode fit-content)
  SearchRow(icon, input (16/24), TextOptions(Aa ab .*) on the Text tab V-1, KeyChip esc)
  KindTabs (map: Entrances Functions Files Text; search: All + counts) · right text ("Adds to “page”" / "N projects ·
            M results")
  Body
    map:      LookIn(All projects + ProjectRow(Monogram, name, count))  |  Map(ProjectGroup(header, compiling line,
              AppLane(EntranceChip*(on page mark V-5) + "+N more")))   · HoverPopover(preview, board_refs)
    search:   Results(VirtualList groups: Entrances, Functions, Files, Text; ResultRow; "N more <kind>")  |
              Preview(location, title, code block (syntax V-6), relations, buttons)
    commands: CommandList(CommandRow(icon, group ›, name with bold match, KeyChips))
  Footer(key hints)
```

Reads: `entrances` per connected project (map), `board_refs`, `preview`, `search`, `file_view` (file/text
preview), `search_text` stream. Calls on Enter: entrance → `openEntrance(target, project, key)`; function →
`addBlock(target, project, key)`; file → `openPeek({project, path})`; text → `openPeek({project, path, line,
matches})`; command → `actions.run(id)`; ⌘Enter → `addPage` then the same; from Home → `traceEntrance` /
`newBoard(result project)` + add (V-63). Refreshed by: `graph_updated` (map, preview, search), `files_changed`
(file search), `boards_changed` (`board_refs`), `workspace_changed` (projects).

**05 · First launch** (U3)

```text
HomeSidebar (empty variant: "No projects yet")
HomeMain → FirstLaunch(Intro, Button primary "Add project…" ⌘O → addProjectFromPicker, DropZone (drop handled by
          app/, drag-over style), OpenedBefore(rows from workspace.removed: folder icon, path, "Add" → readdProject))
```

Reads `workspace` (`projects` empty, `removed`). Calls `add_project` (window), `readd_project`.

**06 · New project — first Compile** (U3)

```text
HomeMain → NotCompiled(ProjectHeader compact (⋯ only, state line "Not compiled yet · branch main"),
  Summary (languageSentence), Skipped table, PreviewProblems (Health rows, V-69), ScopeCard(Checkbox rows of
  scope_dir "" with counts, "N of M files"), Promise, CompileRow(Button primary "Compile" ⇧⌘B, caption))
```

Reads `project_summary`, `compile_preview`, `scope_dir {path: ""}`. Calls on Compile: `update_project_config`
(staged scope edits, V-69) then `startCompile(project)`. Refreshed by `config_changed`, `project_state`,
`compile_progress`; once the state leaves `not_compiled`, HomeMain renders the compiled Home, filling as
`graph_updated` invalidates `entrances`.

**07 · Project states** (U1a components, U3 usage, U5 menu)

`StateIndicator` (row and line variants), `StateLine` (`home/stateLine.ts`: headline, detail, action per 07 table and
V-25, fallback/scanning info icon with tooltip), toasts (engine: U6; compiled: U3), `projectMenuEntries(row)` (U5, full
menu with V-18's disabled rules, in the V-43 menu style).

**08 · Project settings** (U5)

```text
Dialog(window, 880×700) ProjectSettingsDialog(project, section)
  Header(title "<name> settings", ".unfold/config.toml · shared with your team", Badge "File exists"/"Not created
         yet", IconButton close)
  Nav(General, Scope, Detectors, Python projects)
  General(name TextField (staged), path + Reveal, Switch background updates (immediate V-71), storage notes)
  Scope(heading, TreeView(lazy scope_dir; Checkbox, folder, reason, count) V-70, Rules list (staged removals))
  Detectors(groups by language, Switch per detector (staged))
  PythonProjects(V1 project drafts, restyled)
  Footer(note, Cancel, Save (disabled when clean))
```

Reads `project_config`, `scope_dir`, `project_summary` (notes). Calls `update_project_config {edits}` (all or
nothing), `set_background_updates`, `reveal_in_finder`. Refreshed by `config_changed` (if clean; staged edits stay and
a quiet line says the file changed).

**09 · Source peek** (U4)

```text
PeekSheet(target) (Sheet 640; pinned narrows #content-area)
  Header(Monogram 20, path, "N lines · Language", Pin, More MenuButton(Open in editor ⌘⇧O, Reveal in Finder), Close)
  Lines (virtual by line; pages of 200 via file_view range): AddSlot(⊕ / on-page dot), LineNo, FoldSlot, CodeText
  MatchTicks (scroll overlay)
  FooterHint("adds a function to <page>" · esc closes · pin keeps it open)
```

Reads `file_view {project, path, range}` per page in view, `peek` (call hover). Calls: ⊕ → `addBlock(target,
project, key)` (on Home: new draft, V-66); call link click → `addBlock` or focus; `open_in_editor`,
`reveal_in_finder`. Refreshed by `files_changed`, `graph_updated` (paths).

**10 · Board across projects** (U2)

PlaceholderCard (V-35, V-61), cross-project BlockCard header monogram, LinkEdge + LinkLabel (10 §5.5–5.6),
SelectedLink (Edit label, Remove link), LayerRow monogram column (V-36), PublishState "Draft · uses <names>".
Reads `workspace` (resolution), `card` per resolved project. Calls `windowCommand add_project` (Connect project).

**11 · Health and activity** (U3)

```text
HomeMain(view health|activity, V-47) → ProjectHeader + HealthActivity
  Header("Health and activity", SegmentedControl switch Health(count) | Activity, "Back to overview")
  health:   HealthList(VirtualList HealthRow(icon, title, explanation, location, action Open | Settings)),
            BoardsHealth(BoardHealthRow(board, items text, Review)), QuietNote
  activity: ActivityHeader("Every update to the graph, newest first", Copy diagnostics) + LogTable(virtual rows)
```

Reads `problems`, `index_log`, `project_summary`. Calls: Open → `openPeek({project, path, line})`; Settings →
`openDialog projectSettings` (V-77); Review → `openBoard` + select (V-77); Copy diagnostics → `copyDiagnostics(project)`.
Refreshed by `problems_changed`, `graph_updated` (`index_log`).

**12 · Publish** (U5)

```text
BoardMenu (MenuButton in BoardHeader; boardMenuEntries(ref)): Rename… · Move to folder › (folders, Top level, New
  folder…) · Duplicate · Open in new window · — · Share › (Export Mermaid…, Copy Mermaid) · Fold all cards · Unfold all
  cards · — · Discard changes… (only with changes) · Move back to drafts (published only) · — · Delete…
PublishDialog(canvas placement, 520): first publish (name field) | changes | conflict (RadioGroup Keep mine / Take
  theirs) (V-72)
DiscardDialog(400) · DeleteBoardDialog(400) (V-73)
```

Reads `list_boards` (folders, publish state, changes). Calls `publish_board {name?, resolve?}`, `discard_changes`,
`unpublish_board`, `delete_board`, `duplicate_board`, `set_board_folder`, `create_folder`,
`export_mermaid_text`/`export_mermaid`, `move_tab_to_new_window` chain. Refreshed by `boards_changed`.

**13 · Board — dark** — tokens only (§5); no component differences (V-32 tab merge applies in both themes).

### 7.3 Every row of the redesign's §6 table

| V1 | Component / action (owner) |
| --- | --- |
| Folder switcher (Recent, Open Folder, … Preferences) | `HomeSidebar` projects list and `ProjectRow` (U3); `project.add` + footer/header `+` (U1b action, U3 buttons); `projectMenuEntries` (U5); gear → `app.preferences` (U1c); Recents → `NavRow` + Home `recents` view (U3) |
| Title bar board title (rename) | `BoardHeader` inline rename (U2); board menu Rename… (U5) → `renameBoard` |
| Title bar Code \| Flow | `TopBar` lens (U2); `board.lensCode` / `board.lensFlow` |
| Title bar Compile | `NotCompiled` Compile (U3); project menu Compile again / Rebuild / Cancel (U5); `project.compile` (U1b) |
| Title bar Publish | `TopBar` Publish (U2) → `PublishDialog` (U5); `board.publish` |
| Toggle Inspector | removed; `SelectedSection` (U2); `view.toggleSidebar` ⌘B (U1b) |
| Board menu `⋯` (Rename, Mermaid, Fold/Unfold All, Move to New Window, Move Back to Drafts, Delete) | `BoardMenuButton` + `boardMenuEntries` (U5) in `BoardHeader` (U2); Fold/Unfold also in the canvas menu (U2); Mermaid under `shareMenuEntries` (U5) in `TopBar` (U2) |
| Activity bar | removed (no component) |
| Files view | Add Files kind (U4); scope actions → settings Scope (U5); Open in editor / Reveal → peek `⋯` (U4) and project menu Reveal in Finder (U5); Show callers → card menu (U2) |
| Entrances view | `HowItStarts` (U3) and Add map (U4); Add entrance → project menu (U5), Home "Add entrance" and application card menu (U3) → `entranceEditor`; Edit/Remove manual → entrance row menu (U3) |
| Search view (tabs, match case, whole word, regex) | `AddBox` search with kind tabs and text options (U4) |
| Boards view | `BoardsSection` (U3) and sidebar `ProjectTree` (U3); outline and groups → `OnThisPage` (U2); Flow-label stepper → `LabelThemLine` (U2); health and repairs → card `ReviewBanner` + `RepairPopover` (U2), `HealthStrip`/`HealthActivity` (U3) |
| Editor tabs for boards | `TabStrip` (U1c) over `state/tabs` (U1b) |
| Source tabs | `PeekSheet` (U4) |
| Board bar (Back, trail, zoom, Fit, Arrange) | `Dock` with Forward (U2) |
| Canvas and card context menus | `BoardCanvas` pane menu, `cardMenu` (U2) |
| Banners (outside edit, failed save) | `BoardBanner` (U2) |
| Inspector | `SelectedSection` (U2) + `DescriptionStrip` editing on the card (U2) |
| Bottom panel: Problems | `HealthStrip` + `HealthActivity` health (U3) |
| Bottom panel: Index log | `HealthActivity` activity (U3) |
| Bottom panel: References | `SelectedCard` Called by / Reached from (U2); `canvas.showCallers` (U2) |
| Status bar | `StateIndicator` on `ProjectRow` + `StateLine` (U3); fallback/scanning → `StateLine` tooltip and settings General (U5); connection → engine toast (U6); zoom → `Dock` (U2) |
| Status menu (Compile, Rebuild, Cancel, Pause/Resume, Index log, Copy Diagnostics) | `projectMenuEntries` (U5); Copy Diagnostics → `help.copyDiagnostics` (U5) in Help menu, project menu, Add commands |
| Welcome screen | `FirstLaunch` (U3); `project.add`; drop anywhere (U1c) |
| First-Compile screen | `NotCompiled` (U3) |
| Close Folder | project menu "Remove from workspace…" (U5): `confirm_remove_project` → `closeBoardsOfProject` → `remove_project` |
| Folder Settings dialog | `ProjectSettingsDialog` (U5) |
| Manual entrance editor | `EntranceEditorDialog` (U5) |
| Preferences | `PreferencesDialog` (U5); gear (U3), `app.preferences` ⌘, (U1c) |
| Command palette, Go to File, Go to Symbol | `AddBox` commands / Files / Functions (U4); `add.commands`, `add.files`, `add.functions` |
| Copy Diagnostics | `help.copyDiagnostics` (U5): Help menu `menu_command`, project menu, Add |
| Several windows | `window.new` (U1c); Open in new window: tab menu (U1c), board menu (U5) → `moveBoardToNewWindow` (U1b); restore (U6) |
| Application menu | `keyboard/menu.ts` `MENU_COMMAND_ACTIONS` (U1b, audited by U6); the menu itself is desktop D3 |

---

## 8. Keyboard

### 8.1 Action map (`keyboard/keymap.ts`)

All are well-known ids: rebindable, listed in Preferences → Keyboard, stored by id (V1 rules). Shortcuts in keymap
notation (`Mod` = ⌘/Ctrl).

| Action id | Title (category) | Default | Registered by | `MenuCommand` |
| --- | --- | --- | --- | --- |
| `add.open` | Add… (Go) | `A`, `Mod+K`, `/` | U4 | `add` |
| `add.commands` | Commands… (Go) | `Mod+Shift+P` | U4 | `commands` |
| `add.files` | Go to File… (Go) | `Mod+P` | U4 | `add_files` |
| `add.functions` | Go to Function… (Go) | `Mod+T` | U4 | `add_functions` |
| `tabs.home` | Home (View) | `Mod+0` | U1c | `show_home` |
| `tabs.next` / `tabs.previous` | Next Tab / Previous Tab (View) | `Ctrl+Tab`, `Mod+Shift+]` / `Ctrl+Shift+Tab`, `Mod+Shift+[` | U1c | |
| `tabs.close` | Close Tab (File) | `Mod+W` | U1c | `close_tab` |
| `board.new` | New Board (File) | `Mod+N` | U1c | `new_board` |
| `board.newPage` | New Page (Board) | `Alt+Mod+N` | U2 | `new_page` |
| `window.new` | New Window (File) | `Mod+Shift+N` | U1c | |
| `window.close` | Close Window (File) | `Mod+Shift+W` | U1c | |
| `project.add` | Add Project… (File) | `Mod+O` | U1c | |
| `project.compile` | Compile (Project) | `Mod+Shift+B` | U1c | `compile` |
| `board.lensCode` / `board.lensFlow` | Code / Flow (View) | `Mod+1` / `Mod+2` | U2 | `lens_code` / `lens_flow` |
| `view.toggleSidebar` | Toggle Sidebar (View) | `Mod+B` | U1c | `toggle_sidebar` |
| `navigation.back` / `navigation.forward` | Back / Forward (Go) | `Mod+[` / `Mod+]` | U2 | |
| `board.fit` | Fit (Board) | `Shift+1` | U2 | |
| `board.zoomIn` / `board.zoomOut` | Zoom In / Zoom Out (Board) | `Mod+=`, `Mod+Shift+Plus` / `Mod+-` | U2 | |
| `board.arrange` | Arrange by Call Depth (Board) | `Shift+A` | U2 | |
| `edit.undo` / `edit.redo` | Undo / Redo (Edit) | `Mod+Z` / `Mod+Shift+Z` | U1c (scoped) | `undo` / `redo` |
| `app.preferences` | Preferences… (File) | `Mod+,` | U1c | |
| `board.publish` | Publish (Board) | `Mod+Shift+E` | U2 | `publish` |
| `project.remove` | Remove Project from Workspace… (Project) | — | U5 (V-76) | `remove_project` |
| `help.copyDiagnostics` | Copy Diagnostics (Help) | — | U5 | `copy_diagnostics` |

Other registered actions (not rebindable, reachable in Add's `>` mode; `group` in parentheses): U2 `board.foldAll`,
`board.unfoldAll`, `board.groupFromLines`, `board.describeSelected`, `board.nextUnlabeled`, `board.removeSelected`,
`board.showCallers` (Board), `board.exportMermaid`, `board.copyMermaid` (Share); U5 `project.compileAgain`,
`project.rebuild`, `project.cancelCompile`, `project.pause`, `project.resume`, `project.settings`,
`project.addEntrance`, `project.revealInFinder`, `home.showHealth` (Project), `board.rename`, `board.moveToFolder`,
`board.duplicate`, `board.openInNewWindow`, `board.discardChanges`, `board.moveBackToDrafts`, `board.delete` (Board);
U4 `peek.close` (`Escape` while the sheet is open, not typing), `peek.openInEditor` (`Mod+Shift+O`, only while open).
Escape in overlays is handled by each layer, never by the registry.

### 8.2 Dispatcher rules (kept from V1, `keyboard/dispatcher.ts`)

1. Events already `defaultPrevented` (lists, menus, the canvas's own keys) are left alone; composing is ignored.
2. Plain keys (`A`, `/`, `Shift+1`, `Shift+A`) never fire while typing in an editable target; ⌘/Ctrl shortcuts do,
   except actions with `allowInInput: false` (undo, redo).
3. No window shortcut fires while a modal layer is open (`pushModal`: Add, every `Dialog`); the peek sheet is not modal.
4. Later registrations win (a mounted view over the shell); disabled actions let the key through.
5. A handled key is `preventDefault`ed, so Electron's menu accelerator doesn't run it twice; auto-repeat is swallowed
   unless `repeatable` (zoom).
6. Undo/Redo go to the innermost command scope around focus or the last click: `BoardView` and `BoardSidebar` both
   register the board's scope.
7. New (V-75): `[`, `]`, `=`, `-`, `,` fall back to the physical key (`event.code`) when Shift changes the typed
   character, so `Mod+Shift+]` matches on every layout; `Ctrl+Tab` is a valid chord on macOS.

### 8.3 Menu commands

`MENU_COMMAND_ACTIONS: Record<MenuCommand, ActionId>` per the table's last column (all 17 contract §7.3 commands).
`runMenuCommand` runs the action if registered and enabled (V1). Menu items the main process handles itself (Add
Project…, New Window, Close Window) need no mapping; their UI actions exist for the dispatcher and Add.

### 8.4 Custom key bindings

Unchanged mechanism: `Preferences.keyBindings` = action id → shortcut (`""` = none); unknown ids and bad shortcuts
ignored; Save carries foreign ids over; conflicts flagged. The rebindable set is §8.1's first table.

---

## 9. Performance

| Concern | Rule |
| --- | --- |
| Long lists (library: `@tanstack/react-virtual` 3.14.13, already a dependency, through `VirtualList`/`TreeView` or `useVirtualizer`) | Virtualized: Home sidebar project tree (flattened rows, 28px), Add results (32px rows), Add map (one virtual item per project group when > 8 projects), Home Boards / Recents / Drafts grids (rows of cards once > 24 boards), Health list (≤ 5,000), Activity log, expanded application (> 100 entrances), peek lines (20px, pages of 200), settings scope tree (`TreeView`), On this page (> 150 rows). Not virtualized: Pages, Selected, menus, map chips inside one project |
| Canvas | V1 kept: node and edge objects cached by id and reused while unchanged; `CardNode`, `ArrowEdge`, new `LinkEdge` memoized; per-card zustand selectors; `onlyRenderVisibleElements` above 40 cards; sizes in page view state so remounts don't re-measure; only the active board tab is mounted (V-46) |
| Card data | One `card {project, key}` query per definition shared by every page and board; Flow labels applied in the UI (V-56); last verified render kept in memory per `project\0key` |
| Invalidation | §4.3 predicates by project, path, and key; `workspace` and `get_preferences` replaced from event payloads; `project_state` patches one row; no blanket refetch except `graph_updated.everything` (that project only) and the restart reset (§3.8) |
| Waking | Lists use `rests` commands (`list_boards`, `drafts`, `entrances`, `search`, `board_thumbnails`); `project_summary` only for the selected project; Add's map popover waits 300 ms before `preview` (V-64) |
| Thumbnails (contract `Thumbnail`, D-26) | `board_thumbnails` in chunks of ≤ 60 ids aligned to the sorted, filtered list; a chunk is enabled only when one of its rows is rendered; geometry computed by a pure function memoized per `Thumbnail` object (WeakMap): fallback card sizes from `canvas/layout.ts` (`lines` → height 44 + 20·min(lines, 24); folded → 44; entrance 240×160), bounds fit into 252×138 with 22px margins, 2–5 text lines per card, edges as 1px paths, foreign cards with the 9px monogram square; plain SVG, no measuring, ≤ 200 cards |
| Debounces | Autosave 600 ms; health refresh 250 ms; `save_board_view` 1 s; `save_window_state` 400 ms; Add query 120 ms; preview 150 ms; popover 300 ms |
| Streams | One `search_text` at a time per Add box, cancelled on every keystroke; late messages dropped |
| Memory | Query `gcTime` default (5 min) so cards of closed boards and peek pages are collected; peek fetches only the pages in view |

---

## 10. Work split

### 10.1 Order

```text
U1-0 (orchestrator): cp -R ui/src docs/redesign/build/v1-ui/src   (read-only reference)
U1a kit  ─┐
U1b core ─┴─► U1c frame ─► U2 board ┐
                          U3 home   ├─► U6 windows, menu audit, restart, sweep
                          U4 add    │
                          U5 dialogs┘
```

U1a and U1b run in parallel; U1c needs both. `just js-check` is green at the end of U1c and must stay green after each
of U2–U6 (each runs it at the end and reports failures in files it doesn't own instead of fixing them). Every package
runs `docs/redesign/build/gates.sh js`.

### 10.2 Packages and file ownership

| Pkg | Scope | Owns (creates, edits, deletes) |
| --- | --- | --- |
| **U1a kit** | Tokens (§5), base CSS, contrast test; `icons/` (whole V-22 set incl. state glyph paths); every component of §6 with tests; `lib/format.ts`, `lib/entrances.ts`, `lib/files.ts`; **token renames in every `*.css` under `ui/src/`** (mechanical) | `theme/**`, `icons/**`, `components/**`, `lib/**`, token renames in all CSS |
| **U1b core** | `ipc/` (§4), `keyboard/` (§8), `state/` (§3 except `session.ts`), `test/` helpers; port `state/boards/*` to pages/projects/`BoardEdit` incl. transfer; mechanical adaptation of `canvas/`, `canvas/cards/`, `code/`, `flow/` TS to the new types and per-page store (no visual change; `ProjectScope`); global action implementations (`state/actions.ts` functions used by U1c registrations) | `ipc/**` (not `generated/`), `keyboard/**`, `state/**` (not `session.ts`), `test/**`, TS files of `canvas/**`, `code/**`, `flow/**` |
| **U1c frame** | `main.tsx`, `app/**` (frame, TabStrip with tab menu, sidebar container, content switch, overlays host, drop, global action registrations of §8.1 marked U1c, `project_added`); create every screen package's entry file with the §10.3 signatures and a minimal body; delete V1 folders (§1); remove `cmdk`; make `just js-check` green | `main.tsx`, `app/**`, `package.json`, the stub entry files (ownership passes to U2–U5 at once), deletions |
| **U2 board** | `board/**` (sidebar board mode: header, Pages, On this page, Selected; top bar; dock; BoardView; RepairPopover; groupFromLines), canvas restyle (V-34, V-57), `LinkEdge`/link mode/link labels, `PlaceholderCard`, Show callers, `code.css`, `flow.css`, card menu; additions to `state/boards/` (link ops, `showCallers`, link mode); remove `lucide-react` | `board/**`, `canvas/**`, `code/**`, `flow/**`, `state/boards/**` (from the end of U1) |
| **U3 home** | `home/**`: HomeSidebar (workspace row, Recents, Drafts, projects, tree, footer), HomeMain (overview, first launch, not compiled, compiling, health, activity, recents, drafts views), thumbnails, state line, compiled toast | `home/**` |
| **U4 add** | `add/**` (map, look in, search, text stream, preview, commands, footer, `add.*` actions), `peek/**` (sheet, model, matches, menu, `peek.*` actions) | `add/**`, `peek/**` |
| **U5 dialogs** | `dialogs/**` (DialogHost, Publish, Discard, Delete, New/Rename folder, Remove entrance confirm), `settings/**` (project settings, entrance editor, diagnostics), `preferences/**` (restyle), `menus/**` (board, share, Move to folder, project menu, worktree switcher, `useProjectActions` registering U5 actions) | `dialogs/**`, `settings/**`, `preferences/**`, `menus/**` |
| **U6** | `state/session.ts` (restore, `save_window_state`, reconnect order, engine toast, `focus_board`), wire it in `app/` (minimal edit), audit `MENU_COMMAND_ACTIONS` against registered actions, dead-code sweep over `ui/` (orphan exports, CSS rules, tokens, dependencies, tests), delete `docs/redesign/build/v1-ui/` | `state/session.ts`; minimal edits anywhere for wiring and deletion only |

Rules for parallel packages: edit only owned files; the `state/` API of §10.3 is frozen after U1 (U2 may add to
`state/boards/` but must not change what §10.3 lists); no package imports `lucide-react` or `cmdk`; new shared needs
are met inside the package (and reported) rather than by editing `components/`.

### 10.3 Interfaces U1 exports for U2–U6

```ts
// ipc (U1b)
call, callStream, onEngineEvent, windowCommand, onShellEvent, useConnectionState
engineKey, engineQueryOptions, useEngineQuery(command, args, { enabled?, refetchOnWindowFocus? })
installEngineInvalidation(client), resetEngineQueries(client), createLatestStream(command)

// keyboard (U1b)
interface ActionSpec { id; title; category; group?: string; icon?: IconName; shortcuts?; keywords?;
  allowInInput?; repeatable?; showInAdd?: boolean /* default true */; enabled?(): boolean; run(): void | Promise<void> }
useRegisterActions(specs), useActions(), actions.run(id), useShortcut(id), useShortcutLabel(id),
useCommandScope(ref, {undo, redo}), requestFocus(key), useFocusTarget(key, ref), pushModal(), ActionIds

// state (U1b) — §3.2, §3.3, §3.11 signatures, plus:
useAuthor(): string | null                                     // person:<username>
// state/boards/index.ts
type BoardRef; type PageTarget; boardKey(ref); sameBoard(a, b)
openBoard(ref, { activate?: boolean; page?: PageId }): Promise<{ status: "opened"; ref: BoardRef } | { status: "held"; heldBy: WindowId }>
newBoard(project, { title?, folder?, pageTitle? }): Promise<BoardRef>          // opens and activates its tab
closeBoard(ref): Promise<void>; closeBoardsOfProject(project): Promise<void>
useOpenBoard<T>(ref, select: (b: OpenBoard) => T): T | undefined; getOpenBoard(ref)
setActivePage(ref, page); usePageView<T>(target, select: (v: PageView) => T): T
editBoard(ref, recipe, { coalesce?, page? }); undo(ref); redo(ref); canUndo(ref); canRedo(ref)
updatePageView(target, recipe); flushBoard(ref); saveBoard(ref, { force? }); resolveConflict(ref, "mine" | "theirs")
addPage(ref, { title?, activate? }): PageId; renamePage(target, title); duplicatePage(target): PageId
movePage(target, delta: -1 | 1); deletePage(target)
addBlock(target, project, key): string | undefined        // focuses an existing card instead (returns its id)
openEntrance(target, project, entranceKey): Promise<void> // entrance + resolved handlers, placement as V1
traceEntrance(project, entranceKey): Promise<BoardRef>    // new draft titled after the entrance, opened
followCall(request); focusCard(target, cardId); select(target, selection: Selection)
describeCard(target, cardId, text, author); labelNode(target, project, key, text, author)
editGroup(target, groupId, change); ungroup(target, groupId); removeCard(target, cardId); foldAll(target, folded)
addLink(target, from, to): string; setLinkLabel(target, linkId, text, author); removeLink(target, linkId)
usePageKeys(target | null): ReadonlySet<string>           // `${projectId}\u0000${key}` of resolved cards
useResolvedCardProject(ref, card): ProjectId | null
publishBoard(ref, { name?, resolve?: "keep_mine" }): Promise<PublishOutcome>   // adopts a new id + tab
discardChanges(ref); unpublishBoard(ref); deleteBoard(ref); duplicateBoard(ref): Promise<BoardSummary>
setBoardFolder(ref, folder | null); renameBoard(ref, title); repairBoard(ref, repair: RepairSpec)
exportPageMermaid(target): Promise<ExportedFile | null>; copyPageMermaid(target): Promise<void>
createFolder(project, path); renameFolder(project, from, to); deleteFolder(project, path)
moveBoardToNewWindow(ref): Promise<WindowId>
itemsFor(health, target), repairLabel(kind), repairEffect(kind, item, page), repairSpec(kind, item, page, to?)

// screen entry files created by U1c (bodies replaced by the owner)
// home/index.ts      (U3) export function HomeSidebar(): JSX.Element; export function HomeMain(): JSX.Element
// board/index.ts     (U2) export function BoardSidebar(p: { board: BoardRef }): JSX.Element
//                         export function BoardView(p: { board: BoardRef }): JSX.Element
// add/index.ts       (U4) export function AddBox(): JSX.Element | null     // mounted once; registers add.*
// peek/index.ts      (U4) export function PeekSheet(): JSX.Element | null  // mounted once in #content-area
// dialogs/index.ts   (U5) export function DialogHost(): JSX.Element | null
// menus/index.ts     (U5) export function boardMenuEntries(ref: BoardRef): MenuEntry[]
//                         export function shareMenuEntries(target: PageTarget): MenuEntry[]
//                         export function BoardMenuButton(p: { board: BoardRef }): JSX.Element
//                         export function projectMenuEntries(row: ProjectRow): MenuEntry[]
//                         export function ProjectMenuButton(p: { project: ProjectId; variant: "row" | "header" }): JSX.Element
//                         export function WorktreeButton(p: { project: ProjectId }): JSX.Element
//                         export function useProjectActions(): void   // mounted once by App
// settings/index.ts  (U5) export function copyDiagnostics(project?: ProjectId): Promise<void>
// preferences/index.ts (U5) PreferencesDialog, usePreferencesTheme(), usePreferencesKeyBindings()
```

---

## 11. Unit tests

| Area | Kept (adapted to new types) | Deleted | New (owner) |
| --- | --- | --- | --- |
| canvas | `arrows.test`, `layout.test` | `BoardTab.test` (→ BoardView) | U2: `BoardView.test` (actions, undo scope, first fit), links (create, select, remove, label), placeholder card, Show callers placement, page switching keeps cameras |
| code, flow | `CodeView.test`, `flow/layout.test` | | U2: external call class, ligatures off (render snapshot of classes) |
| components | `Dialog`, `Menu`, `Resizable`, `TreeView`, `VirtualList` tests | Tabs/Chip/TreeMarks have none | U1a: Button variants, IconButton pressed, SegmentedControl variants, Checkbox mixed, Switch, RadioGroup, Monogram letter, StateIndicator per state, Sheet (Escape, pinned), Toast (id replace, action), submenus, Icon names complete |
| theme | `contrast.test` (new pairs) | | |
| ipc | | `queries.test` (V1 rules) | U1b: one test per §4.3 row (predicate hits only P, path, key); `createLatestStream` drops late messages |
| keyboard | `bindings`, `dispatcher`, `registry`, `scopes`, `shortcut`, `bindings.render` tests | V1 keymap ids in them | U1b: `Mod+Shift+]` physical match, `Ctrl+Tab`, menu map covers every `MenuCommand`, `showInAdd` |
| state | `boards/store.test`, `navigation.test`, `transfer.test`, `files.test` (pages, projects) | `layout.test`, `tabs.test`, `persistence.test` (V1) | U1b: document ops (pages, links, labels per project), `BoardEdit` payload, undo returns to page, resolution and same-name refusal, `closeBoardsOfProject`, tabs (rename on id change, 100 cap), layout snapshot validation, compile per project. U6: restore order, reconnect order, window-state payload |
| home | `factLines` (from panel.test), `languageSentence`/`skippedClauses` (from welcome.test), `relativeTime` (→ `lib`) | panel, welcome, shell, views tests | U3: state line per `ProjectState`, sidebar tree order and expansion, thumbnail geometry, boards filter/sort, health row actions, first-launch readd, scope staging until Compile |
| add, peek | `highlight` tests (from SearchView.test), `peek/model.test` (from editor/source) | `SearchView.test`, `CommandPalette.test`, `SourceTab.test` | U4: map paging and on-page marks, search grouping and "N more", text stream cancel, Enter/⌘Enter targets incl. Home, command fuzzy bold, peek ⊕ and matches |
| settings, prefs, dialogs, menus | `EntranceEditor.test`, `entrance.test`, `preferences.test`, `preferencesSync.test`, `KeyboardSection.test` | `FolderSettingsDialog.test`, `boards.test`, `BoardsView.test`, `inspector.test` | U5: project settings staging and all-or-nothing save, scope mixed state, project menu per state (V-18 disabled), board menu conditional items, publish variants (name taken, conflict keep mine / take theirs), worktree switch closes own tabs |

Goldens in `testdata/workspace/commands/` come from R4/R5 (re-blessed with `project`); UI tests read them through
`test/engine.ts` `fixture`.

E2E (step 10, not in U1–U6) keeps these hooks stable: `role=tablist` "Window tabs", `role=tab` per tab,
`aside[aria-label="Sidebar"]`, `[data-project-id]` on project rows, `[data-board-key]` on board rows and the board
view, `role=dialog` named "Add" / "Publish" / "<name> settings", `.board-card` with `data-card-id`,
`.react-flow__pane`, `.code-view`.

---

## 12. Decisions made here

Appended to [decisions.md](decisions.md) under "UI plan decisions" as **V-40..V-80**; the plan cites them where they
apply. Three are contract gaps the orchestrator should see: V-56 (`card.board` is never sent, so Rust could drop it),
V-70 (scope counts can't show included files), V-72 (Publish shows a change count, not a change list, because the
contract returns only `changes: number`).
