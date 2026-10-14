# Commands and events

Read this for the transport, engine commands, events, trust, the main process's window commands, the MCP server agents
connect to, or adding a command. Rust owns engine command names and types in
[ipc/table.rs](../crates/unfold-workspace/src/ipc/table.rs) and
[commands/](../crates/unfold-workspace/src/commands/mod.rs).
[cargo xtask gen-ipc](../tools/xtask/src/gen_ipc.rs) generates
[the TypeScript mirror](../ui/src/ipc/generated/index.ts); never edit that file by hand. Shell, agent, and render
types live in [desktop/src/shared/](../desktop/src/shared/unfold-api.ts).

## How a command travels

```text
UI call → window.unfold → preload → MessagePort → engine host → addon.dispatch → Rust Host → handler
UI      ← result or {code, message}, stream messages, events ← one ordered channel ← Rust
Main    → engine host (trusted requests of its own; agents' requests untrusted) → the same Host
```

[ui commands.ts](../ui/src/ipc/commands.ts) wraps typed requests; [queries.ts](../ui/src/ipc/queries.ts) caches results
under `["engine", command, args]` and holds the event-driven invalidation. Transport shapes and routing live in
[transport.ts](../desktop/src/shared/transport.ts) and [router.ts](../desktop/src/engine-host/router.ts).

- A request is `{command, args}`. Arguments are camelCase and strict: an unknown field is `invalid_args`.
- Every command about one project names its `ProjectId` first.
- Results are JSON (`null` for none). Failures are `{code, message}` with a stable snake_case code
  ([ipc/error.rs](../crates/unfold-workspace/src/ipc/error.rs) lists them). A caught panic becomes `internal`.
- Handlers run on Rust command threads and never wait for a compile, except `compile` itself and an
  `update_project_config` that changed only `[[app]]` tables (it waits for the entrances to resolve again).
- Long work checks cancellation; `cancel(requestId)` stops a request. `compile` streams progress messages.
- Stream messages, events, and settlements share one ordered channel, so a result never overtakes its own stream
  messages or the events its command sent.

## Liveness and paths

Commands served from stored data leave a project resting; commands that need its engine wake it. `workspace`,
`list_boards`, `read_board`, and `search` never wake a project. Read the handler: `ctx.project` doesn't wake, `ctx.live` does. See
[project lifecycle](engine.md#project-lifecycle).

Every path a window names is project-relative and `/`-separated, and goes through
[confine.rs](../crates/unfold-workspace/src/confine.rs): absolute paths, `..`, backslashes, NUL, and symlinks that
leave the folder are rejected. A project id is not permission to reach another project's files.

## Trusted commands

Only the main process's own requests are trusted. `MAIN_PROCESS_ONLY` in the router names them: `add_project` (an
absolute path from the folder picker), `window_state`, and `save_window_state`. A window message naming one, or
carrying `trusted`, is refused with `untrusted` before it reaches Rust, and the handlers check trust again
(`ctx.require_trusted`). The MCP server's requests go untrusted, exactly as a window's do. A new trusted command
updates both the router and its handler.

## The addon's surface

[unfold-node](../crates/unfold-node/src/lib.rs) wraps one `unfold_workspace::Host` and holds no graph logic:

| Function | Does |
| --- | --- |
| `init(options)` | Takes `{appDataDir, logsDir}`, loads `workspace.json` with every project resting |
| `subscribe(sink)` | Receives every event and stream message, one JSON string each |
| `dispatch(windowId, requestId, command)` | Runs one command; resolves with its JSON result |
| `cancel(requestId)` | Cancels a request |
| `detachWindow(windowId)` | Waits for the window's closing writes, then closes its boards |
| `shutdown()` | After every window detached: closes every live project, remembering its state in `workspace.json` |

Promise-returning functions start on a Rust thread and settle through a thread-safe callback; the JavaScript thread
never waits on Rust. New commands need no new addon function. `Host` also has blocking `dispatch` and `detach_window`
for tests and tools.

### Closing writes

A window's last saves and its detach travel on different channels, so `CLOSING_WRITES` (in
[host.rs](../crates/unfold-workspace/src/host.rs) and the router, kept equal by a test) names the commands a detach
waits for: `save_board`, `repair_board`, `delete_board`, `set_board_folder`, `create_folder`, `rename_folder`,
`delete_folder`, and `save_window_state`. Detach cancels the window's other requests, waits (up to 15 s) for those,
then closes the window's boards so their projects can rest. A command that finishes after its window closed can't
open a board or select a project for it (`window_closed`). Add a new closing write to both lists.

## Engine commands

Search the table with `rg -n '"<command>"' crates/unfold-workspace/src/ipc/table.rs`, then read the handler's types and
tests. Exact arguments, results, defaults, and caps live in code.

| Area | Commands |
| --- | --- |
| Workspace | `workspace` (project rows), `add_project` (main only; compiles at once; refuses a folder Unfold can't write `.unfold/` into), `remove_project`, `reorder_projects`, `set_project_expanded`, `select_project` |
| Project and Home | `project_summary` (state line and Health), `entrances` (the `[[app]]` tables as written, each `code` with its state and fix; one app by name or key), `problems`, `compile` (streams progress), `cancel_compile` |
| Settings | `project_config`, `update_project_config` (scope, Python projects, evidence, and `[[app]]` edits; returns the diff), `get_settings`, `set_settings`, `app_info` (the username that signs edits) |
| Code and cards | `code_view` ([the code panel's model](reader.md#the-code-panels-model)), `open_in_editor` (the editor command, else TextEdit; creates an empty `unfold.toml` when asked for a missing one), `card` (a definition's [drawn flow](reader.md#the-drawn-flow) with the page's overlay, open paths, and Show all calls; or an app's or entrance's card), `code_text` and `flow_text` ([agent text](reader.md#agent-text)) |
| Relations and contracts | `relations` (callers, outgoing calls, Reached from), `callers` (Show callers), `board_refs`, `call_contract` (by the call's block key), `type_definition`, `type_uses` |
| Search | `search`: definitions, apps and entrances, and files by name, across projects, from the store's name index |
| Boards | `list_boards`, `read_board` (without opening), `open_board`, `new_board`, `reload_board`, `close_board`, `save_board`, `board_health`, `repair_board`, `delete_board`, `set_board_folder`, `create_folder`, `rename_folder`, `delete_folder` |
| Window | `window_state`, `save_window_state` (main only) |

### Boards

A board is addressed as `{project, id}`, its id being its file's path under `.unfold/boards/`. `open_board` and
`new_board` open it in the calling window, and the project keeps a session for it while the app runs. Every other
board command except `read_board` and the folder commands needs the board open (`board_not_open`). `save_board`
checks, signs, and writes the window's version over the file; an outside change sends `board_reloaded`. Folders are
one name (`invalid_folder` for a `/`). The rules are in [reader › Boards](reader.md#boards).

## Events

Read the payload comments in [events.rs](../crates/unfold-workspace/src/events.rs). Events carry a snake_case `type`
and camelCase fields; every project event names its project.

| Event | When |
| --- | --- |
| `workspace_changed` | Project rows were added, removed, reordered, expanded, or collapsed. The UI replaces its `workspace` data |
| `project_state` | A project's state changed (live and resting included) |
| `project_reset` | A project was removed: everything cached for it is stale |
| `compile_progress` | A compile or update advanced (throttled) |
| `graph_updated` | A commit changed the graph: changed and removed files, changed keys, or `everything` past 2,000 |
| `entrances_changed` | The apps and entrances changed. An edit of only `[[app]]` tables sends this alone, without a compile |
| `problems_changed` | The Problems list changed |
| `files_changed` | The watcher saw files appear, change, or disappear |
| `boards_changed` | Board files or folders changed |
| `board_reloaded` | An open board's file changed outside the app and its session reloaded; only the window that opened it gets it |
| `config_changed` | `unfold.toml` changed |

Invalidation touches only the named project's queries (and a cross-project `search` that includes it). Only
`graph_updated` with `everything`, and the engine-restart reset, refetch a project wholesale. New event-dependent data
updates the invalidation in `queries.ts` alongside the handler.

## Window commands and shell events

The main process answers window and dialog commands ([unfold-api.ts](../desktop/src/shared/unfold-api.ts), handled in
[main/ipc.ts](../desktop/src/main/ipc.ts), exposed by [preload](../desktop/src/preload/index.ts)):

| Command | Does |
| --- | --- |
| `add_project` | The folder picker, then the trusted engine `add_project` |
| `confirm_remove_project` | Asks before Remove from workspace |
| `export_png` | The save dialog, then the render window draws the page (unsaved edits included) at twice the size |
| `copy_to_clipboard` | Windows have no clipboard permission, so the main process writes it |
| `window_info` | The window's id, the board to show, and the Show all calls switch |
| `report_shown_board`, `report_show_all_calls` | Saved with the window's bounds in `window.json` |
| `report_view`, `agent_*_result`, `agent_server_status` | The agent channel ([below](#agents)) |
| `render_ready` | The render window's only command |

Shell events: `menu_command` (Edit › Undo and Redo, sent only when focus is outside a text field), `project_added`,
`project_add_failed`, the agent requests, `agent_server_changed`, and `render_page`/`render_frame`. Keep them apart
from engine events.

App data holds `settings.json` (`editorCommand` only), `window.json` (bounds, maximized, the last board shown, and
Show all calls), and `workspace.json` (the projects). Nothing else is remembered.

## Agents

The MCP server runs in the main process ([desktop/src/main/mcp/](../desktop/src/main/mcp/server.ts)) whenever the app
is open. See [architecture](architecture.md) for where it sits among the processes.

- **This machine only.** It listens on `127.0.0.1:29180`, path `/mcp`. A request with an `Origin` header (a web page)
  or a `Host` other than `127.0.0.1:29180` or `localhost:29180` (DNS rebinding) is refused with 403; a body over
  1 MiB with 413. There is no fallback port: when another program holds it, Settings says so (`port_taken`).
- **Stateless Streamable HTTP**, tools and instructions only: no prompts, resources, or sampling.
- **Identity.** Each request names its client; Claude Code signs as `agent:claude`, Codex as `agent:codex`. Settings
  lists the agents seen since the server started.
- **Results are text** under about 6,000 tokens, paged with a `next` cursor. A refusal says what failed and what to
  call instead.

| Group | Tools |
| --- | --- |
| Set up | `list_projects`, `get_project`, `add_project` (after the person confirms in a dialog), `compile_project`, `update_project_settings`, `set_app`, `set_entrance`, `remove_entrance` |
| Read | `get_guide`, `search`, `list_entrances`, `read_code`, `read_call`, `get_relations`, `read_file`, `list_boards`, `read_page`, `get_current_view` |
| Place | `create_board`, `edit_pages`, `organize_boards`, `add_cards`, `set_view`, `arrange_page`, `remove_items`, `show` |
| Shape a card | `shape_card` (title, description, names, combined steps, and open calls in one undo step), `propose` |
| Compose | `frame_cards`, `add_resources`, `connect`, `add_notes`, `describe` |
| Review | `check`, `get_screenshot`, `repair`, `finish_work` |

Graph reads are untrusted engine commands: `read_code` prints `code_text`, `read_page` prints `flow_text`, and labels
map to keys by lookup in those answers ([agent text](reader.md#agent-text)). The server never parses keys.

**Agents edit through the same document operations as people.** Board writes go through the agent bridge
([agents/bridge.ts](../desktop/src/main/agents/bridge.ts)) to the window, which applies them with the edits people's
actions use, as one undo step named after the agent, saved at once through the normal board save and its checks
([agents in the window](ui-shell.md#agents-in-the-window)). Nothing else writes a board document.

- An edit of a board the window doesn't hold loads its session in the background first; the person's view doesn't
  move. `show` is for presenting.
- Page revisions: an edit that passes `expectRevision` is refused (`revision_mismatch`) when the page changed since.
- Requests time out after 30 s. A write that times out answers `unconfirmed`, because it may still land: re-read
  before retrying. A window that closes fails its requests with `window_closed`.
- Links are `unfold://<project folder name>/<board id>?page=<page>&item=<item>`. Only the main process parses them;
  they carry no code and open nothing.
- A hidden render window draws pages for `get_screenshot`, `check`, and Export PNG without holding a board session
  ([render window](ui-shell.md#the-render-window)).

**Rules the server enforces:**

- Compiled facts aren't the agent's: a connection never repeats a compiled call, and a `calls` connection starts at an
  unknown call.
- Cite or don't draw: an agent's resource cards and connections need citations whose verified text names them
  ([grounding](reader.md#grounding)).
- Unknown calls stay `?`; a name never resolves a call.
- People's words are theirs: replacing or removing a person's text needs `replacePeopleText`, and agents re-sign or
  accept only what agents wrote.
- Agents can't edit code, delete boards, remove projects, or change Settings.

While an agent works, the page shows "Claude is working…" and outlines what it changed; `finish_work` clears the marks
and shows its summary as a toast.

## Payload limits

Return one card, one page, or one bounded slice; never a whole graph. Handlers own the caps (overlay sizes, open
paths, callers, search hits, board size). Validate untrusted lengths before expensive reads, and keep total caps on
streams too.

## Adding a command

1. Write the handler and its types in the owning `commands` module. Take `ProjectId` for project work; use
   `ctx.project` for stored data and `ctx.live` when the engine is needed. Confine paths, check cancellation, and
   require trust where needed.
2. Derive serde and `ts_rs::TS`. Arguments are camelCase with `deny_unknown_fields`; optional arguments use serde
   defaults and `ts(optional)`. Doc comments become the generated JSDoc. Bound result sizes.
3. Add one line to the command table. Add it to `MAIN_PROCESS_ONLY` or `CLOSING_WRITES` in both Rust and the router
   when it is one.
4. Run `cargo xtask gen-ipc`, then `cargo xtask gen-ipc --check` and `pnpm --filter @unfold/ui typecheck`.
5. Test through `Host` in `crates/unfold-workspace/tests/commands/`, and update query invalidation. Pick further
   checks with [testing](testing.md).
