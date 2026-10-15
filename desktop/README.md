# Desktop shell

The Electron main process owns the one window, the menu, native dialogs, the MCP server, and the render window. An
Electron utility process, the engine host, loads the Rust addon. The renderer has no filesystem or Node access. See
[architecture](../docs/architecture.md) and [IPC](../docs/ipc.md).

## Code map

| Area | Files (`src/`) |
| --- | --- |
| Lifecycle, the window, menu, dialogs | `main/index.ts`, `windows.ts`, `menu.ts`, `dialogs.ts` |
| Window commands (the only IPC main answers) | `main/ipc.ts` |
| Window state and adding projects | `main/workspace.ts`, `window-placement.ts`, `app-paths.ts` |
| Hardening, the `app://` origin | `main/security.ts`, `protocol.ts`, `app-url.ts` |
| Engine host supervision and routing | `main/engine-host.ts`; `engine-host/index.ts`, `router.ts`, `binding.ts` |
| The renderer's API | `preload/index.ts`; `shared/unfold-api.ts`, `transport.ts` |
| Agent bridge, links, view reports, page images | `main/agents/`; `shared/agent-api.ts`, `render-api.ts`, `revision.ts` (`@unfold/desktop/revision`) |
| What a page draws, read one way for the window and the MCP server: the walk of a drawn flow, how a card shows, the arrows between cards | `shared/drawing.ts` (`@unfold/desktop/drawing`) |
| MCP server, tools, guide | `main/mcp/`; tools in `main/mcp/tools/`, guide topics in `main/mcp/guide/` |
| Build and packaging | `electron.vite.config.ts`, `electron-builder.yml` |

## Transport and windows

The preload exposes only `window.unfold`. Engine commands travel over the window's MessagePort to the engine host,
which hands them to the addon; window commands (`window_info`, `export_png`, `report_view`, …) go to main over IPC.
Main's own trusted requests, such as adding a project from the folder picker, saving Settings, and the window's
state, use the utility process's parent port. Adding an engine command changes the Rust command table, not each hop.

Main restarts a crashed engine host and hands the window fresh ports; the window reconnects with its board and
unsaved edits. Main does no graph work and never waits on the engine.

There is one window, with no title bar: the macOS window buttons sit over the sidebar and the top bar moves the
window. Closing it keeps the app running; the Dock, a second launch, File › Add Project…, or an agent that needs it
opens it again. It comes back with its bounds, the last board shown, and the Show all calls switch, saved in the app
data's `window.json` (through the engine) and placed on a display that still exists.

## Agents

The MCP server lives in main and listens on `127.0.0.1:29180` at `/mcp` from launch until quit, with no fallback
port; Settings › Agents says when another program holds the port. It is stateless Streamable HTTP and offers tools and
instructions only. The listener refuses requests with an `Origin` header, a `Host` other than this port on
`127.0.0.1` or `localhost`, or an oversized body. Each request names its client: Claude Code signs its edits as
`agent:claude`, Codex as `agent:codex`.

- **Tools** are `defineTool`s in `main/mcp/tools/`, listed in order in `tools/index.ts`. Every object in a tool's
  arguments is a `z.strictObject`, so an unknown argument is refused, never dropped. Results and refusals stay under
  about 6,000 tokens, and refusals say what to call instead. Tools read through the engine with untrusted requests, as a
  window does.
- **The guide** (`main/mcp/guide/*.md`) and the server instructions (`instructions.ts`) teach agents that use
  Unfold. `teaching.test.ts` checks their size limits and that they name only tools, arguments, and topics that
  exist. `tasks.md` is the manual task suite for the person tuning that teaching; agents never see it. Edit them when
  product behavior changes, not as repository onboarding.
- **The agent bridge** (`main/agents/bridge.ts`) has the window make an agent's edits as it makes the person's: one
  undo step each, signed with the agent's name, refused when the page changed since the agent's `expectRevision`
  (the window compares it as it saves the edit, with `shared/revision.ts`).
  An edit to a board the window doesn't hold loads that board's session in the background without changing what the
  person sees; if the window is closed, the bridge opens it (after the launch restore). A request that times out is
  `unconfirmed` and may still land, so re-read before retrying.
- **Links** are `unfold://<folder name>/<board id>?page=<page id>&item=<item id>`. Only main parses them, resolving
  the folder name against the workspace. They aren't registered with the OS.
- **The view** (`main/agents/views.ts`) is the window's last report: its board, page, selection, what the code panel
  shows, and the boards it holds. Tools default to it.

## The render window

`PageRenderer` (`main/agents/render.ts`) draws one page read-only in a hidden, hardened window (`render.html`) for
`get_screenshot`, the board menu's Export PNG…, and `check`. The page comes from the window when it holds the board,
unsaved edits included, else from the engine, and its health from `board_health`, which judges the file of a board no
window has open: neither opens the window or a board session. The render window reads card data from the engine but
holds no board session. Rendering waits for fonts and measured content and bounds the canvas size, image bytes,
deadlines, and queued work.

## Packaging

`just bundle-dir` builds the release addon, the UI, and the shell, checks the notices, and runs electron-builder for
the unpacked macOS app (`dist/mac*/Unfold.app`). The app holds the built UI and shell in `app.asar`, and the addon
and `THIRD_PARTY_NOTICES.md` beside it in its resources.

Hardening: sandboxed windows with context isolation and no Node integration, a strict content security policy on
`app://`, no navigation, new windows, webviews, or permissions, and Electron fuses flipped at packaging. The app is
signed only with a certificate named in `CSC_LINK` or `CSC_NAME`; otherwise it is signed ad hoc.

## Running and checks

`just dev` builds the debug addon and runs the UI's dev server and the shell with hot reload. `just js-build` builds
the UI and the shell; `pnpm --filter @unfold/desktop start` runs them. Unit tests use a fake Electron
(`src/main/fake-electron.ts`): `pnpm --filter @unfold/desktop test <file>` and `pnpm --filter @unfold/desktop
typecheck`. See [choosing checks](../docs/testing.md#choose-checks-for-a-local-change) before building the app for a
shell change.
