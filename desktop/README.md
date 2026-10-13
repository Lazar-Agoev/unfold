# Desktop shell

Electron main owns windows, menus, dialogs, updates, and opt-in agent MCP. The engine host
utility process owns the Node addon/Rust host; the renderer has no filesystem or Node access.
See [architecture](../docs/architecture.md#processes) and [IPC](../docs/ipc.md).

## Code map

| Area | Entry point |
| --- | --- |
| Startup/windows/OS integration | `src/main/index.ts`, `windows.ts`, `menu.ts`, `dialogs.ts` |
| Hardening/protocol | `src/main/security.ts`, `protocol.ts`, `app-url.ts` |
| Engine supervision and routing | `src/main/engine-host.ts`, `src/engine-host/router.ts`, `binding.ts` |
| Renderer API | `src/preload/index.ts`, `src/shared/unfold-api.ts`, `transport.ts` |
| Agent bridge/rendering | `src/main/agents/`, `src/shared/{agent,render}-api.ts` |
| MCP | `src/main/mcp/`; tools, teaching assets, listener and server |
| Restore/location/updates | `src/main/workspace.ts`, `window-placement.ts`, `app-paths.ts`, `updater.ts`, `update-policy.ts` |
| Build/package | `electron.vite.config.ts`, `electron-builder.yml` |

## Transport and windows

Preload exposes only typed `window.unfold` operations. Engine requests use the window's
MessagePort; main-origin requests use the utility-process parent port. The router filters
untrusted requests, routes streams/events/results, and coordinates detach/closing writes.
The host adapter must keep sink delivery and promise completion ordered. Ordinary command
additions change Rust types/table, not each transport hop.

Main restarts crashed engine hosts and reconnects windows. Keep renderer board edits and
sessions available during reconnect. Do not block main with graph work.

Stable window keys and remembered bounds/tabs support restore. Choose usable placement for
current displays and avoid silently losing unpublished board work. The holding-window rule
and closing-write wait are owned by Rust host plus renderer sessions, not a second main-process
board store. See [board lifecycle](../docs/reader.md#files-drafts-publish-and-unpublished-changes).

## Agents

MCP is opt-in, local, and lives in main. The listener checks local/HTTP request constraints;
its tests own exact refusals. Rust reads use untrusted command routing. Tool schemas,
identity, formatting, results, prompts, and guides live under `src/main/mcp/`.

`AgentBridge` opens/locates the holding window and sends typed requests to its renderer.
It preserves unsaved state, optimistic page revisions, single-step undo, and author marks.
A delivered edit may outlive its response timeout; re-read after `unconfirmed` before retrying.
Requests/answers from the wrong window or revision cannot substitute for the holder's result.
See [agent IPC](../docs/ipc.md#agents) and [UI agent edits](../docs/ui-shell.md#agents-in-the-window).

`unfold://` links are parsed by the server to locate project/board/page/item. They are not
registered as an OS handler. Product guides/prompts teach agents using Unfold; edit them only
when that product behavior changes, not as repository onboarding.

## The render window

`PageRenderer` queues screenshot/page-check requests through a hidden offscreen hardened
window using `render.html` and `src/shared/render-api.ts`. It has an engine connection for
card data but never opens or claims a board. Wait for fonts and measured content before
capture; bound canvas size, PNG bytes, deadlines, and queued work. Close idle/unresponsive
renderers. UI handling is in [render window](../docs/ui-shell.md#the-render-window).

## Packaging

`electron-builder.yml` configures the unpacked macOS app, which `just bundle-dir` creates. The
release addon, built renderer/shell, assets/fonts, and third-party notices must be included.

Hardening includes sandbox/context isolation, no renderer Node integration, protocol/CSP
checks, denied navigation/permissions, and package-time Electron fuses.

Signing is enabled only with explicit certificate configuration. Updater traffic is optional.

## Running and checks

`just dev` builds the debug addon and starts renderer/shell development servers. `just js-build`
builds UI/shell. Desktop tests use fake Electron for focused rules; typecheck with
`pnpm --filter @unfold/desktop typecheck` and test with `pnpm --filter @unfold/desktop test <file>`.
Use [check selection](../docs/testing.md#choose-checks-for-a-local-change) for actual process,
restore, agent, packaging, or screenshot changes. Do not build/package the whole app for shell docs.
