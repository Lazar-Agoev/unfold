# `unfold-workspace` core for the board commands (R4 → R5)

What R4 left for the board commands (contract §3.5, §4.2–§4.7, §6; R5 in §9). Rustdoc on each item
is the reference; this page is the map, the seams, and what R5 must wire back in.

## State of the tree after R4

- `src/commands/boards/` and `src/commands/export.rs` are **not compiled**: their `mod` lines are gone
  from `src/commands/mod.rs` and their rows from `src/ipc/table.rs`. The files are untouched V1 code for
  R5 to rewrite. Re-add `pub mod boards;` and `pub mod export;` in `commands/mod.rs`, and their rows
  in the `command_table!` (`list_boards`, `drafts`, `board_thumbnails`, `open_board`, `new_board`,
  `reload_board`, `close_board`, `move_board`, `save_board`, `save_board_view`, `board_health`,
  `repair_board`, `publish_board`, `discard_changes`, `unpublish_board`, `delete_board`,
  `duplicate_board`, `set_board_folder`, `create_folder`, `rename_folder`, `delete_folder`,
  `board_refs`, `export_mermaid` (trusted), `export_mermaid_text`).
- `tests/commands/main.rs` has no `mod boards;`: re-add it with the rewritten `tests/commands/boards.rs`;
  `tests/commands/home.rs` has R4's half (R5 adds `list_boards` and `board_thumbnails` cases).
- `CLOSING_WRITES` in `src/host.rs` already lists every board write of contract §3.1 and is checked
  against `desktop/src/engine-host/router.ts` by `host::tests::closing_writes_match_the_routers`.
- `ui/src/ipc/generated/index.ts` was **not** regenerated (R5/R7 run `cargo xtask gen-ipc` once the board
  commands are back in the table; the xtask test `the_generated_types_are_up_to_date` fails until then).

## The `// R5:` seams

Every seam is in `src/commands/board_hooks.rs` (a stub body with a `// R5:` comment). The core calls
them; R5 fills them in and removes the markers.

| Hook | Called by | Must do |
| --- | --- | --- |
| `BoardExtras` (in `ProjectExtras::boards`) | — | Hold the live project's open board sessions (`BoardId → session` with its working file, document, revision, health), the health cache of **every** board of the project, and the `disk_check` mutex. Dropped with the extras when the project rests or switches worktree |
| `released(host, project, boards)` | `Host::detach_window`, after the core released the window's claims on `boards` of `project` | Drop those sessions; publish the project's board problems again |
| `refresh_health(host, workspace, extras)` | The coalesced refresh thread (one per project, `HostState::refresh_health`) after the project woke, after its `graph_updated`, and after `graph_updated` of a project in `dependents` | Recompute every board's health (cache by file revision and the graph generations of the projects its cards resolve to), then `workspace.set_board_problems(items)` |
| `dependents(host, project) -> Vec<ProjectId>` | `HostState::observe` on `graph_updated` of `project` | The live projects whose boards have cards from `project` |
| `boards_changed(host, workspace, extras, boards)` | `HostState::observe` on `boards_changed` (on its own thread) | V1 `check_disk` per working file: `board_conflict` to the holder (`host.deliver(&[window], &event)`), and refresh what the change made stale |
| `health_rows(extras) -> Vec<BoardHealthCount>` | `project_summary` | The boards with health items, most items first (the core truncates to 20 and counts `boardsNeedingReview`) |

Board health items reach `problems` and the row's `problems{count}` through
`Workspace::set_board_problems` (pushed, so the row count and `problems_changed` stay right); `problems`
itself needs no hook.

## Projects and liveness (`src/projects.rs`)

| API | Use |
| --- | --- |
| `ctx.project(&id) -> ProjectHandle` | A project without waking it (`unknown_project`). For `rests` commands |
| `ctx.live(&id) -> Arc<Workspace>` | Wakes it and extends its lease. For `wakes` commands (`open_board`, `new_board`, `save_board`, …) |
| `handle.wake()` | Same as `ctx.live`, from a handle |
| `handle.readers() -> Option<ReadPool>` | The stored graph **without waking**: the live pool, or a resting read-only pool (`None`: no graph). Use it to load another project's `BoardGraph` (§6.4: never wake it) |
| `handle.layout()` | Where `local/` is (fallback honored) without writing: `BoardDirs::new(handle.root(), &handle.layout().drafts())` works for resting projects too |
| `handle.root()`, `handle.name()`, `handle.id()`, `handle.confine(path)`, `handle.state()` | Folder, shared name, id, confinement, row state |
| `handle.claims() -> &BoardClaims` | The project's claims (below) |
| `handle.extras() -> Arc<ProjectExtras>` | The live project's command-side state (`extras.boards` is yours) |
| `projects.resolve_name(name) -> Option<ProjectId>` | D-3: the first project in workspace order with that shared name (the board's own project included). `None` = not connected |
| `projects.ids()` | Every project in workspace order (the `drafts` fan-out) |
| `projects.record_recent_board(project, board, title)` | `open_board` (most recent first, ≤ 20; writes `workspace.json`, sends `workspace_changed`) |
| `projects.move_recent_board(project, from, to)` | A board id changed (first Publish, Move back to drafts) |
| `projects.forget_recent_board(project, board)` | `delete_board` |

Interest (what keeps a project live) is computed by the core from: the project's claims, Home
selections, compile pins, and **the references of every claim** (`BoardClaims::set_references`). The
reaper wakes a resting project that gained interest and rests one that lost it once its lease ran out.
So R5 only keeps claims and their references current.

## Claims (`src/claims.rs`, one `BoardClaims` per project)

`claim(board, window)` (`board_held` with the holder), `release(board, window)`, `transfer(board, from,
to)` (`move_board`; any window id is valid now), `rename(from, to)` (board id changes),
`check(board, window)` (`board_held` / `board_not_held`), `holder(board)`, `set_references(board,
projects)` (the other projects the open board's cards resolve to: call after open, reload, save,
repair), `is_empty()`, `release_window(window)` (the core's detach). `remove_project` and
`switch_worktree` refuse with `boards_open` while `is_empty()` is false.

## `state.json` (`src/state.rs`, through the live workspace)

`workspace.local_state()` / `workspace.update_local_state(|state| …)` (kept in memory before the first
Compile, written after). `LocalState { background_updates, empty_folders: Vec<String> (sorted),
boards: IndexMap<BoardId, BoardView> }` with `board_view(id)`, `put_board_view(id, view)` (most recent
last; ≤ 500 boards, ≤ 200 pages), `move_board_view(from, to)`, `remove_board_view(id)`. `BoardView {page,
pages: {id: PageView {x, y, zoom, lens}}}` is the contract's `BoardView` (TS type generated once a
command uses it). Keeping `empty_folders` right (sorted, a folder leaves it once a board goes in or under
it) is R5's.

## Events

- Everything a workspace sends goes to every window: `workspace.emit(&WorkspaceEvent::BoardsChanged {
  project, boards, folders })` after a command writes board files (`folders: true` when folders may have
  changed). The watcher already reports outside edits, mapping `drafts/x.changes.json` to
  `.unfold/boards/x.json`, with `folders: true`. Call `workspace.note_own_write(path)` before writing a
  board file so the watcher doesn't report the app's own write.
- `board_conflict` is the only targeted event: `WorkspaceEvent::BoardConflict { project, board, file:
  WorkingFile::{Draft, Changes, Published}, saved_revision, disk_revision }`, sent with
  `host.deliver(&[holder], &event)` (or `ctx.deliver` from a command).

## Errors

`unknown_project`, `boards_open`, `not_a_worktree`, `too_many_projects` are `WorkspaceError` variants.
R5's `invalid_folder`, `folder_exists`, `folder_not_empty` are documented in `src/ipc/error.rs` and made
with `CommandError::new(code, message)`. `not_attached` is gone: board tests that used it now test
`board_not_held`.
