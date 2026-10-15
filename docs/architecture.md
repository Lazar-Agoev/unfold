# Architecture

Unfold is a macOS desktop app that compiles TypeScript and Python into connected diagrams. A workspace holds
projects; each project is one folder with its own graph, its `unfold.toml`, and its boards. Boards hold pages of
cards (modules, definitions, types, apps, and entrances), with names, combined steps, descriptions, proposals,
frames, resources, and connections added by people and agents.

This is the system overview. For a change, use the [task map](README.md) and read only the owning reference.
Implementation details live beside their code.

## Processes

```text
React renderer → preload / MessagePort → Electron engine host → Node addon → Rust Host
       ↑                                      ↓                         ↓
       └──────────── results, streams, events ───────────────────────────┘
Electron main → the window, menus, dialogs, the render window, the MCP server and agent bridge
```

There is one window: a sidebar of projects, folders, boards, and pages; Home or a board beside it; and the code
panel. The engine host is an Electron utility process, so a crash in native engine code never takes down the main
process; it owns one Rust Host for the workspace, and main restarts it and re-attaches the window. Rust commands run
off the JavaScript thread. The renderer has no Node or filesystem access. A hidden render window draws page images
and page checks for agents and Export PNG. See [IPC](ipc.md#how-a-command-travels).

The MCP server runs in the main process whenever the app is open, on `127.0.0.1:29180` only, and refuses requests
from web pages. Its tools use the same engine commands a window uses.

## Ownership

| Area | Owns | Reference |
| --- | --- | --- |
| `unfold-core` | Nodes, links, typed records, apps and entrances, vocabularies, keys, the ownership rule, validation, the canonical export, text and paths | [Graph model](graph-model.md) |
| `unfold-engine` | The language traits, discovery and scope, settings and their edits, the update pipeline, resolution and pairing, resolving entrance `code` | [Engine](engine.md) |
| `unfold-lang-typescript`, `unfold-lang-python` | Parsing, lowering to nodes, binding, linking, highlighting | [TypeScript](languages/typescript.md), [Python](languages/python.md) |
| `unfold-store` | The SQLite cache: nodes, links, typed records, apps and entrances, the definitions catalog, name search, the facts cache | [Store entry point](../crates/unfold-store/src/lib.rs) |
| `unfold-reader` | Owners, the catalog, relations, the drawing function, agent text, the code panel's model, contracts, boards and their health | [Reader](reader.md) |
| `unfold-workspace` | Projects and their lifecycle, engine threads, the watcher, commands, events, app settings, window state | [Engine › Project lifecycle](engine.md#project-lifecycle), [IPC](ipc.md) |
| `unfold-node` | The Node-API adapter over the Rust Host | [Addon source](../crates/unfold-node/src/lib.rs) |
| `desktop/` | Electron processes, transport, operating-system integration, the MCP server | [Desktop](../desktop/README.md) |
| `ui/` | React state, interactions, layout, rendered cards | [UI shell](ui-shell.md) |

Core depends on no other Unfold crate. Language crates depend on core and engine, never on storage. The engine
produces graphs; the workspace commits them through the store. **Only core's key module reads keys**: everything
else treats a key as opaque text and gets kinds, parents, and files from records. The reader never parses source;
it receives highlighting through core's `Highlighter` trait. Only the addon depends on Node. The UI calls typed
`window.unfold` commands; graph computation, including what a drawn flow allows, stays in Rust, and the UI computes
only geometry.

## Guarantees

- **Proof:** a call or type name resolves to a project definition only with proof of exactly one permitted target.
  Library, builtin, and unknown outcomes keep their labels and reasons; runtime dispatch is never guessed.
- **Written facts only:** the compiler records written structure and written types, never inferred ones.
- **Verified source:** bytes are checked against the graph's hash before they are shown or signed. Last-good facts
  are kept for files that stop parsing, and stay visibly distinct from disk.
- **Stable identity:** structural keys identify blocks outside a file; local IDs only inside its graph.
- **Determinism:** the same inputs give the same canonical output, independent of thread order. An incremental
  update agrees with a fresh compile.
- **No execution:** analyzed code, package scripts, Git, dependency installers, and Python or Node tools never run
  while compiling. Parsers run inside the engine.
- **Confined writes:** project writes stay under `.unfold/`, plus `unfold.toml` when a person or agent edits
  settings; app state lives in the app-data folder.
- **Local only:** the MCP server answers only this machine, and payloads are bounded.
- **Ranges:** engine ranges are UTF-8 bytes; rendered ranges are UTF-16.

The detailed rules are in [Graph model](graph-model.md), [Engine](engine.md), and [Reader](reader.md). New features
must keep them.

## Storage

| Location | Holds | Shared through git |
| --- | --- | --- |
| `unfold.toml` | `languages`, `[scope]`, `[[python.project]]`, `[evidence]`, `[[app]]` ([Engine › Shared config](engine.md#shared-config)) | Yes |
| `.unfold/boards/<id>.json` | Boards, saved as they are edited | Yes |
| `.unfold/local/` | `cache/` (the graph database and the folder lock), `state.json` (empty board folders), and a `.gitignore` of `*` | No |
| App data (`~/Library/Application Support/Unfold`) | `workspace.json` (the projects), `settings.json` (the editor command), `window.json` (bounds, the last board shown, Show all calls) | No |

`unfold.toml` and the [board file](schemas/unfold-board.schema.json) are the shared contracts. SQLite is a private
cache: [the executable schema](../crates/unfold-store/src/schema.rs) owns its shapes and version, and a database
from another version, or a damaged one, is rebuilt without migrations. One engine thread owns the writer; read pools
use committed snapshots. A prepared update is committed before the engine adopts it, so a failed write leaves engine,
store, and reader on the previous graph. Remembered state files that are missing or invalid read as defaults.

## Security

Every renderer path is relative to the named project and confined there, including symlink checks. Commands that
take an operating-system path, set a program Unfold runs (the editor command), or keep the window's state are
main-process only: the engine host marks them trusted, never a window's or the MCP server's requests. Engine payloads
are bounded, and long requests can be cancelled.

The window and the render window use context isolation and sandboxing, without Node integration. They can't
navigate, open windows, or get any permission, and nothing loads from the network; see
[security.ts](../desktop/src/main/security.ts), [protocol.ts](../desktop/src/main/protocol.ts), and
[electron-builder.yml](../desktop/electron-builder.yml). The MCP listener refuses any request with an `Origin` header
or a `Host` other than `127.0.0.1` or `localhost` at its port, and bodies over its limit. There is no update check or
other network feature. See [Desktop](../desktop/README.md).

## Validation and builds

Choose focused checks with [Testing](testing.md). Docs changes need no app build; end-to-end journeys run only for
integrated behavior.

Unfold builds for macOS only: `just dev` and an unpacked macOS build. See [Desktop](../desktop/README.md) and the
pull-request workflow in `.github/workflows/`.
