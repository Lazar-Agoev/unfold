# Core rebuild log

The build log for [the core rebuild plan](core-rebuild-plan.md) (rule 10). Each step records what was done, the
choices made, deviations from the plan or the designs, and the gate results. It is kept current so the work can resume
after a context reset.

**Started:** 2026-10-13, on `main` at `06569d5`, with `docs/analysis/` and `docs/designs/` untracked.

**Git:** the kickoff said "Don't commit or push unless I say so"; the owner then allowed commits ("feel free to keep
committing changes as you go"). So each green step is committed with its title (plan rule 8); sub-agents stay off git;
nothing is pushed. The untracked design docs this build didn't write stay uncommitted.

## Where the build is

| Step | State |
| --- | --- |
| Before step 1 | done |
| 1. Tooling, tests, and docs | done, committed |
| 2. One window | done, committed |
| 3. Projects, Home, and entrance screens | done, committed |
| 4. Boards are files | in progress |

## Working setup

- Cargo: `source "$HOME/.cargo/env"` first.
- Shared target dirs for parallel sub-agents: `target/agent-1`, `target/agent-2`, `target/agent-3` (seeded from
  `target/debug` with `cp -Rc`). Delete them when the build ends.
- Scratchpad (session-only): `/private/tmp/claude-501/-Users-lazaragoev-Desktop-repos-unfold/4fd2ec4c-f0f1-4976-b91e-93fb8771188b/scratchpad`.

## Before step 1

### Baseline on the untouched tree

Run 2026-10-13 at `06569d5`, before any change.

| Check | Result |
| --- | --- |
| `just test` | **Red.** 17 failures in `unfold-workspace --test commands` (cargo stopped there). Cause: stray local state in `testdata/playgrounds/.unfold/local/` (old drafts and a cache), which the plan's "Before step 1" deletes |
| `just js-check` | **Red.** typecheck and lint pass; `@unfold/node`'s test "wakes a selected project…" fails for the same stray cache (`compiling` instead of `not_compiled`); the run stops there |
| `pnpm --filter @unfold/ui test` | 150 files, 1,190 tests pass |
| `pnpm --filter @unfold/desktop test` | 37 files, 331 tests pass |
| `just check` (fmt, clippy) | Green |
| `cargo xtask gen-ipc --check`, `notices --check`, `cargo deny check`, `cargo machete` | Green |
| `cargo xtask oracles check ts-orders entrances py-orders mixed-playground python-ranges` | **Red.** All six corpus/language pairs report "stale oracle": the corpus hash covers `unfold.toml` inputs, which changed after the oracles were generated on 2026-10-11 |
| `knip` (not wired yet; run with `npx knip@5`) | **Red.** 6 unused files (entry points knip doesn't see), 1 unused dependency (`electron-updater`), 31 unused exports, 174 unused exported types |

### Stray state deleted

`testdata/playgrounds/.unfold/` (with `local/cache`, `local/drafts`, `local/state.json`),
`testdata/playgrounds/ts-orders/.unfold/`, and the two `node_modules/` folders in
`testdata/fixtures/typescript-evidence/`.

After deleting it, `cargo test --workspace --no-fail-fast`: 941 pass, **2 fail**, 5 ignored. The two failures are
`launches::{configured_launches_group_into_applications_by_category, check_launch_previews_a_draft_against_the_current_graph}`:
they expect `evals/` files that `.gitignore` keeps out of the checkout (`not_found` instead of `not_indexed`). Step 3
deletes both tests with `commands/launches.rs`; until then they are known baseline failures.

### Real-data corpus

`../pathway-agents` cloned with `cp -Rc` to `<scratchpad>/pathway-agents` (3.1 GB, APFS clone). The original is not
touched. **For the owner:** its untracked `unfold.toml` uses `[detectors]`, `[[application]]`, and two process
`[[entrance]]` tables, so it stops parsing in step 8. Rewrite it with `[[app]]` tables (or ask for it to be
rewritten); step 8 writes the clone's tables as the plan describes.

## Step 1 — Tooling, tests, and docs

**Done.** Two sub-agents (Rust side; JS/CI/docs side), verified and gated by the orchestrator.

**What was done**

- **Parity:** deleted `testdata/reference/{poc,poc-reader,canonical}`, `divergences.toml`, `corpora.toml` (README
  trimmed to the oracles); xtask `parity`, `ledger`, `reference`, `compare`, `canonical_diff`, `canonical_check` and
  their subcommands (plus `compile::copy_folder`, which only parity used); `tools/oracles/export-reader.mjs`; the parity
  tests in both language crates and the reader; `testdata/fixtures/{classes,worked,playground-v1}` and the discovery
  test that compared with the proof of concept; `canonical/view.rs`, `OLDEST_VERSION`, `read_versioned`,
  `write_version`; `csv`, and the TypeScript crate's `toml`/`anyhow` dev-dependencies. `read_graph`, `PathMapping`,
  and `utf16_to_bytes` moved into `tools/xtask/src/oracles/` first.
- **Goldens:** `testdata/goldens/{ts-orders,py-orders}.jsonl` written with `cargo xtask export` (version 2); the
  reader's `tests/common/mod.rs::load` and `flow/tests.rs` read them. No kept assertion needed changing.
- **Fuzzing, budgets, coverage:** `fuzz/` and its `exclude`; `e2e/budgets/`, the measurements reporter, the
  `e2e-budgets` recipe, the `budgets` project; the store and workspace benchmarks; `coverage.spec.ts`,
  `e2e/src/coverage.ts`, and the keyboard and "Where V1's parts went" tables in `docs/ui-shell.md`.
- **CI:** `nightly.yml` and `release.yml` deleted; `pr.yml` is one macOS job without parity, bundling, or the smoke
  test, plus a knip step; `cargo-deny` stays on Ubuntu.
- **Packaging:** electron-builder's signing, publish, dmg/zip, notarize, Gatekeeper, document types, Windows and Linux
  sections; `bundle`, `smoke`, `windows-shell` recipes; `desktop/scripts/`; `@electron/fuses` and `playwright-core`;
  the `.ico` note in `tools/icons`.
- **Docs:** `docs/entrances.md`, `docs/proposals/`, `docs/reviews/` and every link to them (12 code comments too);
  minimal edits where reference docs described what was deleted.
- **knip 6.41.0** pinned as a root devDependency with a `knip` script. Its electron-vite and vite plugins find the real
  entries, so no entries were added; the baseline's "unused files" came from knip 5. Fixed: one unused export
  deleted, `export` dropped from 18 values and 8 types used only in their own file.
- **Oracles refreshed** (`cargo xtask oracles refresh` on the five corpora): only the header hashes changed; every
  answer row is identical. `../graph-compiler` stayed clean.

**Choices**

- `testdata/mermaid/` deleted now: only the deleted `mermaid_reference.rs` wrote it (step 5 deletes the rest of
  Mermaid).
- The validator's version-1 mode (`without_typed_records`, the `complete` flag) went with the old export reader:
  typed records are always required.
- e2e stays outside knip (it already was in `ignoreWorkspaces`).
- `electron-updater` stays until step 2, which deletes it.

**Deviations:** none.

**Gate** (2026-10-13)

| Check | Result |
| --- | --- |
| fmt, clippy | pass |
| `cargo test --workspace --no-fail-fast` | 893 pass; 2 fail: the known `launches::` baseline failures (step 3 deletes them) |
| gen-ipc, notices, deny, machete | pass |
| `oracles check` (5 corpora) | pass. Precision exact everywhere; recall at its floor: ts-orders 71/71, entrances 3/3, py-orders 91/96, mixed TS 71/71, mixed Python 91/96, python-ranges 3/3 |
| `pnpm -r typecheck`, `lint`, unit tests | pass (ui 1,190 tests, desktop 331) |
| `pnpm knip` | pass |
| `just addon-test` | 10/10 |
| `just e2e` | 44 journeys pass (2.0 min) |
| Residue `rg -i 'parity\|divergence\|poc-reader\|fuzz\|budgets\|notariz'` | only "fuzzy" name matching in search and a made-up `fuzz.py` in a robustness test |

## Step 2 — One window

**Done.** Four sub-agents in sequence: Rust and desktop in parallel, then the UI, then the journeys.

**What was done**

- **Engine (`unfold-workspace`):** board claims became a board-session registry, `open_boards.rs::OpenBoards` (one
  per project). It never refuses an open, keeps a session per opened board while the app runs, drives project
  liveness and the `boards_open` refusals, and `Host::detach_window` closes the window's boards after its closing
  writes land. Board commands other than reads and folder commands refuse a board that isn't open
  (`board_not_open`). Deleted: `claims.rs`, `BoardHeld`/`BoardNotHeld`, `move_board` (the IPC command; the folder
  helper stays), multi-window state, `windows.json`, per-board view state and `save_board_view`, `indexmap`, the
  theme/threads/key bindings/updates/agents preferences, the `preferences_changed` event, `diagnostics` (its
  `username()` moved into `settings.rs`), the non-macOS `open_as_text`. New: `get_settings`/`set_settings`
  (`settings.json` = `{ editorCommand? }`), `open_in_editor` running the command or `/usr/bin/open -t` (error
  `editor_not_found` asks for the full path), `window.json` = `{ bounds, maximized, board }` behind main-process-only
  `window_state`/`save_window_state`.
- **Desktop:** one `AppWindow` with `titleBarStyle: "hidden"` and the window buttons at (16, 18) over the sidebar;
  closing it keeps the app running and an agent, the Dock, a second launch, or Add Project reopens it. Window restore
  reads `window_state` and saves bounds, maximized, and the shown board (500 ms debounce, and on close before the
  detach). Menu: app menu, File (Add Project…, Close Window), Edit (Undo/Redo as `menu_command`, then roles), View
  (full screen; reload and dev tools in development), Window. The MCP server always listens on `127.0.0.1:29180`, no
  fallback port, reporting `port_taken`. Deleted: several windows, the updater and `electron-updater`, the dark
  background, the Agents switch and port, `open-file`/argv/second-instance folders and drops, the non-macOS
  branches, the removed menu items, `MAIN_EVENTS`.
- **UI:** the shown-board store (`state/shownBoard.ts`), reported with `report_shown_board` and restored from
  `window_info().board` at boot; every opened board keeps its session and undo history. `BoardView` applies the
  `menu_command` undo/redo. Settings (`ui/src/appSettings/`) opens from the sidebar's gear, per the frame. The
  sidebar is a fixed 264 px with a 48 px drag bar for the window buttons; the board's top bar drags too. Remove from
  workspace closes the project's open boards first. Deleted: tabs, `ui/src/keyboard/` (`focus-targets.ts` moved to
  `lib/`), KeyChip, every shortcut prop, Add's `>` mode, Go to File/Function, the dark theme, transfer/session
  adoption, `drop.ts`, `Resizable`, Copy Diagnostics, per-board view saving.
- **Journeys:** `keyboard.spec.ts` and `windows.spec.ts` deleted; the engine-host restart test moved to
  `home.spec.ts`. Journeys add projects through the sidebar's Add Project… (the command-line folder is gone), drive
  Undo/Redo through the app menu, and use the Settings dialog.

**Choices**

- Board commands refuse a board that isn't open, rather than opening it on demand: sessions and registry entries stay
  one-to-one.
- The settings type is `AppSettings`, to avoid clashing with the engine's project `Settings`.
- The board's top bar keeps today's lens, Share, and Publish until steps 4, 5, and 11 remove them.
- Settings saves the editor command when the field is left, on Enter, and on close (the frame has no Save button).
- Escape: a window-level Escape closes the peek sheet (until step 11 deletes it) unless a field, menu, or dialog took
  the key; the selected line's contract closes first.
- Add's input is labelled "Find" now that it runs no commands.
- The agent view report still has a `tabs` field (desktop's `ViewReport`); step 15 renames it with the MCP tools.

**Deviations**

- Sub-agents ran `git mv` twice despite the no-git rule; both were undone (index reset to clean), and later briefs
  forbid every git command that changes the index or the working tree.
- No journey covers window restore now that `windows.spec.ts` is gone; desktop unit tests cover restore of bounds and
  the last board. Step 13's `home.spec.ts` replaces today's journeys.
- One flaky step seen once in `boards.spec.ts` (the failed-save Retry); it passed 6/6 on repeat and in both full
  runs. Today's journeys are replaced in step 11.

**Gate** (2026-10-13)

| Check | Result |
| --- | --- |
| fmt, clippy, gen-ipc, deny, machete, oracles | pass |
| `cargo test --workspace --no-fail-fast` | all pass but the 2 known `launches::` baseline failures |
| notices | regenerated (`electron-updater`, `indexmap` gone), then pass |
| typecheck, lint, unit tests, knip, addon test | pass (ui 1,041 tests, desktop 321) |
| `just e2e` | 38 journeys pass |
| Residue `rg` | nothing |
| Screenshots (Home, Settings, a board) vs core-scope.html | Settings matches the frame; the window has no title bar |

## Step 3 — Projects, Home, and entrance screens

**Done.** Rust and desktop agents in parallel, then the UI, then the journeys; small cleanups by the orchestrator.

**What was done**

- **Engine:** deleted worktrees (`git.rs`, `git_info`, `switch_worktree`, `NotAWorktree`, `ProjectRow.git`, and the
  de-duplication by repository: a folder now matches a row only by its own path), `compile_preview`, `scope_dir`,
  `discovery/report.rs` and `Discovery.skipped`, `readd_project` and the removed-project records, recent boards
  (nothing read them once the `workspace` result dropped them), pause/resume and the `Paused` states, Rebuild from
  scratch (`CompileMode` is gone: `compile` takes `{ project }`; `Writer::clear` stays), the fallback location and
  the watcher's network-volume scanning (`volume.rs` stays), the activity log, Reveal in Finder and `opener`, the
  launch commands with `LinkInputs`/`linked_from`/`Engine::link_inputs`, `deploy_import` and `saphyr`, and their
  fixtures. Engine pieces left without callers went too: `plan_draft`, the launch editor's `Suggestion`,
  `context_preview`.
- **`add_project` compiles at once.** An unwritable folder (read-only, on a network volume, or with `.unfold/`
  through a link) is refused with `folder_not_writable` and a message that Unfold needs to write `.unfold/` there.
- **`project_summary`** = state, status, `updatedAtMs`, files, languages, and Health (`doesntParse`,
  `boardsNeedingReview`, `boards`).
- **Open `unfold.toml`:** `open_in_editor` on the path `unfold.toml` creates an empty file first when the project has
  none (`create_new`, never through a link, after confinement); no new command.
- **Desktop:** the fallback folder and transport field; the MCP setup tools lost `compile_preview`, `rebuild`,
  `set_detector`, `set_background_updates`, and the git fields; `add_project` reports the compile like
  `compile_project`; the setup guide lost its Detectors and Background updates sections.
- **UI:** deleted the project settings dialog (and moved the app Settings dialog to `ui/src/settings/`), the worktree
  button, the first-Compile screen and first launch, "Opened before", pause/resume, the Activity view, the
  "compiled" toast, Reveal in Finder, `ui/src/launches/`, Add's entrance browser, Add launch/Add entrance…, the
  card menu's "Add as entrance…" (it opened the deleted launch editor; step 13 builds the new popover), Recents, and
  components left without users (`Badge`, `Checkbox`, `Switch`, `TreeView`). Built the project menu (Compile again,
  Cancel, Open `unfold.toml`, Remove from workspace…), the state line, the Health line, and the empty workspace.
- **Journeys:** `settings.spec.ts` deleted (its app-Settings test moved into `home.spec.ts`); first-launch, home,
  project-menu, and background trimmed; a read-only folder is refused; Open `unfold.toml` runs a stub editor.

**Choices**

- Home's amber Health items are buttons (a board needing review opens it; the rest open the Health view, which stays
  without its Activity half).
- `add_project` (MCP) waits by queuing a second compile behind the one adding starts; that compile finds nothing to do.
- The store's `Reader::stats` stays: store tests use it to count rows (it was described as the Index log's; reworded).
- A project's default name is its folder's name.

**Deviations:** none.

**Gate** (2026-10-13)

| Check | Result |
| --- | --- |
| fmt, clippy, `cargo test --workspace` (all pass), gen-ipc, notices, deny, machete | pass |
| `oracles check` | pass, unchanged from step 1 |
| typecheck, lint, unit tests, knip, addon test | pass (ui 923 tests, desktop 322) |
| `just e2e` | 33 journeys pass |
| Residue `rg` | nothing |
| Screenshots: Home and the project menu vs core-scope.html | match the Home and Menus frames |
