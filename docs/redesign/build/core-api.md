# Core API (U1b)

What `ipc/`, `keyboard/`, and `state/` export, as built by U1b, for U1c and U2–U6. This is the frozen surface of UI
plan §10.3 (deviations are listed at the end). U2 may add to `state/boards/`; nobody changes what is listed here.

Import from the module entry points: `ipc/index.ts`, `keyboard/index.ts`, `state/<file>.ts` (`workspace`, `window`,
`tabs`, `layout`, `overlays`, `compile`, `author`, `actions`), and `state/boards/index.ts`. `canvas/` exports
`ProjectScope`/`useScopeProject` (`canvas/ProjectScope.ts`) and `cardQuery` (`canvas/cardData.ts`).

## Wiring the frame (U1c, U6)

Once per window, before rendering:

| Call | What it installs | Uninstall |
| --- | --- | --- |
| `installEngineInvalidation(queryClient)` (`ipc`) | Cache invalidation by event (contract §5, plan §4.3) | returned function |
| `installBoardEvents()` (`state/boards`) | `board_conflict` (reload or conflict), `graph_updated` → debounced `board_health` of open boards whose cards resolve to the project, `project_reset` → forget remembered renders, `pagehide` → `saveAllNow` | returned function |
| `installCompileEvents()` (`state/compile`) | `compile_progress` → progress; `project_state` leaving compiling/updating → clears it | returned function |
| `installTransferReceiver()` (`state/boards`) | `board_state_transferred` receiver (queued until `setTransfersReady(true)`) | — |
| `useShortcutDispatcher()`, `useMenuCommands()` (`keyboard`) | keydown dispatcher; `menu_command` → action | on unmount |
| `setWindowInfo(info)` (`state/window`) | after `windowCommand("window_info")` | — |

Reconnect order (U6, plan §3.8): `selectProject(selected)` → `reclaimBoards()` (re-claims every board tab in tab order:
`held` drops the tab with a toast, a changed revision goes through the conflict rule; then flushes every dirty board
and pending view) → `save_window_state` → `resetEngineQueries(queryClient)`. `ipc/` installs no reconnect refetch.

Restore (U6, plan §3.9): `window_state {key}` → `applyLayoutSnapshot(saved.layout)` → `selectProject(...)` →
`openBoard(ref, { activate: false, page })` per saved tab (in parallel; rejects or `held` drop it) →
`activateTab(...)` → `setTransfersReady(true)`. Save: `{ key, selectedProject, ...tabsSnapshot(), layout:
layoutSnapshot() }`, 400 ms after a change of `useWindowStore`, the tabs or an open board's `activePage`
(`subscribeToTabs`), or the layout (`subscribeToLayout`).

## ipc/

| Export | Signature / notes |
| --- | --- |
| `queryClient` | The window's `QueryClient` (`staleTime: Infinity`, `retry: false`, no focus refetch). Focus = window focus (Electron) |
| `call` | `(command, args) => Promise<CommandResult<C>>`; rejects with `UnfoldError` |
| `callStream` | `(command, args, onMessage) => TypedStream<C>` (`{ result, cancel() }`) |
| `onEngineEvent` | `(type, handler: (event: Events[E]) => void) => unsubscribe` |
| `windowCommand` | `(command, ...args)` typed by `@unfold/desktop/unfold-api` (contract §7.3) |
| `onShellEvent` | `(type, handler)` for `menu_command`, `board_state_transferred`, `focus_board`, `project_added` |
| `addDroppedFolder` | `(file: DroppedFile) => Promise<AddedProject>` (window `{status, project: ProjectId}`) |
| `useConnectionState` | `() => ConnectionState` |
| `engineKey` | `(command, args) => ["engine", command, args]` |
| `engineQueryOptions` | `(command, args)` → `{ queryKey, queryFn }` (+ `staleTime: 0` for `git_info`, V-80) for `useQueries` / `queryClient.query` |
| `useEngineQuery` | `(command, args, { enabled?, refetchOnWindowFocus? }) => UseQueryResult<Result, UnfoldError>`; `refetchOnWindowFocus: true` refetches on every focus |
| `installEngineInvalidation`, `resetEngineQueries` | see Wiring. Reset keeps `get_preferences`, `app_info` |
| `createLatestStream` | `(command) => { run(args, onMessage): Promise<Result>; cancel(): void }`; each `run` cancels the previous; a superseded run's messages are dropped and it rejects with code `cancelled` |
| `UnfoldError`, `toUnfoldError` | `UnfoldError.code`: Rust's codes, transport codes, and `too_many_tabs` (from `openBoard`/`newBoard`) |
| types | every generated type (`export type *`), plus `ConnectionState`, `DroppedFile`, `MenuCommand`, `ShellEvent`, `WindowInfo` |

Invalidation (exactly plan §4.3; `P` = event project): `workspace_changed` sets `workspace`; `project_state` patches the
row and invalidates `project_summary(P)`; `project_reset` removes every query with `args.project === P`;
`graph_updated` (`everything`: every `(P)` query, `search` incl. P, `workspace`; else `(P)`: `file_view` by path,
`card` by key/file + every entrance card, `peek` by key/file, `relations`, `callers`, `preview`, `project_summary`,
`index_log`, `board_thumbnails`, `entrances` when entrances/inputs changed; `search` incl. P); `files_changed` (`(P)`
`file_view` and `card` by path, all when truncated; `search` incl. P with files); `boards_changed` (`list_boards(P)`,
`board_thumbnails(P)` naming a listed board, `drafts`, every `board_refs`, `project_summary(P)`); `problems_changed`
(`problems`, `project_summary` of P); `config_changed` (`project_config`, `compile_preview`, `scope_dir` of P);
`preferences_changed` sets `get_preferences`. `search_text` is a stream: Add re-runs it itself (U4).

## keyboard/

As V1's `index.ts`, plus `ActionIds` (new ids) and `ADD_GROUPS`.

| Export | Notes |
| --- | --- |
| `ActionSpec` | `{ id; title; category; group?; icon?: IconName; shortcuts?; keywords?; allowInInput?; repeatable?; showInAdd? (default true); enabled?(); run() }`. `group` is Add's group, default the category |
| `ActionIds` | `add` `add.open`, `addCommands` `add.commands`, `addFiles` `add.files`, `addFunctions` `add.functions`, `home` `tabs.home`, `nextTab` `tabs.next`, `previousTab` `tabs.previous`, `closeTab` `tabs.close`, `newBoard` `board.new`, `newPage` `board.newPage`, `newWindow` `window.new`, `closeWindow` `window.close`, `addProject` `project.add`, `compile` `project.compile`, `lensCode` `board.lensCode`, `lensFlow` `board.lensFlow`, `toggleSidebar` `view.toggleSidebar`, `back` `navigation.back`, `forward` `navigation.forward`, `fit` `board.fit`, `zoomIn` `board.zoomIn`, `zoomOut` `board.zoomOut`, `arrange` `board.arrange`, `undo` `edit.undo`, `redo` `edit.redo`, `preferences` `app.preferences`, `publish` `board.publish`, `removeProject` `project.remove`, `copyDiagnostics` `help.copyDiagnostics` — the rebindable set (§8.1); defaults in `DEFAULT_SHORTCUTS`, names in `ACTION_INFO` |
| `ADD_GROUPS` | `["Board", "Share", "Project", "Go", "View", "File", "Edit", "Help"]`: the order of Add's command groups |
| `MENU_COMMAND_ACTIONS` | every contract §7.3 `MenuCommand` → its action id (17) |
| `actions` | the window registry: `run(id)`, `get(id)`, `list()`, `shortcutsFor(id)`, `isEnabled(id)`, `register`, `subscribe` |
| hooks | `useRegisterActions(specs)`, `useActions()`, `useShortcut(id)`, `useShortcutLabel(id)`, `useShortcutLookup()` |
| scopes | `useCommandScope(ref, { undo, redo })`, `runScoped(command)`, `scopedHandler(command)` (U1c's `edit.undo`/`edit.redo`: `enabled: () => scopedHandler("undo") !== null`, `allowInInput: false`) |
| focus targets | `requestFocus(key)`, `useFocusTarget(key, ref)`. Keys used so far: `selected.description`, `selected.group:<groupId>` (card menu → U2's Selected) |
| modal | `pushModal()`, `isModalOpen()` |
| bindings, shortcut, platform | unchanged from V1 (`keyBindings`, `parseKeyBindings`, `findConflicts`, `shortcutLabel`, `usePlatform`, …) |

Rules (§8.2): plain keys never fire while typing; no window shortcut while a modal layer is open; later registrations
win; handled keys are `preventDefault`ed; repeats swallowed unless `repeatable`; `[ ] = - ,` with Shift match and record
by physical key (V-75); `Ctrl+Tab` works on macOS.

## state/

### `state/window`

`useWindowStore` (`{ info: WindowInfo | null; selectedProject: ProjectId | null }`), `setWindowInfo(info)`,
`useSelectedProject()`.

### `state/workspace` (§3.2)

`useWorkspace()`, `getWorkspace()`, `ProjectIndex { rows; byId; byName }`, `projectIndex(workspace)` (memoized per
`WorkspaceView`), `useProjectIndex()`, `getProjectIndex()`, `useProjectRow(id)`, `resolveRefProject(index,
boardProject, refProject) => ProjectId | null`, `refProjectFor(index, boardProject, cardProject) => { ok: true;
project?: ProjectName } | { ok: false; reason: "same_name" }`, `selectProject(id)` (never rejects),
`addProjectFromPicker(): Promise<ProjectId | null>` (selects it), `readdProject(path): Promise<ProjectId>` (selects
it), `setProjectExpanded(id, expanded)`.

### `state/tabs` (§3.3)

`TabKey = "home" | boardKey(ref)`; `useTabs` (`{ boards: BoardRef[]; active: TabKey }`), `MAX_BOARD_TABS` (100),
`activateHome()`, `activateBoard(ref)`, `activateTab(key)`, `openBoardTab(ref, { activate?, index? })`,
`closeTab(key)` (board: saves edits and view, `close_board`, removes the tab; Home: no-op), `closeOtherTabs(key)`,
`nextTab()`, `previousTab()` (wrap through Home), `moveTab(key, index)`, `renameBoardTab(from, to)`,
`useActiveBoard(): BoardRef | null` (stable ref), `getActiveBoard()`, `tabsSnapshot(): { tabs: WindowTab[];
activeTab: number | null }`, `subscribeToTabs(fn)` (tabs, active tab, or an open board's active page changed). A board tab exists exactly while its board is open in this window.

### `state/layout` (§3.9)

`WindowLayout { v: 1; sidebar: { width; visible }; selectedCollapsed; peek: { pinned }; home: { view: HomeView;
boardsFilter; boardsSort }; folders: string[] }`, `useLayout`, `initialLayout`, `SIDEBAR_WIDTH` (220/360/264),
`setSidebarWidth(px)`, `toggleSidebar()`, `setSelectedCollapsed(b)`, `setPeekPinned(b)`, `setHomeView(v)`,
`setBoardsFilter(f)`, `setBoardsSort(s)`, `setFolderExpanded(project, path, b)`, `folderKey(project, path)`,
`layoutSnapshot()`, `applyLayoutSnapshot(json: unknown)` (validated per field), `subscribeToLayout(fn)`.

### `state/overlays` (§3.3)

`AddKind`, `AddRequest`, `AddTarget`, `openAdd(request = { mode: "find" })`, `closeAdd()`, `useAddRequest()`,
`useAddTarget()`; `PeekTarget`, `openPeek(target)`, `closePeek()`, `usePeekTarget()`; `DialogRequest` (plan §3.3
union), `ProjectSettingsSection`, `DefinitionRef` (`{ path; kind; name; qualifiedName; line }`), `ManualRow`
(`Pick<EntranceRow, "key" | "detector" | "title">`), `openDialog(request)`, `closeDialog()`, `useDialog()`.

### `state/compile` (§3.11)

`startCompile(project, mode = "check")` (one per project per window; `cancelled` resolves), `cancelCompile(project)`,
`setBackgroundUpdates(project, on)`, `useCompileProgress(project)`, `useCompileStartedHere(project)` (true while a
Compile this window started for it runs; watch its falling edge for V-67's toast), `installCompileEvents()`.

### `state/author`

`useAuthor(): string | null` (`person:<username>`), `authorName(author)`.

### `state/actions` (global action implementations, U1c registers them)

| Function | For |
| --- | --- |
| `closeActiveTab()` | `tabs.close` (V-44: on Home closes the window only when Home is the only tab) |
| `newBoardHere(): Promise<BoardRef \| null>` | `board.new` (active board's project, else the selected one; V-63) |
| `compileHere()` | `project.compile` (same project rule) |
| `addProject()` | `project.add` (picker, then Home overview of it) |
| `newWindow()`, `closeWindow()`, `openPreferences()` | `window.new`, `window.close`, `app.preferences` |
| `showProject(project)` | project row click (V-52) and `project_added`: overview, Home tab, select |
| `currentProject()` | the active board's project, else the selected one |

`tabs.home` → `activateHome`, `tabs.next`/`tabs.previous` → `nextTab`/`previousTab`, `view.toggleSidebar` →
`toggleSidebar` (layout), `edit.undo`/`edit.redo` → `runScoped` (keyboard).

### `state/boards` (§3.4–§3.7, §3.10)

Types: `BoardId`, `PageId`, `WindowId`, `BoardRef { project; id }`, `PageTarget { board; page }`, `OpenBoard` (plan
§3.4 fields plus `savedDocument`), `PageView` (plan §3.4), `Selection`, `Camera`, `CardSize`, `TrailItem`,
`FocusMove`, `FocusRequest`, `PendingGroup`, `PendingPlacement`, `BoardSession`, `EditOptions`, `OpenOutcome`,
`FollowRequest`, `GroupChange`.

| Area | Exports |
| --- | --- |
| Keys | `boardKey(ref)`, `sameBoard(a, b)`, `pageKey(target)`, `cardRefKey(project, key)` |
| Open/close | `openBoard(ref, { activate?, page? }): Promise<OpenOutcome>` (opens the tab; `held` focuses the holder with `focus_board`), `newBoard(project, { title?, folder?, pageTitle? }): Promise<BoardRef>` (tab activated), `closeBoard(ref)`, `closeBoardsOfProject(project)`, `reclaimBoards()` |
| Read | `useOpenBoard(ref, select)` (select stable values), `getOpenBoard(ref)`, `isDirty(board)`, `usePageView(target, select)`, `getPageView(target)` (never-shown page = frozen empty view) |
| Pages | `setActivePage(ref, page)`, `addPage(ref, { title?, activate? }): PageId \| undefined`, `renamePage(target, title)`, `duplicatePage(target): PageId \| undefined`, `movePage(target, ±1)`, `deletePage(target)` |
| Edit | `editBoard(ref, recipe(document), { coalesce?, page? })`, `editPage(target, recipe(page, document), { coalesce? })`, `amendPage(target, recipe)` (part of the last step), `undo(ref)`, `redo(ref)`, `canUndo(ref)`, `canRedo(ref)` |
| View | `updatePageView(target, recipe)` (camera/lens changes are saved, debounced 1 s), `setCamera(target, camera)`, `setPageLens(target, lens)` (clears exceptions), `effectiveView(lens, cardView, defaultView, exception)`, `select(target, selection)`, `requestCardFocus(target, cardIds, "center" \| "fit")`, `pushFocusMove(target, trail?)`, `travel(target, "back" \| "forward")`, `setPendingGroup`, `setPendingPlacement`, `setLinking(target, from \| null)` |
| Save | `saveBoard(ref, { force? })`, `flushBoard(ref)`, `flushBoardView(ref)`, `flushAllBoards()`, `resolveConflict(ref, "mine" \| "theirs")`, `discardChanges(ref)` |
| Cards | `addBlock(target, project, key): string \| undefined`, `openEntrance(target, project, entranceKey)`, `traceEntrance(project, entranceKey): Promise<BoardRef>`, `followCall(request)`, `focusCard(target, cardId)`, `displayedRects(board, page)`, `entranceTitle(cardView)`, `lineOffsetIn(element)` |
| Authoring | `describeCard(target, cardId, text, author)`, `labelNode(target, project, key, text, author)`, `editGroup(target, groupId, change)`, `ungroup(target, groupId)`, `removeCard(target, cardId)` (with its groups and links), `foldAll(target, folded)`, `addLink(target, from, to): string \| undefined`, `setLinkLabel(target, linkId, text, author)`, `removeLink(target, linkId)` |
| Resolution | `usePageKeys(target \| null): ReadonlySet<cardRefKey>`, `useResolvedCardProject(ref, card): ProjectId \| null` |
| Document helpers (for `editPage` recipes) | `cardKey`, `keyDisplayName`, `findPage`, `findCard`, `labelsFor(labels, project)`, `moveCards`, `setCardView`, `setCardsFolded`, `addGroup`, `updateGroup` |
| Files | `publishBoard(ref, { name?, resolve? }): Promise<PublishOutcome>` (`done` moves the session and tab to the new id), `unpublishBoard(ref): Promise<BoardRef>`, `deleteBoard(ref)`, `duplicateBoard(ref): Promise<BoardSummary>`, `setBoardFolder(ref, folder): Promise<BoardSummary>`, `renameBoard(ref, title)`, `repairBoard(ref, repair)`, `exportPageMermaid(target)`, `copyPageMermaid(target)`, `createFolder`, `renameFolder(project, from, to): Promise<RenamedFolder>`, `deleteFolder`, `slugOf(title)`, `isValidName(name)`, `publishedPath(name)` |
| Health | `itemsFor(health, page, target)`, `kindWord(kind)`, `repairLabel(kind)`, `repairEffect(kind, item, page: Page)`, `repairSpec(kind, item, page: Page, to?)` (carries `page` and `project`) |
| Windows | `moveBoardToNewWindow(ref): Promise<WindowId>`, `installTransferReceiver()`, `setTransfersReady(b)`, `installBoardEvents()` |

Rules kept from V1: one undo step per edit, `coalesce` bursts within 1,200 ms merge, ≤ 200 steps; autosave 600 ms
after the last edit, saves never overlap, a conflict blocks autosave until resolved; `replaceEqualDeep` keeps identities
across saves; reload, discard and repairs are undoable steps. New: each step records its page and undo/redo shows it
(V-54); `save_board` sends `{ title, pages }` only; cards from another project carry its shared name in `ref.project`
and resolve against the current workspace; ids are unique within the page and ≤ 100 characters.

## canvas/, code/, flow/ (mechanical adaptation for U2)

`BoardTab({ board, active })` renders the active page (one React Flow store per page, keyed by page; `data-board-key`
on its root). `BoardCanvas({ target, ... })`, cards get `{ target, project }`; a card whose project isn't connected
renders a minimal "From <name> · not connected" placeholder (U2's `PlaceholderCard` replaces it). `cardQuery(project,
key)`, `lastVerifiedCode(project, key)`, `forgetVerifiedCode(project)`. Arrows join only cards resolving to the same
project. `code/CallLink` reads its project from `ProjectScope` (each card provides it; the peek sheet must too).
`BoardBar`, `BoardTab` (+ test), and the card menu stay in V1's look until U2 replaces them; the card menu's "Show in
Inspector" became "Describe…" (opens Selected, focus target `selected.description`), "Remove from Board" became
"Remove from Page", and "Add as Entrance…" opens `openDialog({ kind: "entranceEditor", mode: "add", ... })` using
`canBeHandler` from `settings/locators.ts`.

## Deviations from plan §10.3

- `itemsFor(health, page, target)` takes the page: card, group, and link ids are only unique within a page.
- `addLink` returns `string | undefined` (refused for a card linked to itself or a missing end); `addPage` and
  `duplicatePage` return `PageId | undefined` (board not open); `readdProject` returns `Promise<ProjectId>`.
- The 101st board tab is refused by `openBoard`/`newBoard` rejecting with `UnfoldError` code `too_many_tabs` before
  anything is claimed; the caller's failure toast shows it (state shows no toast of its own).
- `discardChanges` lives in the store (it reloads like Take theirs, as one undo step); exported from `state/boards`.
- `OpenBoard` has one extra field, `savedDocument` (the last loaded or saved version), so Move to New Window can tell
  the receiving window what is unsaved.
- `lib/keys.ts` (`pageKey`, `cardRefKey`) is not created (U1b doesn't own `lib/`); they live in `state/boards`.
- Added beyond §10.3: `state/actions.ts` (above), `tabsSnapshot`, `subscribeToTabs`, `reclaimBoards`, `editPage`/`amendPage`, page-view
  helpers, `installBoardEvents`, `installCompileEvents`, `setWindowInfo`, `addDroppedFolder`.
