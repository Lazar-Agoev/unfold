# Commands and events (IPC)

This is the catalogue of the engine's commands and events: what each one means, when to use it,
and the rules every command follows. The **types** are not repeated here. They are defined once in
Rust, in `crates/unfold-workspace` (the command table in `src/ipc/table.rs`, the handlers and their
argument and result types in `src/commands/`), and generated into
`ui/src/ipc/generated/index.ts` by `cargo xtask gen-ipc` ([architecture §12](architecture.md#12-commands-and-events),
[U-25](decisions-and-tradeoffs.md#u-25-typed-commands-generated-from-rust)). The generated file is
the source of truth for shapes; never edit it by hand.

## How a command travels

```text
UI ── window.unfold.invoke(name, args) ──► preload ── port ──► engine host ── addon.dispatch(windowId, requestId, json)
                                                                                   │
UI ◄── result / {code, message} ◄──────── preload ◄── port ◄── engine host ◄───────┘  Host::dispatch_async → command thread
UI ◄── window.unfold.stream(...) messages ◄────────────────── sink {"kind":"stream",...}
UI ◄── window.unfold.on(type, ...) ◄───────────────────────── sink {"kind":"event",...} / {"kind":"broadcast",...}
```

- A command is `{"command": "<name>", "args": {…}}`. Arguments are camelCase and strict: an
  unknown field is an `invalid_args` error.
- Each command runs on its own Rust thread for one window (`windowId`), against the folder that
  window shows. Long commands check their request's cancellation; `cancel(requestId)` stops them.
- A result is JSON (`null` for commands that return nothing). A failure rejects with
  `{"code", "message"}` ([codes](#error-codes)). A panic in Rust becomes `internal`; the process
  keeps running.
- Streamed commands (`compile`, `search_text`) send messages before they resolve:
  `{"kind":"stream","windowId":1,"requestId":7,"message":{…}}` (`SinkMessage`). The final result
  is the command's resolution.
- Events go to every window attached to the folder they concern (`board_conflict` only to the
  window holding the board): `{"kind":"event","windows":[1,2],"event":{"type":…}}`.
- App-wide events (`preferences_changed`) go to **every** window, including welcome windows that show
  no folder: `{"kind":"broadcast","event":{"type":…}}`. Rust can't list those windows (a window
  without a folder never calls `openWorkspace`, so no workspace knows it), so the engine host's
  router posts a broadcast to every window port it has; each window receives it as an ordinary
  `{kind:"event", event}`.
- **Trusted commands.** The engine host adds `"trusted": true` to a command the **main process**
  sends, and never to one a window sends (the router builds the JSON itself, so a window can't add
  it). `export_mermaid` (writes to an absolute path from a save dialog) and `folder_windows` (reads
  another folder's `state.json`) require it; from a window they fail with `untrusted` (the router
  refuses them, and any window message carrying `trusted`, before they reach Rust). The main
  process sends its own requests to the engine host over the utility process's parent port
  (`{type:"request", id, windowId, command, args}`, answered `{type:"result", id, ok, …}`;
  `desktop/src/shared/transport.ts`). The UI exports through the window command
  `export_mermaid {boardId | entrance, title?}`, which shows the save dialog in the main process.
- **Paths.** Every path a window names is folder-relative, `/`-separated, and confined: `..`,
  absolute paths, backslashes, and symlinks that leave the folder are rejected with `invalid_path`
  ([architecture §13](architecture.md#13-security-and-privacy)). Board IDs are paths too.
- **Payloads stay small** (U-25): one card, one tree level, one page of lines, one page of
  callers, at most 2,000 text matches.

The generated module exports, besides every payload type:

| Export | What |
| --- | --- |
| `Commands` | `name → { args; result; stream? }` |
| `CommandName`, `CommandArgs<C>`, `CommandResult<C>` | Lookups into `Commands` |
| `StreamCommandName`, `StreamMessage<C>` | The streamed commands and their message types |
| `WorkspaceEvent`, `EventType`, `Events` | Every event, its `type`, and a map keyed by `type` |
| `SinkMessage` | What the addon's sink receives (for the engine host) |
| `CommandError` | `{code, message}` |
| `COMMAND_NAMES`, `STREAM_COMMANDS` | The names at run time |

## The addon's surface (`Host`)

`unfold_workspace::Host` is what the napi-rs addon wraps (brief §6). It is thread-safe; the
`_async` entry points run on a new Rust thread and call `done` there, so the addon resolves its
promise through a thread-safe function and never blocks the JavaScript thread.

| Addon | Host | Notes |
| --- | --- | --- |
| `init(options)` | `init(&str)` | `{"appDataDir","logsDir","fallbackCacheDir"}`; once |
| `subscribe(sink)` | `subscribe(Fn(String))` | Every event and stream message |
| `openWorkspace(windowId, root)` | `open_workspace_async(window, root, done)` | Resolves with `OpenedFolder` JSON; sends `folder_opened` |
| `closeWorkspace(windowId)` | `close_workspace_async(window, done)` | Waits (up to 15 s) for the window's board writes and `save_window_state` already dispatched, so a final save lands; then releases the window's boards; the last window closes the folder |
| `dispatch(windowId, requestId, command)` | `dispatch_async(window, request, json, done)` | `done(Ok(json))` or `done(Err(CommandError))`; the rejection message is `error.to_json()` |
| `cancel(requestId)` | `cancel(request)` | No-op when finished |

Blocking variants (`open_workspace`, `close_workspace`, `dispatch`) exist for tests and tools.
`Host::shutdown` closes every folder when the app quits.

## Commands

Window and dialog commands (`new_window`, `focus_window`, `choose_folder`, `close_folder`,
`choose_export_path`, …) belong to the main process and are typed in `desktop/src/shared/unfold-api.ts`, not
here. Closing a folder is one of them: which folder a window shows is a main-process lifecycle concern, so
`close_folder` re-attaches the window to the engine host with no folder, and the engine host's
`closeWorkspace` detaches it from the folder's workspace (after any board write or window-state save already sent; the folder's last
window closes it). The engine has no command of its own for it.

### Folders

| Command | Does |
| --- | --- |
| `folder_summary` | The first-Compile screen (§11.6): files that would be graphed by language, skipped folders with reasons, where `local/` would live (fallback reason), whether the settings file exists or is invalid. Walks the folder; writes nothing. |
| `recent_folders` | Recent folders, most recent first, and the welcome window's bounds. |
| `forget_recent_folder` | Removes one from the list. |

### Compile

| Command | Does |
| --- | --- |
| `compile` | `mode: "check"` (Compile; the default) or `"from_scratch"` (Rebuild from scratch). Streams `CompileProgress` (`stage`, `done`, `total`), resolves with `CompileOutcome`. The first Compile creates `.unfold/`. Cancel with `cancel(requestId)` or `cancel_compile`; it then rejects with `cancelled` and the previous graph stays. |
| `cancel_compile` | Cancels the running Compile or Rebuild and every queued one (any window's); returns `{cancelled}`, whether there was one. Their `compile` requests reject with `cancelled`. A Compile or Rebuild can be cancelled until its store commit finishes: a cancel during the commit rolls the transaction back, the engine doesn't adopt the update, and the graph, facts (last-good ones included, even for a Rebuild), and status stay as they were. Background updates are not affected: once one starts committing it finishes ([compiling.md](compiling.md#cancellation-and-threads)). |
| `set_background_updates` | Turns background updates on or off for the folder (remembered in `state.json`); returns the status. |
| `graph_status` | The status bar's state (§7.9). It also arrives as `graph_status` events. |
| `problems` | The Problems list (U-28): files that don't parse or can't be read, settings errors, discovery and input problems, a failed compile, and the health of open boards. |
| `index_log` | The last updates' statistics, oldest first, and the graph's counts. |

Commands that read the graph (`card`, `relations`, `entrance_tree`, `file_outline`, …) fail with
`not_compiled` before the first Compile; `list_dir`, `file_view`, `search` (files), and
`folder_summary` work on any folder.

### Files

| Command | Does |
| --- | --- |
| `list_dir` | One level of the tree, folders first. Each entry carries what its quiet marks need: `language` (a graphed source), `state` in the graph (`last_good` = the amber "doesn't parse" warning), `pending` (changed, not in the graph yet; for a folder, something inside), `entrances` (`→N`), `launches` (`N launches` on `package.json`/`pyproject.toml`), `skipped` (`ignored`/`default` for ignored entries; `excluded`/`not_included`/`language_off`/`javascript` for sources that aren't indexed), and `unfold` for `.unfold/`. The `●` mark is the UI's: it knows its boards. |
| `file_view` | A source tab (§11.4): the whole file's render model, or with `range` one page of lines for virtualization. `graph` says how the text relates to the graph: `current` (colors, outline, folds, links), `not_compiled` (the file changed since the graph; current text with colors, no links), or `not_indexed`. Offsets in a page are UTF-16 offsets from the start of the **file**; `textStart` is where the page's `text` begins. Binary and very large files come back with `unshowable`. |
| `file_outline` | An indexed file's definitions in source order, nested by where they're written (classes hold their methods), with their keys. |
| `open_in_editor` | Runs the preferences' editor command (for example `code -g {path}:{line}`; `{path}`, `{line}`, `{column}` are replaced, and the path is appended when the template doesn't name it) **without a shell**; with no editor set, opens the file as text in the system's text editor (`open -t` on macOS, Notepad on Windows; elsewhere the desktop's handler, refused with `no_editor` for executable files and `.desktop` launchers), never with its default app, which could run it. Folders are refused. Returns which. |
| `reveal_in_finder` | Shows the file or folder in the system's file manager. |

### Graph

| Command | Does |
| --- | --- |
| `card` | A block key gives a **block card**: the header (kind, name, qualified name, path, lines, language, file state, source freshness, owner), the Code model (highlighted, folds per the card policy, call links), and the Flow model ("Paths from written structure"). `lens: "code"` or `"flow"` builds only one. `board` names an open board the window holds: its authored labels label the Flow nodes. An `entrance:…` key gives an **entrance card**: its header, its evidence (the Code model of a registration call, or the verified text of a configuration entry such as a `package.json` script), its targets in registration order with names and notes, and the applications it belongs to. |
| `peek` | The hover over a call link: a block's first written line (its signature, when the source verifies) and location, or an entrance's title and location. |
| `relations` | For a block: one page of **callers** (`callersOffset`/`callersLimit`, at most 500; `callersTotal`, `callersNext`), **outgoing** edges grouped as on the board (`onBoard` keys the window sends), openable, and external or unresolved (with their module or reason in words), the qualified names of resolved targets, and **reached from** (the non-process entrances whose closures contain it). |
| `entrance_tree` | The Entrances view (§11.3): applications named after their startup, with their launches and their entrances (method column, trigger, language tag); then "Not reached from a launch" (`unreached`) and "Launches into code that isn't indexed" (`unindexedLaunches`). |

### Search

| Command | Does |
| --- | --- |
| `search` | Definitions, entrances, and files, grouped and ranked. Definitions and entrances are FTS5 candidates (every word of the query is a substring of the name, qualified name, or path) ranked by `nucleo` fuzzy score (name ×4, qualified name ×2, path ×1), then shorter name, then key. Files are the folder's files under discovery's ignore rules, fuzzy-matched on their path. `kinds: ["files"]` serves ⌘P and `["definitions"]` ⌘T; `limit` per group (50, at most 500). Match offsets for highlighting are UTF-16. |
| `search_text` | Text or regex (`regex`, `caseSensitive`, `wholeWord`) across the same file list, never `.unfold/`, skipping binary files. Streams one `TextSearchFile` per file **in path order** (each line with its number, text, and UTF-16 match ranges; a very long line is a window around its first match, with `column`), stops after 2,000 matches (`truncated`), and is cancellable. |

### Boards

A board's **ID** is its folder-relative file path: `.unfold/boards/<name>.json` (published) or
`.unfold/local/drafts/<name>.json` (draft; the ID stays the same when `local/` is in the fallback
location). Board documents travel as `BoardDocument`, exactly the `unfold-board` v1 file shape.

A board is open in **one window at a time** (U-37). Every board command except `list_boards`,
`open_board`, and `new_board` checks that the calling window holds the board, and fails with
`board_held` (another window has it) or `board_not_held`.

| Command | Does |
| --- | --- |
| `list_boards` | Published boards and drafts: title, card count, the window holding it, and a `label` only when something is wrong (`1 missing · 2 outdated`). |
| `open_board` | Claims the board for the calling window and returns it (`status: "opened"`) with its revision, health, and outline. If another window holds it, returns `status: "held"` with `heldBy`, and the UI asks the main process to focus that window. |
| `new_board` | Creates an empty draft (needs a compiled folder), opens it, and returns it. |
| `save_board` | Validates the edited document against the graph (no new missing references, broken groups, or held references), signs new descriptions and labels from verified source, writes it atomically, and returns it as saved. Drafts autosave here. Refused with `board_conflict` when the file changed on disk since the window loaded it, unless `force: true`. |
| `reload_board` | Reads the board from disk again (after an outside edit) and returns it. |
| `close_board` | Releases the claim. Save first: unsaved edits live in the window. |
| `move_board` | Hands the claim to another window of the folder (Move to New Window). |
| `publish_board` | Writes the draft to `.unfold/boards/<name>.json` (`name` defaults to the slug of the title) without ever overwriting, and removes the draft; the board's ID changes. A taken name answers `status: "name_taken"` with the existing `path` and a free `suggestion`; nothing is written. |
| `unpublish_board` | Moves a published board back to drafts. |
| `rename_board` | Changes the title (saved like an edit) and, for a published board, its file name (never overwriting; `name_taken` as above). |
| `delete_board` | Deletes the board's file (the UI asks first) and closes it. |
| `board_health` | Health (missing, outdated, broken groups, to review, each with its repairs) and outline (cards in follow order; Flow nodes still showing code names). |
| `repair_board` | Applies one repair a health item offers (`reconnect`, `remove_card`, `remove_label`, `ungroup`, `remove_description`, `resign`, `accept`) and saves. |

Open boards' health is recomputed after every graph update and feeds the Problems list (one item
per health problem, its path the board ID). Board files the app writes are announced with
`boards_changed` to every window of the folder, so other windows refresh their Boards view.

**Outside edits.** When the watcher sees an open board's file change (an agent or a teammate wrote
it, or it was deleted), the window holding it gets `board_conflict` with the revision it last
loaded or saved and the revision on disk. A window without unsaved edits calls `reload_board`; one
with unsaved edits asks which to keep: `reload_board` takes theirs, `save_board` with `force: true`
keeps its own.

### Export

| Command | Does |
| --- | --- |
| `export_mermaid` | Writes a board (`board`, its last saved version) or an entrance with its handlers (`entrance`) as Mermaid to the absolute `path` the person chose in the main process's save dialog. **Trusted only.** |
| `export_mermaid_text` | The same text, for the clipboard; no path. |

### Settings

| Command | Does |
| --- | --- |
| `get_preferences` / `set_preferences` | App preferences: theme, editor command, background-update default, thread limit (applies to folders opened afterwards), key bindings (`keyBindings`: action id → shortcut such as `Mod+Shift+K`, `""` for none; the UI validates and applies them, see [ui-shell.md](ui-shell.md#custom-key-bindings)), and the update check (`updates.enabled`). `set_preferences` takes the whole object and, once it is saved, sends `preferences_changed` with it to every window. |
| `get_folder_config` | `.unfold/config.toml`: whether it exists, its text, the validated settings (languages, scope, disabled detectors, Python projects, manual entrances), or the error with line and column. |
| `update_folder_config` | Applies `SettingsEdit`s in order, all or nothing, keeping comments and formatting: exclude from graph, include in graph, graph only this folder, remove one scope pattern as written, add/replace/remove a manual entrance, set/remove a Python project, turn a detector on or off. An edit that would make the file invalid fails with `invalid_settings`; a broader rule in the way fails with `settings_conflict`. Sends `config_changed`; the next background update applies it. |
| `app_info` | The username that signs edits (`person:<username>`) and the engine version. |
| `diagnostics` | Copy Diagnostics (architecture §13): a redacted summary for bug reports: engine build and version, producers, OS and architecture, and for the window's folder the graph state and completeness, files by language and state, the last update's statistics, problems by kind, background updates, watch mode (events, scan), whether the fallback location is in use, the lock state, and settings counts. No source, no file contents, and no paths (not even folder-relative ones); the folder path, home folder, and username are replaced with placeholders should any slip in. The UI adds the app, Electron, and Chromium versions. Works without a folder. |

### Window state

Windows are remembered by a stable `key` the main process assigns (window IDs change every run).
A window learns its key from the window command `window_info` (and the `folder_changed` shell event
when it switches folders). The main process calls `folder_windows`, `save_window_bounds`, and
`forget_window_state` itself; the UI calls `window_state` and `save_window_state`.

| Command | Does |
| --- | --- |
| `window_state` | What the window with `key` remembered: tabs, active tab, layout, open boards, bounds. |
| `save_window_state` | Remembers it in the folder's `state.json` (kept in memory until the first Compile creates `local/`). Bounds saved separately are kept when `bounds` is absent. A save still waiting when the window closes is sent at once (`pagehide`), and the window's detach waits for it. |
| `forget_window_state` | Forgets a window the person closed, so it doesn't come back. |
| `folder_windows` | A folder's saved windows, so the main process can restore them before any window opens it. **Trusted only.** |
| `save_window_bounds` | A window's position and size: in its folder's `state.json` with a `key`, or the welcome window's in `recent.json` without one. |

## Events

| Event | When | Payload |
| --- | --- | --- |
| `folder_opened` | A window attached to a folder (sent to that window) | folder, name, compiled |
| `graph_status` | The status bar's state changed kind (not on every progress tick) | the status |
| `compile_progress` | A compile or update advanced (throttled) | stage, done, total |
| `graph_updated` | A commit changed the graph | changed and removed files, changed block keys, changed entrance keys, `inputsChanged`, `everything` (lists too long, or a first or full commit: refresh all), `complete` |
| `problems_changed` | The Problems list changed | count |
| `files_changed` | The watcher saw files appear, change, or disappear | paths, `truncated` |
| `boards_changed` | Board files changed (outside the app, or written by a board command) | published IDs, draft file names |
| `board_conflict` | An open board's file changed on disk outside the app (only to the holding window) | board ID, `savedRevision`, `diskRevision` (`null` when deleted) |
| `config_changed` | `.unfold/config.toml` changed | folder |
| `preferences_changed` | `set_preferences` saved the app preferences (any window). Sent to **every** window, folder or welcome, as a broadcast | `preferences`: the whole saved `Preferences` |

The UI invalidates its TanStack Query caches on `graph_updated`, `boards_changed`, and
`config_changed` (§11.8). On `preferences_changed` it replaces the `get_preferences` result with the
event's payload (no refetch), and the shell reapplies the theme (`data-theme`) and key bindings from it,
so every open window follows a change at once.

## Error codes

| Code | Meaning |
| --- | --- |
| `invalid_command` | The command JSON is malformed or names no command |
| `invalid_args` | The arguments don't fit the command |
| `invalid_path` | A path leaves the folder or isn't folder-relative |
| `not_found` | The key, path, or board doesn't exist |
| `not_compiled` | The folder has no graph yet |
| `not_attached` | The window shows no folder |
| `untrusted` | The command is accepted only from the main process |
| `board_held` / `board_not_held` | Another window holds the board / the calling window doesn't |
| `board_invalid` | A board file or document breaks the `unfold-board` format |
| `board_problem` | A save would introduce a missing reference, broken group, or held reference, or text can't be signed (source changed: compile first) |
| `board_conflict` | The board changed on disk since it was loaded |
| `invalid_settings` / `settings_conflict` | A settings edit would make the file invalid / needs a broader rule changed |
| `cancelled` | The request was cancelled |
| `compile_failed`, `store`, `reader`, `io`, `not_a_folder`, `open_elsewhere`, `closed` | From the workspace (`WorkspaceError::code`) |
| `not_initialized`, `already_initialized` | `init` wasn't called, or was called twice |
| `internal` | A bug: a panic or a broken invariant |

## Adding a command

1. Write the handler in the right `crates/unfold-workspace/src/commands/` module:
   `pub fn my_command(ctx: &Ctx<'_>, args: MyArgs) -> CommandResult<MyResult>`. Get the folder
   with `ctx.workspace()?`, confine every path with `workspace.confine(path)?`, check
   `ctx.check_cancelled()?` in long loops, stream with `ctx.stream(&message)`, and call
   `ctx.require_trusted(…)?` if it needs a path from a dialog.
2. Give the argument and result types `#[derive(Deserialize, TS)]` / `#[derive(Serialize, TS)]`
   with `#[serde(rename_all = "camelCase")]` (and `deny_unknown_fields` on arguments). Optional
   arguments are `Option<T>` with `#[serde(default)]` and `#[ts(optional)]`. Document every type
   and field: the docs become the generated JSDoc. Don't send whole-graph payloads; page them.
3. Add one line to `command_table!` in `src/ipc/table.rs`:
   `/// What it does.` then `"my_command" => MyCommand(MyArgs) -> MyResult = module::my_command;`
   (add `, stream MyMessage` for a streamed command).
4. Run `cargo xtask gen-ipc`, then `pnpm --filter ui typecheck`. CI runs
   `cargo xtask gen-ipc --check`, which fails when the generated file is stale.
5. Test it through `Host` in `crates/unfold-workspace/tests/commands/`, and add it to this
   catalogue.

The addon and the engine host don't change when a command is added.
