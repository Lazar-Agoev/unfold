# Commands and events

Read this for transport, engine commands, events, trust boundaries, or adding a command.
Rust owns engine command names and types in
[ipc/table.rs](../crates/unfold-workspace/src/ipc/table.rs) and
[commands/](../crates/unfold-workspace/src/commands/mod.rs).
[cargo xtask gen-ipc](../tools/xtask/src/gen_ipc.rs) generates
[the TypeScript mirror](../ui/src/ipc/generated/index.ts); never edit that file manually.
Shell/agent/render types live in `desktop/src/shared/`.

## How a command travels

```text
UI call → window.unfold → preload → MessagePort → engine host → addon.dispatch → Rust Host
UI      ← result/error, stream messages, and events ← ordered sink channel ← Rust
Main    → utility-process parent port → same engine host, with explicit trust handling
```

[ui commands.ts](../ui/src/ipc/commands.ts) wraps typed requests; queries/invalidation live in
[queries.ts](../ui/src/ipc/queries.ts). Transport shapes and routing live in
[desktop transport.ts](../desktop/src/shared/transport.ts) and
[router.ts](../desktop/src/engine-host/router.ts).

Requests are `{command, args}` with strict camelCase arguments. Unknown fields produce
`invalid_args`. A project command names its `ProjectId`; a window shows the whole workspace
and is identified separately. Results are JSON (`null` for void); failures carry `{code, message}`.
A caught Rust panic becomes `internal` rather than an unexplained renderer hang.

Commands run on Rust threads. Long work checks cancellation; `cancel(requestId)` stops its
request. Streamed commands emit messages before settling. Sink events/messages and promise
settlement share ordered delivery, so completion cannot overtake earlier stream messages.

Broadcast events reach all windows and name the project when scoped. Targeted board conflicts
reach the holding window. Query invalidation must match the event's project and affected data;
do not refresh the whole workspace for every local update.

## Liveness and paths

Commands served from stored data keep a project resting; commands needing its engine wake it.
Listing does not wake projects. Key/path use normally wakes; `read_board` only reads the file.
Read the handler to determine liveness rather than adding an unnecessary `ctx.live` call.
See [project lifecycle](engine.md#project-lifecycle).

Every renderer path is project-relative, `/`-separated, and confined through workspace helpers,
including symlink checks. Reject absolute paths, traversal, backslashes, and escaping symlinks.
A project ID is not permission to reach another project's files.

## Trusted commands

Only main-origin requests get trusted status. The router reconstructs renderer requests from
allowed fields, rejects caller-provided trust anywhere, and denies main-only names before
Rust dispatch. The authoritative set is `MAIN_PROCESS_ONLY` in the router, including project
addition by absolute path, OS-path export, and remembered-window lifecycle operations.

Handlers that require trust must enforce it too. Adding one requires updating both its Rust
check and router classification. MCP requests are untrusted; a local agent is not a substitute
for a user's file dialog or permission to invoke privileged shell commands.

## The addon's surface

The Node addon wraps `unfold_workspace::Host` with init, subscribe, dispatch, cancel,
detach-window, and shutdown. Asynchronous host entry points complete through thread-safe
callbacks; never block the JavaScript thread with long native work. The adapter/router need
no new method for each ordinary command.

Initialization receives app-data/log/cache locations. Subscribe receives the ordered JSON
sink. Dispatch carries window/request identity; detach and shutdown release sessions only
after their closing writes. Blocking variants exist for tests/tools.

### Closing writes

The window port and main lifecycle channel can race. `CLOSING_WRITES` in Rust host and the
router identifies board/window-state writes that must finish before detach; a test keeps
those sets equal. Detach cancels ordinary requests, waits for closing writes, then releases
claims. Quit shuts down after detaching windows. Include a new closing write in both sets.

## Finding a command

Use `rg -n '"<command>"' crates/unfold-workspace/src/ipc/table.rs`, then its handler's types,
comments, and tests. Search one generated TypeScript type when consuming it; do not read the
whole generated module for onboarding.

| Concern | Handler area / representative names |
| --- | --- |
| Projects/Home/settings | `workspace`, `projects`, `settings`, `diagnostics`, `deploy_import` modules |
| Entrance catalog | `entrances` (overview by category, or one scope's rows with a cursor), `launches` (`check_launch`, `launch_suggestions`) |
| Files/graph/relations | `files`, `file_view`, `graph`, `relations` modules |
| Typed lines | `types` module: `line_labels`, `call_contract`, `type_definition`, `type_uses` |
| Search | `search`, `text_search`; streamed text results |
| Boards | `commands/boards/`: claims, reads/writes, drafts, folders, Publish, health/repair |
| Exports | `export`; text exports versus main-only absolute output paths |
| Window state | `windows`; saved bounds/views and detach-sensitive writes |

Exact arguments, results, defaults, command names, and liveness are maintained in code.
The table generates `Commands`, command/stream lookup types, runtime name lists, events,
errors, and sink shapes for UI consumers.

## Window commands and shell events

The main process handles menus, dialogs, opening/revealing files, windows, updater actions,
agent answers, and render-window readiness. Its types are
[unfold-api.ts](../desktop/src/shared/unfold-api.ts), with handlers in
[main/ipc.ts](../desktop/src/main/ipc.ts) and preload exposure in
[preload/index.ts](../desktop/src/preload/index.ts).

A dropped folder uses the paths recorded by preload's actual drop handling; do not accept
an arbitrary renderer-provided OS path. Shell events communicate engine restarts, menu
commands, project additions, preferences, and agent/render requests through their typed
contracts. Keep these distinct from Rust workspace events.

## Agents

The opt-in MCP server runs in main and routes graph reads through untrusted commands.
Board mutations go through `AgentBridge` to the holding renderer, preserving its unsaved
state, validation, undo history, and agent attribution. Do not bypass the bridge with a
second writer of the board document.

Page revisions support optimistic checks; refuse stale `expectRevision` before applying an
edit. The holding window applies an operation batch as one undo step and saves through the
normal board path. Requests can time out after a mutation was delivered (`unconfirmed`);
re-read before retrying rather than assuming no edit occurred. Wrong-window/stale answers
are ignored. Grounding and authorship rules are in [reader](reader.md#grounding).

`unfold://` text links identify shared project, board, page, and item for MCP resolution;
they are not an OS protocol handler that executes arbitrary link actions. A hidden render
window supplies screenshots/page checks without holding a board. Protocol types and error
handling live in `desktop/src/shared/agent-api.ts` and `render-api.ts`; implementation and
teaching files live beside `desktop/src/main/mcp/` and `main/agents/`.

## Events and errors

Workspace events invalidate relevant queries or update transient UI state. Read the event
union and payload comments in [events.rs](../crates/unfold-workspace/src/events.rs).
For new event-dependent data, update UI invalidation alongside the handler.

Use established structured error codes, not message parsing. Command errors distinguish
invalid arguments/paths/settings, missing or resting projects, unavailable/changed source,
board conflicts/grounding, cancellation, trust, and internal failures. The command error
module, workspace conversion, and MCP error formatter own exact codes and translations.

## Payload limits

Return one card, bounded page, scope level, or related slice; never a whole graph by default.
Handlers own numeric caps and defaults for lists, callers, labels, searches, files, boards,
and saved views. Respect UTF-16 text caps and byte-size limits. Validate untrusted lengths
before expensive reads or allocations; streams do not remove the need for total-result caps.

## Adding a command

1. Add the handler and types in the owning command module. Use `ProjectId` for project work,
   `ctx.project` plus read pools for resting data or `ctx.live` when engine use is required.
   Confine paths, check cancellation, and require trust where needed.
2. Derive serde/`TS` for argument/result types. Arguments use camelCase and
   `deny_unknown_fields`; optional args use serde defaults and `ts(optional)`. Document
   fields because their comments become generated TypeScript JSDoc. Bound result sizes.
3. Add the command-table entry, and stream type if applicable. Add privileged/closing-write
   classifications to both Rust and router when required.
4. Run `cargo xtask gen-ipc`, then `cargo xtask gen-ipc --check` and
   `pnpm --filter @unfold/ui typecheck`.
5. Test through `Host` in `crates/unfold-workspace/tests/commands/`; update affected query
   invalidation. Add focused runtime verification only under [check selection](testing.md#choose-checks-for-a-local-change).
