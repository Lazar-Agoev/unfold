# Commands and events (IPC)

This is the catalogue of the engine's commands and events: what each one means, when to use it,
and the rules every command follows. The **types** are not repeated here. They are defined once in
Rust, in `crates/unfold-workspace` (the command table in `src/ipc/table.rs`, the handlers and their
argument and result types in `src/commands/`), and generated into
`ui/src/ipc/generated/index.ts` by `cargo xtask gen-ipc` ([architecture §12](architecture.md#12-commands-and-events),
[U-25](decisions-and-tradeoffs.md#u-25-typed-commands-generated-from-rust)). The generated file is
the source of truth for shapes; never edit it by hand. The window commands the main process handles
are typed in `desktop/src/shared/unfold-api.ts`.

## How a command travels

```text
UI ── window.unfold.invoke(name, args) ──► preload ── port ──► engine host ── addon.dispatch(windowId, requestId, json)
                                                                                   │
UI ◄── result / {code, message} ◄──────── preload ◄── port ◄── engine host ◄───────┘  Host::dispatch_async → command thread
UI ◄── window.unfold.stream(...) messages ◄────────────────── sink {"kind":"stream",...}
UI ◄── window.unfold.on(type, ...) ◄───────────────────────── sink {"kind":"broadcast",...} / {"kind":"event",...}
```

- A command is `{"command": "<name>", "args": {…}}`. Arguments are camelCase and strict: an
  unknown field is an `invalid_args` error.
- **Every command about one project takes `project` (a `ProjectId`) first.** A window attaches to
  nothing: it shows the whole workspace ([U-39](decisions-and-tradeoffs.md#u-39-one-window-shows-the-workspace-commands-name-their-project)),
  and Rust knows it only by the `windowId` its commands carry. An unknown id fails with
  `unknown_project`.
- Each command runs on its own Rust thread. Long commands check their request's cancellation;
  `cancel(requestId)` stops them.
- A result is JSON (`null` for commands that return nothing). A failure rejects with
  `{"code", "message"}` ([codes](#error-codes)). A panic in Rust becomes `internal`; the process
  keeps running.
- Streamed commands (`compile`, `search_text`) send messages before they resolve:
  `{"kind":"stream","windowId":1,"requestId":7,"message":{…}}` (`SinkMessage`). The final result
  is the command's resolution. The addon sends sink messages and settles promises through one
  ordered channel, so a request settles only after every message sent before it (its stream
  messages, and the events its command sent) reached the sink.
- **Events go to every window** as `{"kind":"broadcast","event":{…}}`, which the engine host's
  router posts to every window port it has. Only `board_conflict` is targeted, to the window holding
  the board: `{"kind":"event","windows":[w],"event":{…}}`. Project events name their `project`.
- **Liveness.** Each project command either `wakes` its project (opens its engine and watcher, and
  extends its lease) or is served by a resting project from disk (`rests`); a few concern no project
  at all (`—`). Listing never wakes a project; using a result (any command that takes a key or a path
  of a project) does. The rules are in [architecture §11.2](architecture.md#112-live-and-resting-projects).
- **Trusted commands.** The engine host adds `"trusted": true` to a command **the main process**
  sends, and never to one a window sends: the router rebuilds every window command from `command`
  and `args`, and refuses a window message that names a trusted-only command or carries `trusted`
  anywhere, with `untrusted`, before it reaches Rust. Trusted-only commands are the ones that take an
  absolute path (from a dialog, a drop, or the command line) or manage window lifecycle:
  `add_project`, `export_mermaid`, `saved_windows`, `save_window_bounds`, and `forget_window`
  (`MAIN_PROCESS_ONLY` in `desktop/src/engine-host/router.ts`). The main process sends its own
  requests over the utility process's parent port (`{type:"request", id, windowId, command, args,
  trusted}`, answered `{type:"result", id, ok, …}`; `desktop/src/shared/transport.ts`). Its MCP
  server's requests for agents go with `trusted: false` and window `0`, and the router treats them as
  a window's: a trusted-only command is refused with `untrusted`. A streamed one (`search_text`) sent
  with `stream: true` gets each stream message as `{type:"stream", id, message}` before its result.
- **Paths, per project.** Every path a window names is relative to the folder of the project the
  command names, `/`-separated, and confined there: `..`, absolute paths, backslashes, and symlinks
  that leave the folder are rejected with `invalid_path`
  ([architecture §13](architecture.md#13-security-and-privacy)). Board ids are paths too. A window
  can't reach another project's files through a project id it passes: each path is confined to the
  project its command names.
- **Payloads stay small** (U-25): one card, one scope level, one page of lines, one page of
  callers; lists are capped and filtered in Rust ([limits](#payload-limits)).

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

`unfold_workspace::Host` is what the napi-rs addon (`crates/unfold-node`) wraps. It is thread-safe;
the `_async` entry points run on a new Rust thread and call `done` there, so the addon resolves its
promise through a thread-safe function and never blocks the JavaScript thread.

| Addon | Host | Notes |
| --- | --- | --- |
| `init(options)` | `init(&str)` | `{"appDataDir","logsDir","fallbackCacheDir"}`; once. Loads `workspace.json`; every project starts resting. Starts the liveness reaper |
| `subscribe(sink)` | `subscribe(Fn(String))` | Every event and stream message, one JSON string each, on the channel that also settles promises (in order) |
| `dispatch(windowId, requestId, command)` | `dispatch_async(window, request, json, done)` | `done(Ok(json))` or `done(Err(CommandError))`; the rejection message is `error.to_json()`. Any window id is accepted |
| `cancel(requestId)` | `cancel(request)` | No-op when finished |
| `detachWindow(windowId)` | `detach_window_async(window, done)` | A window closed: waits for its [closing writes](#closing-writes), then releases its board claims and sessions and its Home selection. Projects that lose interest start their lease |
| `shutdown()` | `shutdown_async(done)` | The app is quitting: closes every live project, writing each one's last known state to `workspace.json`. Called once |

Blocking variants (`dispatch`, `detach_window`, `shutdown`) exist for tests and tools. The addon and
the engine host don't change when a command is added.

### Closing writes

A window's last saves (its boards, its remembered state) and its detach travel on different channels
(the window's port and the main process's lifecycle message), so a detach can arrive while a save it
should follow is still running. The **closing writes** are `save_board`, `repair_board`,
`publish_board`, `discard_changes`, `unpublish_board`, `delete_board`, `duplicate_board`,
`set_board_folder`, `create_folder`, `rename_folder`, `delete_folder`, `save_board_view`,
`save_window_state`, and `save_window_bounds` (`CLOSING_WRITES` in `crates/unfold-workspace/src/host.rs`
and in the router, which a test keeps equal).

- The router never cancels a closing write when its window detaches; it cancels every other request
  of the window, then calls `detachWindow`. Detaches of one window run in order.
- Rust counts each window's closing writes from the moment `dispatch` is called, and
  `detach_window` waits until the window has none in flight (at most `CLOSE_WAIT`, 15 s), so those
  saves land while the window still holds its boards.
- On quit, the main process sends `shutdown` to the engine host, which detaches every window (so
  their closing writes land) and then calls `shutdown`.

## Commands

Columns: **L** is liveness (`wakes`, `rests`, or `—`). Commands marked **trusted** run only from the
main process.

### Workspace

| Command | L | Does |
| --- | --- | --- |
| `workspace` | — | Every project row (id, shared name, path, expanded, live, `ProjectState`, and the branch from Git's files), the default selection for new windows, recent boards (≤ 20), and the folders "Opened before" (`removed`, ≤ 5). Reads each repository's `HEAD` on every call. |
| `add_project` | — | **Trusted.** Adds an absolute folder as a project, appended, expanded, and resting, and writes nothing in it. A folder that already is a project, or another worktree of a project's repository, answers `status: "exists"` with that row ([U-41](decisions-and-tradeoffs.md#u-41-branches-and-worktrees-from-gits-files)). `not_a_folder`, `too_many_projects` (64). |
| `readd_project` | — | Adds back a folder "Opened before" lists; any other path is `not_found`. Otherwise as `add_project`, which is how a window can add a folder without naming arbitrary paths. |
| `remove_project` | — | Removes a project from the workspace: closes it if live, drops its recent boards, keeps its path in "Opened before", and never touches its `.unfold/`. `boards_open` while any window holds one of its boards. Sends `project_reset` and `workspace_changed`. |
| `reorder_projects` | — | A new order of the rows; must be a permutation of the ids (`invalid_args`). |
| `set_project_expanded` | — | Expands or collapses a row in the sidebar (remembered in `workspace.json`). |
| `select_project` | wakes | The calling window's Home selection (or `null`): it keeps the project live, and becomes `workspace.json`'s default for new windows. Returns after the project is open, so Home's next query finds it live. |
| `git_info` | rests | The project's branch (or detached commit) and its repository's worktrees (≤ 64): path, branch, main, current, available. `null` outside a repository. |
| `switch_worktree` | — | Points the project row at another available worktree folder from `git_info` (`not_a_worktree` otherwise; `boards_open` while a window holds one of its boards). The graph, published boards, drafts, and `state.json` become the new folder's. Sends `project_reset`, `workspace_changed`, and `project_state`. |
| `drafts` | rests | One fan-out over every project: its drafts, and published boards with unpublished changes, newest first (≤ 500). |

There is no command to rename a project: a project has one name, its shared name, which is a project
setting (`update_project_config` with `set_project_name`).

### Project, Home, and settings

| Command | L | Does |
| --- | --- | --- |
| `project_summary` | wakes | Home's header and Health line: the `ProjectState` and the graph status (fallback location, watching by scanning, background updates), when the graph was last committed, files, languages, applications, entrances, the branch and worktrees, and a health summary: graph and settings problems, files that don't parse, the settings error, unreached entrances, launches into code that isn't indexed, and the boards that need review (most items first, ≤ 20). |
| `compile_preview` | wakes | What the not-compiled Home shows: files that would be graphed by language, files that wouldn't, skipped folders with their reasons, where the first Compile would put `local/` (and why, with the fallback location: a network volume, a link, or a folder this process can't write, asked of the operating system), whether the settings file exists or is invalid, and discovery problems. Walks the folder; writes nothing. |
| `scope_dir` | wakes | One level of the scope tree (`path`, `""` for the root): its folders only (≤ 2,000, by name), each with why it is skipped (if it is) and how many source files discovery finds under it, ignoring `[scope]`. |
| `entrances` | rests | How the project starts, for Home and Add's map: applications named after their startup, with their launches (≤ 5) and their first `perApplication` entrances (default 8, 1–500) and total; then the entrances no launch reaches, and launches into code that isn't indexed (≤ 50). `application` (a startup key) returns that application only; `filter` keeps entrances whose title, trigger, label, or path contains it, ignoring case. HTTP routes sort by their first path segment (`group`), path, and method. `not_compiled` before the first Compile. |
| `problems` | wakes | The Problems list ([U-28](decisions-and-tradeoffs.md#u-28-problems-lists-only-real-problems)): files that don't parse or can't be read, settings errors, discovery and input problems, a failed compile, and the health of **every** board of the project (≤ 5,000). |
| `index_log` | wakes | The last updates' statistics, oldest first, and the graph's counts. |
| `compile` | wakes | `mode: "check"` (Compile; the default) or `"from_scratch"` (Rebuild from scratch). Streams `CompileProgress` (`stage`, `done`, `total`) and resolves with `CompileOutcome`; keeps the project live until it settles. The first Compile creates `.unfold/`. Cancel with `cancel(requestId)` or `cancel_compile`; it then rejects with `cancelled` and the previous graph stays. |
| `cancel_compile` | — | Cancels the project's running Compile or Rebuild and every queued one (any window's); returns `{cancelled}`. A Compile or Rebuild can be cancelled until its store commit finishes: a cancel during the commit rolls the transaction back and nothing changes. Background updates are not affected ([compiling.md](compiling.md#cancellation-and-threads)). A no-op on a resting project. |
| `set_background_updates` | wakes | Turns background updates on or off for the project (remembered in its `state.json`); returns the graph status. |
| `project_config` | rests | `.unfold/config.toml`: whether it exists, its text, the validated settings (languages, scope, disabled detectors, Python projects, manual entrances, and `evidence`: the `[evidence] files` patterns as written), or the error with line and column; and the project's shared name, with whether it is the default. |
| `update_project_config` | wakes | Applies `SettingsEdit`s (tagged `op`) in order, all or nothing, keeping comments and formatting: exclude from graph, include in graph, graph only this folder, remove one scope pattern as written, add/replace/remove a manual entrance, set/remove a Python project, turn a detector on or off, set or remove the project's name (`set_project_name`, `null` for the default), and add or remove an evidence file pattern (`add_evidence_pattern` appends one unless it is listed; `remove_evidence_pattern` removes one exactly as written, `settings_conflict` when it isn't listed, and drops an emptied key and table). An edit that would make the file invalid fails with `invalid_settings`; a broader rule in the way fails with `settings_conflict`. Sends `config_changed`; a name change also updates `workspace.json` and sends `workspace_changed`. The next background update applies the rest. |
| `get_preferences` / `set_preferences` | — | App preferences: theme, editor command, background-update default for newly compiled projects, thread limit (applies to projects opened afterwards), key bindings (`keyBindings`: action id → shortcut, `""` for none; see [ui-shell.md](ui-shell.md#custom-key-bindings)), the update check (`updates.enabled`), and whether agents may connect to the main process's MCP server and on which port (`agents: {enabled, port}`, off and 29180 by default; a port below 1024 is `invalid_args`). `set_preferences` takes the whole object and, once it is saved, sends `preferences_changed`. |
| `app_info` | — | The username that signs edits (`person:<username>`) and the engine version. |
| `diagnostics` | — | Copy Diagnostics: a redacted summary for bug reports: engine build and version, producers, OS and architecture, and, for `project` when given, its graph state and completeness, files by language and state, the last update's statistics, problems by kind, background updates, watch mode, the fallback location, the lock state, and settings counts (a resting project reports its last known state only). No source, no file contents, and no paths; the folder path, home folder, and username are replaced with placeholders should any slip in. The UI adds the app, Electron, and Chromium versions. |

### Files and graph

| Command | L | Does |
| --- | --- | --- |
| `file_view` | wakes | The peek sheet's render model of a whole file, or with `range` one page of lines. `graph` says how the text relates to the graph: `current` (colors, outline, folds, links), `not_compiled` (the file changed since the graph; current text with colors, no links), `evidence` (an evidence file whose bytes are the ones the graph recorded: text with line numbers; a changed one is `not_compiled`), or `not_indexed`. Offsets in a page are UTF-16 offsets from the start of the **file**; `textStart` is where the page's `text` begins. Files that aren't UTF-8 text or are larger than 8 MiB come back `unshowable`. |
| `open_in_editor` | wakes | Runs the preferences' editor command (for example `code -g {path}:{line}`; `{path}`, `{line}`, `{column}` are replaced, and the path is appended when the template doesn't name it) **without a shell**; with no editor set, opens the file as text in the system's text editor (`open -t` on macOS, Notepad on Windows; elsewhere the desktop's handler, refused with `no_editor` for executable files and `.desktop` launchers), never with its default app, which could run it. Folders are refused. Returns which. |
| `reveal_in_finder` | wakes | Shows a file or folder (`""` for the project's folder) in the system's file manager. |
| `card` | wakes | A block key gives a **block card**: the header (kind, name, qualified name, path, lines, language, file state, source freshness, owner), the Code model (highlighted, folds per the card policy, call links), and the Flow model ("Paths from written structure"). `lens: "code"` or `"flow"` builds only one. Flow nodes carry their fallback names; the UI applies a page's authored labels itself, so one card query serves every page. An `entrance:…` key gives an **entrance card**: its header, its evidence, and its targets in registration order with names and notes. |
| `peek` | wakes | The hover over a call link: a block's first written line (its signature, when the source verifies) and location, or an entrance's title and location. |
| `relations` | wakes | For a block: one page of **callers** (`callersOffset`/`callersLimit`, default 100, at most 500), **outgoing** edges grouped as on the page (`onPage`: the keys of this project's cards on the page, ≤ 2,000), openable, and external or unresolved (with their module or reason in words), and **reached from** (the non-process entrances whose closures contain it). |
| `callers` | wakes | Show callers: the distinct definitions that call a block, by path and line, each with its call sites (`limit` default 20, at most 100), and the total. |
| `preview` | wakes | Add's preview pane and the map popover. A block: its header, its first lines as a Code model (≤ 5), reached from (≤ 5), calls in source order (≤ 8), and called by (≤ 5), each with a total. An entrance: its header, handlers (≤ 8), where it is declared, how many definitions it reaches, and the external modules it calls most (≤ 5). |
| `board_refs` | rests | The board pages of **every** project where a key is a card ("Already on …"), ≤ 50. |

Commands that read the graph (`card`, `relations`, `entrances`, …) fail with `not_compiled` before the
first Compile; `file_view` and `compile_preview` work on any project.

### Typed lines

What a line on a board says about the call it draws ([reader](reader.md#typed-lines)). An arrow is
addressed by its occurrence and edge, `(source, seq)`: a Code link is the edge `(source, 0)`; a Flow
chip carries its `seq`. Each command reads the project's committed graph in one store snapshot.

| Command | L | Does |
| --- | --- | --- |
| `line_labels` | wakes | The at-rest labels of a page's arrows in one call: `lines` (≤ 500, else `invalid_args`) of `{source, seq}` → `[{source, seq, label}]` in the order asked. A label (`LineLabel`) has `inputs` (a call: one per argument, the type of the parameter it fills, else its name, else nothing, with a Python keyword argument's `keyword`; a passed reference: one per parameter of the passed callable after the ones the passed value binds itself), `inputsUnknown` (true when a passed function's bound parameters aren't proven: `inputs` is then empty and the label shows `(…)`), and an `output` (the written return type, or a constructed class's name). `label` is `null` for an external or unresolved edge, an import, or an unknown occurrence or edge. |
| `call_contract` | wakes | The contract of one resolved call or construction (`seq` 0): `sends` (each argument with its written text and the parameter it fills or why it fills none, then the parameters no argument filled), `returns` (where the value lands, `awaited`, the return type), the callee's direct `throws`, `caught`, `calleeLines`, and `callerLine`. `not_found` for an unknown occurrence, or an edge that isn't a resolved call or construction. |
| `type_definition` | wakes | A type declaration or class (`key`): `kind` (`interface`, `type_alias`, `enum`, `class`), name, path, line, language, its written `members`, the members it `inherited` (one group per base it extends, depth first in written order, each type once, at most 8 levels down: `from`, the base's name as a `TypeText` whose one span carries its key, module, or reason, and the base's `members`, none for a base without a declaration), a non-object type alias's `aliased` type, and `uses` (how many type links resolve to it, heritage clauses included). `not_found` when the key isn't a type declaration or class. |
| `type_uses` | wakes | One page of the places a type declaration or class is written (`offset`, default 0; `limit`, default 50, at most 200, else `invalid_args`): `total` and `places`, each with its `owner` (the definition the annotation belongs to), the owner's `label`, `path`, `line`, and, for a heritage clause, `heritage` (`extends` or `implements`), by path and position. `not_found` as for `type_definition`. |

Written types travel as `TypeText`: the text and one span per type name in it, in UTF-16 offsets
relative to the text, with a `style` (`declared` with its `target`, `external` with its `module`,
`builtin`, or `unresolved` with a `note`). `graph_updated` of a project invalidates that project's
`line_labels`, `call_contract`, `type_definition`, and `type_uses` results, and `project_reset`
removes them with everything else cached for it.

### Search

One request is one fan-out in Rust over the projects in `projects` ("Look in"; every project by default,
in workspace order). Searching never wakes a project: a resting project answers from its stored graph,
and its files are the indexed files that graph lists.

| Command | L | Does |
| --- | --- | --- |
| `search` | rests | Definitions (functions, methods, classes, and type declarations: interfaces, type aliases, enums), entrances, and files by name. Definitions and entrances are FTS5 candidates (every word of the query is a substring of the name, qualified name, or path) ranked by `nucleo`'s fuzzy score (the name counts most, then the qualified name, then the path); files are a live project's files under discovery's ignore rules, fuzzy-matched on their path. Projects are searched in parallel and merged by score, then workspace order, then shorter name, then key. `kinds` narrows it (`["files"]` serves ⌘P and `["definitions"]` ⌘T), and `definitionKinds` narrows definitions to block kinds (`["interface", "type_alias", "enum", "class"]` finds types; an unknown kind is `invalid_args`); `limit` is per kind across all projects (default 50, at most 500). A hit's `kind` is a definition's block kind or an entrance's kind. Each group reports its total (capped at 2,000 per kind per project), and the result has per-project totals. An entrance hit names its application; a file hit, its definition count. Match offsets for highlighting are UTF-16. |
| `search_text` | rests | Text or regex (`regex`, `caseSensitive`, `wholeWord`) across the same file lists, never `.unfold/`, skipping binary files and files over 16 MiB. Streams one `TextSearchFile` per file, projects in workspace order and files **in path order** within each (each line with its number, text, and UTF-16 match ranges; a very long line is a window around its first match), stops after 2,000 matches in all (`truncated`), and is cancellable. The summary has per-project counts. |

### Boards

A board is addressed by `{project, id}`. Its **id** is its project-relative file path:
`.unfold/boards/<name>.json` (published, with or without unpublished changes) or
`.unfold/local/drafts/<name>.json` (a draft; the id stays the same when `local/` is in the fallback
location). The id changes on a draft's first Publish and on Move back to drafts; those commands return
the new id, and Rust moves the claim, the view state, and recent-board entries itself. Board documents
travel as `BoardDocument` (the [`unfold-board`](reader.md#format) file shape); a window sends edits as
`BoardEdit` (`{title, pages}`: Rust owns `folder`, `projects`, and the file framing). The files, the
unpublished-changes copy, and Publish's conflict rule are in [the reader](reader.md#files-drafts-publish-and-unpublished-changes).

A board is open in **one window at a time** ([U-37](decisions-and-tradeoffs.md#u-37-several-windows-one-window-per-board)).
`open_board` and `new_board` claim it for the calling window, `move_board` transfers the claim,
`close_board` and `delete_board` release it, and `detachWindow` releases all of a window's claims.
Every other command on one board checks that the calling window holds it and fails with `board_held`
(another window has it) or `board_not_held`, except `read_board`, which reads any board without
claiming it. Folder operations and `set_board_folder` work on any board, held or not: the folder is
Rust-owned metadata, never part of what a window saves.

| Command | L | Does |
| --- | --- | --- |
| `list_boards` | rests | A project's boards and its empty folders: for each board its place, name, title, folder, page and card counts, the other projects its cards come from, its unpublished changes (`changes`, 0 for none), when its working file changed, the window holding it, a health `label` only when something is wrong (`1 missing · 2 outdated`; `null` for a resting project, whose health needs a current graph), and `error` when the file can't be read (≤ 2,000). |
| `board_thumbnails` | rests | What Home's thumbnails draw for up to 60 boards: the first page's cards (≤ 200: position, kind (`entrance`, `type` for a type declaration, `class`, or `block`), view, folded, line count, the rows a type card lists (`fields`: its members, or an alias's aliased type), and whether the card is from another project), the graph links between same-project cards, and the drawn links. The UI turns cards into rectangles with the canvas's own sizing rules for the view each shows. |
| `read_board` | rests | A board as its working file holds it (the draft, the changes copy, or the published file), with the file's revision, its publish state as `open_board` reports it (`null` for a draft), and the window holding it (`heldBy`, or `null`). It claims nothing and doesn't wake the project: it only reads files. `invalid_args` for an id that isn't a board id, `not_found` when the file doesn't exist, `board_invalid` when it can't be read. |
| `open_board` | wakes | Claims the board for the calling window and returns it (`status: "opened"`) with its revision, publish state (`published`: the published revision, `changes`, and `baseChanged`), health, page outlines, and view state; records a recent board. If another window holds it, returns `status: "held"` with `heldBy`, and the UI asks the main process to focus that window. |
| `new_board` | wakes | Creates an empty draft with one page (`page-1`; "Untitled board", "Page 1" unless named), optionally in a folder, claims it, and returns it; records a recent board. `not_compiled` before the first Compile (drafts live in `local/`). |
| `reload_board` | wakes | Reads the board's working file again (after `board_conflict`) and returns it. |
| `close_board` | — | Releases the claim. Save first: unsaved edits live in the window. |
| `move_board` | — | Hands the claim to another window (Open in new window). |
| `save_board` | wakes | Validates the edit's shape and the references between each page's items (`board_invalid`), then against the graph of each project its items name (no new missing references, broken groups, held references, or broken links, including what resource cards and connections cite), signs new descriptions, labels, and link labels, and the claims (resource cards, connections that cite code, frame descriptions) from verified source, fills in a connection's channel from an entrance at its `to` end, grounds new or changed claims ([reader: grounding](reader.md#grounding); a refusal is `board_problem` saying what failed and what to do), carries frames' and notes' other fields through as sent, writes the working file atomically, and returns the board as saved. Drafts autosave here; a published board's first edit creates its unpublished-changes copy (its id doesn't change), and an edit that makes it equal to the published version deletes the copy. An item that names the board's own project (a card's `ref.project`, a resource's or anchor's `project`, a link's `carriesProject`) is `invalid_args`. Refused with `board_conflict` when the working file changed on disk since the window loaded it, unless `force: true`. |
| `save_board_view` | wakes | Remembers the board's active page and each page's camera and lens in the project's `state.json`. |
| `board_health` | wakes | The board's health per page (missing, outdated, broken groups, to review, not connected, broken links; each item names its page and target — a card, group, label, link, resource card, or frame — with the key and, for an anchor, its position `anchor` involved, and its repairs) and each page's outline (cards in follow order; Flow nodes still showing code names). |
| `repair_board` | wakes | Applies one repair a health item offers, on one page, and saves: `reconnect` (`from` → `to`, a key, page-wide: cards, group members, labels, resource cards' types and anchors, connections' carried types and anchors, and the steps under a reconnected card), `reconnect_anchor` (`item`, `anchor` position, `to`: `{key, project?}` or `{file, lines, project?}`), `remove_card`, `remove_label`, `ungroup`, `remove_link`, `remove_resource`, `remove_description`, `resign` (a description's or label's text, or a resource card or connection with its texts; `target` is `{card}`, `{group}`, `{label}`, `{link}`, `{resource}`, or `{frame}`), and `accept`. |
| `publish_board` | wakes | A draft: writes `.unfold/boards/<name>.json` (`name` defaults to the slug of the title; only a draft's first Publish takes one) without ever overwriting, removes the draft, and returns the board under its new id; a taken name answers `status: "name_taken"` with the existing `path` and a free `suggestion`. A published board with unpublished changes: writes them over the published file and deletes the copy, unless the published file changed since the copy started (a pull) or is gone: then `status: "conflict"` with `theirsRevision` (`null` when deleted), and nothing is written until `resolve: "keep_mine"`. A published board without changes: nothing to do. |
| `discard_changes` | wakes | Deletes a published board's unpublished-changes copy and returns the published version (this is also Publish's "take theirs"). `invalid_args` for a draft. |
| `unpublish_board` | wakes | Move back to drafts: the working version becomes a new draft; the published file and its copy are removed; new id. |
| `delete_board` | wakes | Deletes the board's files (a published board's copy too; the UI asks first) and releases the claim. |
| `duplicate_board` | wakes | Copies the working version into a new draft, "<title> copy", in the same folder; not claimed. |
| `set_board_folder` | wakes | Moves any board into a folder (`null` for the top level). For a published board it is an unpublished change. |
| `create_folder` | wakes | Adds an empty folder (kept in `state.json` until a board goes into it); a no-op if it exists. `invalid_folder` for a bad path. |
| `rename_folder` | wakes | Renames a folder: rewrites the `folder` of every board at or under it (published ones as unpublished changes) and the empty folders; returns how many boards moved. `folder_exists` when the target exists; `not_found` when the source doesn't. |
| `delete_folder` | wakes | Deletes an empty folder and its empty subfolders; `folder_not_empty` while a board is in or under it. |

Board health is computed for every board of a project and cached per board (by the file's revision and
the graph generations of the projects its cards resolve to); it is recomputed after `graph_updated` of
the project or of any project its boards' cards come from, and it feeds `problems` and
`project_summary`. Board files the app writes are announced with `boards_changed`.

**Outside edits.** When the watcher sees an open board's working file change (an agent or a teammate
wrote it, a pull changed it, or it was deleted), the window holding it gets `board_conflict` with
which file changed, the revision it last loaded or saved, and the revision on disk. A window without
unsaved edits calls `reload_board`; one with unsaved edits asks which to keep: `reload_board` takes
theirs, `save_board` with `force: true` keeps its own.

### Export

| Command | L | Does |
| --- | --- | --- |
| `export_mermaid` | wakes | **Trusted.** Writes one page of a board the named window holds (`board: {id, page}`, its last saved version) or an entrance with its handlers (`entrance`) as Mermaid to the absolute `path` the person chose in the main process's save dialog. |
| `export_mermaid_text` | wakes | The same text, for the clipboard; no path. |

### Windows

Windows are remembered in `windows.json` by a stable `key` the main process assigns (window ids change
every run). A window learns its key from the window command `window_info`.

| Command | L | Does |
| --- | --- | --- |
| `window_state` | — | What the window with `key` remembered: its Home selection, tabs, active tab, layout, and bounds. |
| `save_window_state` | — | Remembers the calling window's Home selection, board tabs (≤ 100, each with its page), active tab, and layout (UI-owned JSON, ≤ 64 KiB); keeps the saved bounds. |
| `saved_windows` | — | **Trusted.** Every remembered window (≤ 16), for restoring them at launch. |
| `save_window_bounds` | — | **Trusted.** A window's position, size, and whether it is maximized. |
| `forget_window` | — | **Trusted.** Forgets a window the person closed, so it doesn't come back. |

## Window commands and shell events

The main process handles the window and dialog commands (`window.unfold.window(command, args)`,
`desktop/src/main/ipc.ts`); how they reach the engine is in [desktop/README.md](../desktop/README.md#window-commands-that-involve-the-engine).

| Window command | Does |
| --- | --- |
| `new_window` | Opens another window on the workspace (it shows the workspace's selected project) |
| `focus_window` | Brings a window to the front; with `board`, that window also gets `focus_board` and shows its tab |
| `close_window` | Closes the calling window |
| `add_project` | The folder picker, then the trusted `add_project`; `null` when cancelled |
| `confirm_remove_project` | The native confirmation ("Remove <name> from the workspace? Its .unfold folder stays."); the UI then calls `remove_project` |
| `export_mermaid` | The save dialog, then the trusted `export_mermaid` for the calling window; `null` when cancelled |
| `export_png` | `{project, board, page, title?}`: the save dialog, then the render window draws the page (unsaved edits included) at scale 2 and the main process writes the PNG; `{path, bytes}`, or `null` when cancelled |
| `move_tab_to_new_window` | Opens a window for a tab and resolves with its id at once; the UI then calls `move_board` and `transfer_board_state` |
| `transfer_board_state` | Hands a board's window-side state (its unsaved version and undo history) to another window, which gets `board_state_transferred` |
| `copy_to_clipboard` | Writes text to the clipboard (windows have no clipboard permission) |
| `window_info` | The calling window's id, stable key, and platform |
| `report_view` | The window's view for agents: `{board?, page?, selection, lens?, visible, tabs}`; the main process keeps the last one per window |
| `agent_edit_result` | `{requestId, result}`: the answer to an `agent_edit` (`{ok: true, page, board, items, warnings}` or `{ok: false, code, message}`) |
| `agent_read_result` | `{requestId, result}`: the answer to an `agent_read` (`{ok: true, page, board}` or a refusal) |
| `agent_open_board_result` | `{requestId, result}`: the answer to an `agent_open_board` (`{ok: true}`, or a refusal with `heldBy`) |
| `agent_create_board_result` | `{requestId, result}`: the answer to an `agent_create_board` (`{ok: true, board, info, folder, pages}`: the draft this window now holds, or a refusal) |
| `agent_edit_board_result` | `{requestId, result}`: the answer to an `agent_edit_board` (`{ok: true, board, pages, results}`: the board as saved, its pages in order, and what each operation did, or a refusal) |
| `agent_show_result` | `{requestId, result}`: the answer to an `agent_show` (`{ok: true, page}`, or a refusal) |
| `render_ready` | `{requestId, result}`: the render window's answer to `render_page` (`{ok: true, bounds?, checks?}`) or `render_frame` (`{ok: true}`), or a refusal; only the render window may send it, and it may send nothing else |
| `agent_server_status` | The MCP server's state for Preferences › Agents: `{state, port, address, message?, agents}` (`off`, `starting`, `listening`, `port_taken`, or `failed`; each connected agent with when it was last seen and last acted) |

`window.unfold.addDroppedFolder(file)` adds a folder dropped on the window as a project. Shell events
arrive through `window.unfold.on` like engine events: `menu_command` (an application-menu item the
window runs as an action), `board_state_transferred`, `focus_board`, `project_added` (select it),
`project_add_failed` (say why), and the agent channel's `agent_edit`, `agent_read`,
`agent_open_board`, `agent_create_board`, `agent_edit_board`, `agent_show`, `agent_finish`, and
`agent_server_changed` (the MCP server's state, sent to every
window when it changes) (`desktop/src/shared/agent-api.ts`; how they travel is in
[desktop/README.md](../desktop/README.md#the-agent-bridge), what the window does with them in
[the UI shell](ui-shell.md#agents-in-the-window)). The render window gets `render_page` (a page to draw, for an
image or the page checks) and `render_frame` (the camera to capture it with) (`desktop/src/shared/render-api.ts`;
[desktop/README.md](../desktop/README.md#the-render-window), [the UI shell](ui-shell.md#the-render-window)).

## Events

| Event | When | Payload |
| --- | --- | --- |
| `workspace_changed` | A project row was added, removed, reordered, expanded or collapsed, renamed (its shared name), or switched to another worktree; recent boards changed | `workspace`: the whole `WorkspaceView` |
| `project_state` | A row's `ProjectState` changed, live ↔ resting included (not on every progress tick) | `project`, `state` |
| `project_reset` | The project's folder changed (worktree switch) or the project was removed: everything cached for it is stale | `project` |
| `compile_progress` | A compile or update advanced (throttled) | `project`, `stage`, `done`, `total` |
| `graph_updated` | A commit changed the graph | `project`, changed and removed files, changed block keys, changed entrance keys, `inputsChanged`, `everything` (lists too long, or a first or full commit: refresh all), `complete` |
| `problems_changed` | The Problems list changed | `project`, `count` (board health included) |
| `files_changed` | The watcher saw files appear, change, or disappear | `project`, `paths`, `truncated` |
| `boards_changed` | Board files, folders, or a board's publish state changed, outside the app or by a command | `project`, `boards` (a copy's change is reported under its published id), `folders` |
| `board_conflict` | An open board's working file changed on disk outside the app (only to the holding window) | `project`, `board`, `file` (`draft`, `changes`, or `published`), `savedRevision`, `diskRevision` (`null` when deleted) |
| `config_changed` | `.unfold/config.toml` changed | `project` |
| `preferences_changed` | `set_preferences` saved the app preferences (the engine host also hands it to the main process, which starts or stops the MCP server) | `preferences`: the whole saved `Preferences` |

`ProjectState` is one indicator per project row: `current`, `compiling {done, total}`, `updating
{files}`, `problems {count}`, `paused {changed}`, `not_compiled`, `failed {message}`, `resting {last,
atMs}`, or `unavailable {reason}`. A live project's state is derived from its graph status, first
match wins: failed, compiling, updating, paused, not compiled, problems, current. The problem count is
graph and settings problems only; boards that need review never make a row amber (they show in Home's
Health line). How each window's caches follow these events is in [the UI shell](ui-shell.md#data-from-the-engine).

## Error codes

| Code | Meaning |
| --- | --- |
| `invalid_command` | The command JSON is malformed or names no command |
| `invalid_args` | The arguments don't fit the command |
| `invalid_path` | A path leaves the project's folder or isn't relative to it |
| `not_found` | The key, path, board, board folder, or removed folder doesn't exist |
| `not_compiled` | The project has no graph yet |
| `unknown_project` | No project with that id is in the workspace |
| `boards_open` | A window holds a board of the project (remove, switch worktree) |
| `not_a_worktree` | `switch_worktree` to a path that isn't an available worktree of the project |
| `too_many_projects` | The workspace has 64 projects |
| `untrusted` | The command is accepted only from the main process |
| `board_held` / `board_not_held` | Another window holds the board / the calling window doesn't |
| `board_invalid` | A board file or document breaks the `unfold-board` format |
| `board_problem` | A save would introduce a missing reference, broken group, held reference, or broken link, a new or changed claim isn't grounded in the code it cites, or text can't be signed (source changed: compile first) |
| `board_conflict` | The board's working file changed on disk since it was loaded |
| `name_taken` | A published board's file name is taken |
| `invalid_folder` | A board folder path breaks the folder rules ([reader](reader.md#pages-cards-and-folders)) |
| `folder_exists` / `folder_not_empty` | A board folder rename's target exists / a deleted board folder has boards |
| `invalid_settings` / `settings_conflict` | A settings edit would make the file invalid / needs a broader rule changed |
| `no_editor` | `open_in_editor` has no editor command, and opening the file with the system could run it |
| `cancelled` | The request was cancelled |
| `compile_failed`, `store`, `reader`, `io`, `not_a_folder`, `open_elsewhere`, `closed` | From the project's workspace (`WorkspaceError::code`); `not_a_folder` and `open_elsewhere` also come from waking a project, which then shows as `unavailable` |
| `not_initialized`, `already_initialized` | `init` wasn't called, or was called twice |
| `internal` | A bug: a panic or a broken invariant |

The transport adds its own codes (`TransportErrorCode` in `desktop/src/shared/unfold-api.ts`):
`engine_unavailable` (the addon couldn't be loaded), `disconnected` (the engine host stopped while the
request was in flight), `cancelled`, `invalid_message`, `invalid_command` (a window command the main
process doesn't know), `untrusted`, and `internal`.

## Payload limits

| What | Limit |
| --- | --- |
| Projects in a workspace; recent boards; "Opened before" | 64; 20; 5 |
| Windows remembered; board tabs per window; a window's `layout` | 16; 100; 64 KiB |
| Worktrees per repository | 64 |
| `entrances` per application; launches per application; unindexed launches | 1–500 (default 8); 5; 50 |
| `scope_dir` folders | 2,000 |
| `problems` | 5,000 |
| `relations` callers per page; `onPage` keys; `callers` | 500 (default 100); 2,000; 100 (default 20) |
| `preview` | 5 code lines; 5 reached from, 8 calls, 5 callers; 8 handlers, 5 modules |
| `line_labels` lines; `type_uses` places per page | 500; 200 (default 50) |
| Written text in typed lines (types, argument text, aliased types) | 120 UTF-16 units |
| `search` hits per kind; totals per kind per project | 500 (default 50); 2,000 |
| `search_text` matches | 2,000 in all |
| `file_view`; `search_text` per file | 8 MiB; 16 MiB |
| `list_boards`; `drafts`; `board_thumbnails` boards and cards; `board_refs` | 2,000; 500; 60 and 200; 50 |
| Board views in `state.json`; pages per board view | 500; 200 |
| A board file | 16 MiB |

## Adding a command

1. Write the handler in the right `crates/unfold-workspace/src/commands/` module:
   `pub fn my_command(ctx: &Ctx<'_>, args: MyArgs) -> CommandResult<MyResult>`. Take `project:
   ProjectId` first if it concerns a project. Get the project with `ctx.project(&args.project)?`
   (never wakes it: use `.readers()` for the stored graph, so the command `rests`) or its engine with
   `ctx.live(&args.project)?` (wakes it). Confine every path with the handle's or the workspace's
   `confine(path)?`, check `ctx.check_cancelled()?` in long loops, stream with `ctx.stream(&message)`,
   and call `ctx.require_trusted(…)?` if it takes an absolute path or manages windows (then add it to
   `MAIN_PROCESS_ONLY` in the router too).
2. Give the argument and result types `#[derive(Deserialize, TS)]` / `#[derive(Serialize, TS)]`
   with `#[serde(rename_all = "camelCase")]` (and `deny_unknown_fields` on arguments). Optional
   arguments are `Option<T>` with `#[serde(default)]` and `#[ts(optional)]`. Document every type
   and field: the docs become the generated JSDoc. Don't send whole-graph payloads; page them.
3. Add one line to `command_table!` in `src/ipc/table.rs`:
   `/// What it does.` then `"my_command" => MyCommand(MyArgs) -> MyResult = module::my_command;`
   (add `, stream MyMessage` for a streamed command). If it writes boards or window state, add it to
   `CLOSING_WRITES` in `src/host.rs` and in the router.
4. Run `cargo xtask gen-ipc`, then `pnpm --filter ui typecheck`. CI runs
   `cargo xtask gen-ipc --check`, which fails when the generated file is stale.
5. Test it through `Host` in `crates/unfold-workspace/tests/commands/`, add it to this catalogue,
   and, if events should refresh its results in the UI, to the invalidation rules in
   `ui/src/ipc/queries.ts`.
