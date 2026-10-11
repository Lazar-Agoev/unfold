# Architecture

Unfold is a desktop app for reading TypeScript and Python as a graph. A workspace connects
repositories as projects; each project has its own graph. Boards contain pages of code,
flow, type, and resource cards, explanations, and connections across projects.

This is the system overview. For a change, use the [task map](README.md) and read only the
owning reference section. Current implementation details live beside their code.

## Processes

```text
React renderer → preload / MessagePort → Electron engine host → Node addon → Rust Host
       ↑                                      ↓                         ↓
       └──────────── results, streams, events ───────────────────────────┘
Electron main → windows, menus, dialogs, updates, MCP server and agent bridge
```

The engine host is an Electron utility process, isolating native engine crashes from the
main process. It owns one Rust host for the workspace. Main supervises and restarts it;
windows reconnect. Rust commands run off the JavaScript thread. The renderer has no Node
or direct filesystem access. See [IPC](ipc.md#how-a-command-travels).

## Ownership

| Area | Owns | Reference |
| --- | --- | --- |
| `unfold-core` | Records, keys, ranges, vocabularies, validation, canonical export | [Graph model](graph-model.md) |
| `unfold-engine` | Language contracts, discovery, settings, update pipeline, shared resolution | [Engine](engine.md) |
| `unfold-lang-typescript`, `unfold-lang-python` | Parsing, extraction, binding, language evidence and detectors | [TypeScript](languages/typescript.md), [Python](languages/python.md) |
| `unfold-store` | SQLite cache, atomic delta commits, read pools, FTS, facts cache | [Store entry point](../crates/unfold-store/src/lib.rs) |
| `unfold-reader` | Derived views, verified Code/Flow/types, boards, health, Mermaid | [Reader](reader.md) |
| `unfold-workspace` | Project lifecycle, engine threads, watchers, commands, events, saved state | [Engine lifecycle](engine.md#project-lifecycle), [IPC](ipc.md) |
| `unfold-node` | Node-API adapter over the Rust host | [Addon source](../crates/unfold-node/src/lib.rs) |
| `desktop/` | Electron processes, transport, operating-system integration, agent bridge | [Desktop](../desktop/README.md) |
| `ui/` | React state, interactions, layout, themes, rendered cards | [UI shell](ui-shell.md) |

Core depends on no other Unfold crate. Language crates depend on core and engine, never
storage. Engine produces graphs; workspace commits them through store. Reader never parses
source and receives highlighting through a trait. Only the addon depends on Node. The UI
uses typed `window.unfold` calls; graph computation stays in Rust, geometry in TypeScript.

## Guarantees

- **Precision:** follow a call or type name only with proof of exactly one permitted target.
  Preserve external and unresolved results and their reasons; do not infer runtime dispatch.
- **Source fidelity:** record written structure and types. Verify bytes against the graph's
  hash before displaying or signing them. Keep last-good facts visibly distinct from disk.
- **Stable identity:** use structural keys outside a file; local IDs only within its graph.
- **Determinism:** equal effective inputs produce equal canonical output, independent of
  thread order. An incremental update must agree with a fresh compile.
- **No execution:** never execute analyzed code, package scripts, Git, dependency installers,
  or Python/Node tools while compiling a project. Parser dependencies run inside the engine.
- **Confined writes:** write project state only under `.unfold/`, or the documented cache
  fallback; application state lives in operating-system app-data folders.

The detailed rules belong to [graph model](graph-model.md), [engine](engine.md), and
[reader](reader.md). New features must preserve them.

## Storage

| Location | Role |
| --- | --- |
| `.unfold/config.toml` | Shared project name, scope, detectors, Python roots, evidence, manual entrances |
| `.unfold/boards/` | Published boards, shared through the repository |
| `.unfold/local/` | Ignored local state, drafts, unpublished edits, graph and facts cache |
| OS app data | Connected projects, window restore, preferences, logs |
| OS cache fallback | Local cache/state when the project folder cannot safely hold them |

Shared settings and published boards are versioned contracts. SQLite is a private cache:
[the executable schema](../crates/unfold-store/src/schema.rs) owns its shapes and version.
Incompatible or damaged caches are recreated, without migrations. One engine thread owns
the writer; read pools use committed snapshots. A prepared update is committed before the
engine adopts it, so failed writes keep engine, store, and reader on the previous graph.

## Security

All renderer paths are relative to the named project and confined there, including symlink
checks. Commands accepting operating-system paths or controlling window lifecycle are
main-process-only. The router does not trust caller-supplied flags. Engine payloads are
bounded and long requests support cancellation.

Windows use context isolation and sandboxing, without Node integration. Navigation,
new windows, permissions, protocol responses, CSP, and packaging fuses are controlled in
[security.ts](../desktop/src/main/security.ts), [protocol.ts](../desktop/src/main/protocol.ts),
and [electron-builder.yml](../desktop/electron-builder.yml). The updater is the app's
network feature; agent MCP is local and opt-in. See [desktop agent handling](../desktop/README.md#agents).

## Validation and releases

Choose focused checks with [testing](testing.md#choose-checks-for-a-local-change). Routine
proposals and docs do not require app builds. E2E is conditional on integrated behavior;
packaged smoke tests and budget measurements cover their respective boundaries.

Builds are configured for macOS, Windows, and Linux. Local verification has been recorded
on Apple-silicon macOS; configuration alone does not establish verification elsewhere.
Signing, notarization, publisher identity, and update-feed configuration are release work.
See [desktop packaging](../desktop/README.md#packaging) and the workflow files in `.github/workflows/`.
