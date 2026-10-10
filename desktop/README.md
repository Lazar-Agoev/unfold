# desktop

Unfold's Electron shell, in TypeScript. It holds no graph logic: it manages windows and the operating
system's dialogs, and routes messages between windows and the Rust engine
([architecture §4](../docs/architecture.md#4-system-overview), [§13](../docs/architecture.md#13-security-and-privacy)).

## Processes

| Process | Source | Does |
| --- | --- | --- |
| Main | `src/main/` | Lifecycle, the single-instance lock, windows, the menu, dialogs, the `app://` protocol, hardening, and supervising the engine host |
| Engine host | `src/engine-host/` | A utility process. Loads the `unfold-node` addon (`crates/unfold-node`) and routes each window's port to it |
| Preload | `src/preload/` | Runs in each sandboxed window and exposes `window.unfold`, and nothing else |

Main process modules:

- `index.ts`: lifecycle. One copy per user (a second launch forwards its folder argument, which joins the workspace). At launch the saved windows come back. macOS keeps running with no windows; Windows and Linux quit with the last one. On quit, the engine host detaches every window (landing its board writes) and closes every live project before the app exits.
- `windows.ts`: the window factory (context isolation, sandbox, no Node, no webviews, no spellchecker) and the registry of windows. Each window has a numeric id and a stable key, and shows the whole workspace.
- `protocol.ts`, `app-url.ts`: the privileged `app://unfold/` scheme. It serves the built UI with a strict content security policy: no inline scripts or styles, no remote origins. In development, windows load the Vite dev server instead.
- `security.ts`: blocks navigation, `window.open`, webviews, downloads, and every permission request. Cancels any request that would leave the machine, and checks the sender frame of every IPC message.
- `menu.ts`: the app menu. Add Project… (the folder picker, then the folder joins the workspace) and New Window run in the main process. Every other item, Edit > Undo and Redo included, is forwarded to the focused window as a `menu_command` event.
- `dialogs.ts`: the folder picker, the export save dialog, and the Remove Project confirmation. Paths outside a project come only from here, from a folder the person dropped on a window, or from the command line.
- `engine-host.ts`: forks the engine host and restarts it if it exits, with backoff. It gives up after more than five exits in a minute, and windows then show "unavailable". Each window gets its own `MessageChannelMain`. It also carries the main process's own trusted engine requests (`request`).
- `workspace.ts`: windows coming back, adding projects from the menu, the command line, the OS, or a drop, and remembering windows (see [Window restore](#window-restore)).
- `window-placement.ts`: reading saved windows and moving a restored window onto a visible screen.
- `export-name.ts`: the export dialog's default file name.
- `ipc.ts`: the window commands ([docs/ipc.md](../docs/ipc.md#window-commands-and-shell-events)), plus `add_dropped_folder`, which only the preload sends.
- `app-paths.ts`: the app folders from architecture §8.2, the UI and addon locations, and parsing the folder from the command line.
- `updater.ts`, `update-policy.ts`: updates from GitHub Releases with electron-updater, the app's only network use (see [Packaging](#packaging)).

## Transport

The wire types live in `src/shared/transport.ts`. The `window.unfold` surface is in `src/shared/unfold-api.ts`, a types-only file the UI imports as `@unfold/desktop/unfold-api`.

```text
renderer (preload) ──MessagePort──▶ engine host ──dispatch/cancel──▶ addon (Rust)
        ▲                                │  ◀──sink(JSON)── events, stream messages
        └──── Electron IPC ──── main ────┘ (lifecycle only: init, attach, detach, shutdown)
```

- **Port protocol.** The renderer sends `{kind:"invoke", id, command, args}` and `{kind:"cancel", id}`. The host replies with `{kind:"result", id, ok, value|error}`, `{kind:"stream", id, message}`, and `{kind:"event", event}`. It also sends one `{kind:"status", state}` per attach, saying whether the addon is loaded.
- **Request ids.** The host gives every invoke a request id that is unique across all windows, because the addon's `cancel` takes only a request id. Stream messages are routed back by window id and request id, events by the event's `windows` list, and broadcasts (every project event) to every window.
- **Attach.** Every window shows the whole workspace, so attaching opens nothing: a window's commands dispatch at once. On detach, the host cancels the window's requests except closing writes (board and folder writes, the window's state and bounds), then calls `detachWindow`, which waits for those writes and releases the window's board claims and selection. Detaches of one window run in order. A reload only swaps the port.
- **No addon.** If the addon can't be loaded, the engine host logs why (`[engine-host] the engine addon could not be loaded from …`), every invoke is answered with `engine_unavailable`, and each window shows the "Reconnecting to the engine…" toast. The app never shows made-up data.
- **Trusted commands.** The commands that take an absolute path or manage windows ([docs/ipc.md](../docs/ipc.md#how-a-command-travels), `MAIN_PROCESS_ONLY` in `router.ts`) run only with `"trusted": true`, which only the main process's own requests get: main sends `{type:"request", id, windowId, command, args}` over the parent port, the engine host dispatches it with `trusted`, and answers `{type:"result", id, ok, value|error}`. A window's message naming one of these commands, or carrying a `trusted` field anywhere, is refused with `untrusted` before it reaches Rust; the router rebuilds every window command from `command` and `args`, so a window can't add the flag.
- **Restarts.** If the engine host dies, each window is told, rejects its in-flight requests with `disconnected`, and shows the "Reconnecting to the engine…" toast. Once the engine host restarts, every window is attached again with a fresh port. Invokes made while no port is present are queued.
- **Shell events.** Some events come from the shell rather than from Rust, and arrive through `window.unfold.on` like engine events: `menu_command`, `board_state_transferred`, `focus_board`, `project_added`, and `project_add_failed`.

The addon is loaded from `crates/unfold-node` in a checkout, and from `<resources>/unfold-node` when packaged: the engine host `require`s that directory, whose `package.json` points at the napi-rs loader `index.js`, which loads `unfold-node.<platform>.node` beside it (for example `unfold-node.darwin-arm64.node`).

## The engine addon

`crates/unfold-node` is a napi-rs v3 addon over `unfold_workspace::Host`, with no logic of its own. It exports exactly `init`, `subscribe`, `dispatch`, `cancel`, `detachWindow`, and `shutdown` ([docs/ipc.md](../docs/ipc.md#the-addons-surface-host)). Promises settle from Rust threads through thread-safe deferreds, so the JavaScript thread never waits. Sink messages and settlements share one ordered channel, a weak (unref'd) thread-safe function called without blocking, so a request settles only after its stream messages reached the sink; a panic rejects with `internal`. Build it with `just addon` (debug) or `just addon-release`; the napi CLI writes `unfold-node.<platform>.node`, `index.js`, and `index.d.ts` into the crate folder. `just addon-test` runs its Node tests against the build.

## Window commands that involve the engine

- **Add and remove projects.** `add_project` shows the folder picker, then sends the trusted `add_project` and returns `{status, project}` (`exists` when the folder or its repository already is a project), or null if the picker was cancelled. Remove Project asks first with `confirm_remove_project {name}` ("Remove <name> from the workspace? Its .unfold folder stays."); the UI then calls the engine's `remove_project`.
- **Export.** The UI calls `window.unfold.window("export_mermaid", {project, board, page, title})` (or `{project, entrance, title}`). The main process shows the save dialog (default name `<title>.mmd`, in Documents), then sends the trusted `export_mermaid` request for the calling window with the chosen path, and returns `{path, bytes}`, or null if the dialog was cancelled. Engine errors (`board_not_held`, `io`, …) reject with their code. `export_mermaid_text` stays an ordinary engine command, for copying.
- **Clipboard.** Windows get no permissions at all, so `navigator.clipboard.writeText` is denied (`NotAllowedError`, verified end to end). The UI copies with the window command `copy_to_clipboard {text}`, which the main process writes with Electron's `clipboard`. Copy Mermaid is `export_mermaid_text` followed by `copy_to_clipboard`.
- **Move to New Window.** `move_tab_to_new_window` opens a window and resolves with its id at once. The UI then calls `move_board {project, id, toWindow}` (the claim moves in Rust) and `window.unfold.window("transfer_board_state", {toWindow, payload})`; the target window receives `{type:"board_state_transferred", fromWindow, payload}` (held until its UI has loaded). Subscribe to it at startup.
- **Held boards.** When `open_board` answers `held` with `heldBy`, the UI calls `focus_window {windowId, board: {project, board}}`; that window receives `focus_board` and shows the tab.
- **Drag and drop.** Dropping a folder on a window adds it to the workspace. The preload records the paths of the latest drop (`webUtils.getPathForFile`) and only those can be added: `window.unfold.addDroppedFolder(file)` from a UI drop handler, or, if the UI doesn't handle the drop, the preload adds a single dropped folder by itself and emits `project_added` (or `project_add_failed` with the engine's reason). The engine checks the path is a folder (`not_a_folder` otherwise).
- **Undo and Redo.** Edit > Undo / Redo (and their shortcuts when the UI didn't handle the key) arrive as `menu_command` `undo` / `redo`. With focus in a text field, the preload runs the field's native undo instead and the UI sees nothing.

## Window restore

[Architecture §11.10](../docs/architecture.md#1110-several-windows-and-restore). Every window has a stable `key` (`window_info().key`), and `windows.json` in app data remembers each one.

- At launch the main process asks the engine for the saved windows (trusted `saved_windows`) and recreates each with its key, bounds, and maximized state, moved onto the primary screen if no screen shows its title bar any more. At most 16 come back; with none, one new window opens. A folder from the command line, `open-file`, or a second launch joins the workspace, and the focused window receives `project_added` so it selects it, or `project_add_failed {path, message}` so it says why it couldn't be added.
- The main process saves a window's bounds (`save_window_bounds`) when it is created and after each move or resize (debounced 500 ms).
- The UI saves each window's selected project, tabs, and layout with `save_window_state {state: {key, …}}` and restores them with `window_state {key}`.
- A window the person closes is forgotten (`forget_window`) unless it was the last one. The last window, and every window open when the app quits, comes back.

## Packaging

`electron-builder.yml` packages the app ([architecture §14](../docs/architecture.md#14-packaging-platforms-and-updates)): a `.dmg` and `.zip` per Mac architecture, an NSIS installer for Windows x64, and an AppImage, `.deb`, and `.rpm` for Linux x64. `just bundle` builds this platform's installers into `dist/`, `just bundle-dir` only the unpacked app.

- **What ships.** The archive holds `package.json`, `out/` (main, engine host, and preload, bundled by electron-vite with electron-updater inside), and `ui/` (the Vite build), without source maps or `node_modules`. The addon (`index.js`, `package.json`, and this platform's `.node` file) sits outside the archive at `<resources>/unfold-node`, next to `THIRD_PARTY_NOTICES.md` (and, on macOS, Electron's `LICENSE.electron.txt` and `LICENSES.chromium.html`).
- **Fuses.** Set at package time ([§13](../docs/architecture.md#13-security-and-privacy)): RunAsNode, NODE_OPTIONS, and the inspector off; cookie encryption, asar integrity validation, and loading only from the asar on; no extra `file://` privileges. Flipping fuses voids the Electron binary's signature, so an unsigned macOS build is re-signed ad hoc.
- **Signing.** Only from the environment (`CSC_LINK` / `WIN_CSC_LINK` and their passwords; notarization from `APPLE_API_KEY*`). `just bundle` never uses an identity it merely finds in the keychain, so local builds are unsigned unless you pass one. The hardened runtime's only entitlement is JIT (`build/entitlements.mac.plist`).
- **Updates.** `updater.ts` checks GitHub Releases half a minute after launch and every six hours, if the `updates.enabled` preference allows it. Downloads must match the manifest's SHA-512; macOS and Windows install only updates signed like the running app (on Windows, by a publisher named in `app-update.yml`); the AppImage replaces itself; `.deb` and `.rpm` installs get a notice linking to the release. Updates are off in development and in any build the release workflow didn't make (it sets `unfoldReleaseBuild` in the packaged `package.json`).
- **Placeholders** (U-3): the app ID `dev.unfold.app`, the release feed `unfold-placeholder/unfold` (also `RELEASES_URL` in `update-policy.ts`), the Windows publisher name, and the Linux maintainer.
- **Smoke test.** `scripts/smoke.mts` (`just smoke`) runs against the unpacked app: it reads the fuses back from the binary; launches the app with an empty `PATH`, a throwaway home, and `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`, and `--inspect` (all ignored); attaches Playwright over the DevTools protocol; adds a copy of the TypeScript playground in a path with spaces and non-ASCII letters as a project and compiles it; waits for the project to be current; checks the window from the inside (no Node globals, only `window.unfold`, inline scripts and the network blocked); and checks that nothing outside `.unfold/` changed and no planted execution sentinel ran. Every window's `webPreferences` are read from the main process of a copy of the app with the inspector fuse turned back on, because Playwright's Electron driver needs the inspector.
- **Icon.** `build/icon.svg` is a placeholder; `pnpm --filter @unfold/desktop exec electron ../tools/icons/render-icon.mjs` renders `build/icon.png`, from which electron-builder makes each platform's icons.

## Running

From the repository root:

```sh
pnpm install          # or: just js-install
just addon            # builds the engine addon (debug) into crates/unfold-node
just dev              # builds the addon, then the Vite dev server for the UI + electron-vite, with hot reload
just js-build         # builds ui/dist and desktop/out
pnpm --filter @unfold/desktop start   # runs the built app (loads ui/dist via app://)
just js-check         # typecheck, lint, and unit tests
```

Pass a folder to add it to the workspace: `pnpm --filter @unfold/desktop start -- /path/to/repo`.
