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

- `index.ts`: lifecycle. One copy per user (a second launch forwards its folder argument). macOS keeps running with no windows; Windows and Linux quit with the last one. On quit, the engine host closes every workspace (saving drafts) before the app exits.
- `windows.ts`: the window factory (context isolation, sandbox, no Node, no webviews, no spellchecker) and the registry of windows. Each window has a numeric id and shows at most one folder.
- `protocol.ts`, `app-url.ts`: the privileged `app://unfold/` scheme. It serves the built UI with a strict content security policy: no inline scripts or styles, no remote origins. In development, windows load the Vite dev server instead.
- `security.ts`: blocks navigation, `window.open`, webviews, downloads, and every permission request. Cancels any request that would leave the machine, and checks the sender frame of every IPC message.
- `menu.ts`: the app menu. Open Folder in New Window and New Window run in the main process. Open Folder goes to the window it acts on as a `menu_command` (`open_folder`), so the window saves its boards before its folder is replaced; with no window it runs in the main process. Every other item, Edit > Undo and Redo included, is forwarded to the focused window as a `menu_command` event.
- `dialogs.ts`: the folder picker and the export save dialog. Paths outside the open folder come only from here, or from a folder the person dropped on a window.
- `engine-host.ts`: forks the engine host and restarts it if it exits, with backoff. It gives up after more than five exits in a minute, and windows then show "unavailable". Each window gets its own `MessageChannelMain`. It also carries the main process's own trusted engine requests (`request`) and tells it when a window's workspace is open (`whenAttached`).
- `folders.ts`: opening folders in windows and windows coming back (see [Window restore](#window-restore)).
- `window-placement.ts`: reading saved windows and moving a restored window onto a visible screen.
- `export-name.ts`: the export dialog's default file name.
- `ipc.ts`: the window commands `new_window`, `focus_window`, `close_window`, `choose_folder`, `open_folder`, `choose_export_path`, `export_mermaid`, `move_tab_to_new_window`, `transfer_board_state`, `copy_to_clipboard`, and `window_info`, plus `open_dropped_folder`, which only the preload sends.
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
- **Request ids.** The host gives every invoke a request id that is unique across all windows, because the addon's `cancel` takes only a request id. Stream messages are routed back by window id and request id, and events by the event's `windows` list.
- **Attach.** On attach, the host calls `openWorkspace(windowId, folder)`, and the window's commands wait for it. On detach, or a change of folder, it calls `closeWorkspace`. Lifecycle calls for one window run in order. A reload that keeps the same folder only swaps the port.
- **No addon.** If the addon can't be loaded, the engine host logs why (`[engine-host] the engine addon could not be loaded from …`), every invoke is answered with `engine_unavailable`, and the status bar says "Engine unavailable". The app never shows made-up data.
- **Trusted commands.** `export_mermaid` (writes to a path from a save dialog) and `folder_windows` (reads another folder's saved windows) run only with `"trusted": true`, which only the main process's own requests get: main sends `{type:"request", id, windowId, command, args}` over the parent port, the engine host dispatches it with `trusted` (after that window's workspace is open), and answers `{type:"result", id, ok, value|error}`. A window's message naming one of these commands, or carrying a `trusted` field anywhere, is refused with `untrusted` before it reaches Rust; the router rebuilds every window command from `command` and `args`, so a window can't add the flag.
- **Restarts.** If the engine host dies, each window is told, rejects its in-flight requests with `disconnected`, and shows "Reconnecting…". Once the engine host restarts, every window is attached again with a fresh port. Invokes made while no port is present are queued.
- **Shell events.** Some events come from the shell rather than from Rust, and arrive through `window.unfold.on` like engine events: `menu_command`, `folder_changed`, and `workspace_open_failed`.

The addon is loaded from `crates/unfold-node` in a checkout, and from `<resources>/unfold-node` when packaged: the engine host `require`s that directory, whose `package.json` points at the napi-rs loader `index.js`, which loads `unfold-node.<platform>.node` beside it (for example `unfold-node.darwin-arm64.node`).

## The engine addon

`crates/unfold-node` is a napi-rs v3 addon over `unfold_workspace::Host`, with no logic of its own. It exports exactly `init`, `subscribe`, `openWorkspace`, `closeWorkspace`, `dispatch`, and `cancel` (brief §6, [docs/ipc.md](../docs/ipc.md#the-addons-surface-host)). Promises settle from Rust threads through thread-safe deferreds, so the JavaScript thread never waits; the sink is a weak (unref'd) thread-safe function called without blocking; a panic rejects with `internal`. Build it with `just addon` (debug) or `just addon-release`; the napi CLI writes `unfold-node.<platform>.node`, `index.js`, and `index.d.ts` into the crate folder. `just addon-test` runs its Node tests against the build.

## Window commands that involve the engine

- **Export.** The UI calls `window.unfold.window("export_mermaid", {boardId, title})` (or `{entrance, title}`). The main process shows the save dialog (default name `<title>.mmd`, in Documents), then sends the trusted `export_mermaid` request for the calling window with the chosen path, and returns `{path, bytes}`, or null if the dialog was cancelled. Engine errors (`board_not_held`, `io`, …) reject with their code. `export_mermaid_text` stays an ordinary engine command, for copying.
- **Clipboard.** Windows get no permissions at all, so `navigator.clipboard.writeText` is denied (`NotAllowedError`, verified end to end). The UI copies with the window command `copy_to_clipboard {text}`, which the main process writes with Electron's `clipboard`. Copy Mermaid is `export_mermaid_text` followed by `copy_to_clipboard`.
- **Move to New Window.** `move_tab_to_new_window` opens a window on the same folder and resolves with its id once that window's workspace is open. The UI then calls `move_board {id, toWindow}` (the claim moves in Rust) and `window.unfold.window("transfer_board_state", {toWindow, payload})`; the target window receives `{type:"board_state_transferred", fromWindow, payload}` (held until its UI has loaded). Subscribe to it at startup.
- **Recent folders.** The welcome screen opens a recent folder with `open_folder {path, target?}`. The main process accepts the path only if the engine's `recent_folders` lists it (canonical paths compared) and it is still a directory (`not_recent` / `not_a_folder` otherwise), then opens it like `choose_folder`.
- **Held boards.** When `open_board` answers `held` with `heldBy`, the UI calls `focus_window {windowId}`.
- **Drag and drop.** Dropping a folder on a window opens it there. The preload records the paths of the latest drop (`webUtils.getPathForFile`) and only those can be opened: `window.unfold.openDroppedFolder(file, {target})` from a UI drop handler, or, if the UI doesn't handle the drop, the preload opens a single dropped folder in this window by itself. The main process checks the path is a directory (`not_a_folder` otherwise).
- **Undo and Redo.** Edit > Undo / Redo (and their shortcuts when the UI didn't handle the key) arrive as `menu_command` `undo` / `redo`. With focus in a text field, the preload runs the field's native undo instead and the UI sees nothing.

## Window restore

Architecture §11.2. Every window has a stable `key` (`window_info().key`; a `folder_changed` event carries the new one when a window switches folders).

- Opening a folder focuses a window already showing it. Otherwise the main process asks the engine for the folder's saved windows (trusted `folder_windows`) and recreates each with its key, bounds, and maximized state, moved onto the primary screen if no screen shows its title bar any more. The first saved window reuses the window the folder was opened in (welcome, or the window switching folders). At most 16 come back.
- The main process saves a window's bounds (`save_window_bounds`) when its workspace opens and after each move or resize (debounced 500 ms). The welcome window's bounds go to the recent list and are reused for the next welcome window.
- The UI saves each window's tabs and layout with `save_window_state {state: {key, …}}` and restores them with `window_state {key}`.
- A window the person closes, or switches to another folder, is forgotten (`forget_window_state`) unless it was the folder's last window. The last window of a folder, and every window open when the app quits, comes back.

## Packaging

`electron-builder.yml` packages the app ([architecture §14](../docs/architecture.md#14-packaging-platforms-and-updates)): a `.dmg` and `.zip` per Mac architecture, an NSIS installer for Windows x64, and an AppImage, `.deb`, and `.rpm` for Linux x64. `just bundle` builds this platform's installers into `dist/`, `just bundle-dir` only the unpacked app.

- **What ships.** The archive holds `package.json`, `out/` (main, engine host, and preload, bundled by electron-vite with electron-updater inside), and `ui/` (the Vite build), without source maps or `node_modules`. The addon (`index.js`, `package.json`, and this platform's `.node` file) sits outside the archive at `<resources>/unfold-node`, next to `THIRD_PARTY_NOTICES.md` (and, on macOS, Electron's `LICENSE.electron.txt` and `LICENSES.chromium.html`).
- **Fuses.** Set at package time ([§13](../docs/architecture.md#13-security-and-privacy)): RunAsNode, NODE_OPTIONS, and the inspector off; cookie encryption, asar integrity validation, and loading only from the asar on; no extra `file://` privileges. Flipping fuses voids the Electron binary's signature, so an unsigned macOS build is re-signed ad hoc.
- **Signing.** Only from the environment (`CSC_LINK` / `WIN_CSC_LINK` and their passwords; notarization from `APPLE_API_KEY*`). `just bundle` never uses an identity it merely finds in the keychain, so local builds are unsigned unless you pass one. The hardened runtime's only entitlement is JIT (`build/entitlements.mac.plist`).
- **Updates.** `updater.ts` checks GitHub Releases half a minute after launch and every six hours, if the `updates.enabled` preference allows it. Downloads must match the manifest's SHA-512; macOS and Windows install only updates signed like the running app (on Windows, by a publisher named in `app-update.yml`); the AppImage replaces itself; `.deb` and `.rpm` installs get a notice linking to the release. Updates are off in development and in any build the release workflow didn't make (it sets `unfoldReleaseBuild` in the packaged `package.json`).
- **Placeholders** (U-3): the app ID `dev.unfold.app`, the release feed `unfold-placeholder/unfold` (also `RELEASES_URL` in `update-policy.ts`), the Windows publisher name, and the Linux maintainer.
- **Smoke test.** `scripts/smoke.mts` (`just smoke`) runs against the unpacked app: it reads the fuses back from the binary; launches the app with an empty `PATH`, a throwaway home, and `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`, and `--inspect` (all ignored); attaches Playwright over the DevTools protocol; compiles a copy of the TypeScript playground in a path with spaces and non-ASCII letters; waits for `graph_status` to be current; checks the window from the inside (no Node globals, only `window.unfold`, inline scripts and the network blocked); and checks that nothing outside `.unfold/` changed and no planted execution sentinel ran. Every window's `webPreferences` are read from the main process of a copy of the app with the inspector fuse turned back on, because Playwright's Electron driver needs the inspector.
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

Pass a folder to open it: `pnpm --filter @unfold/desktop start -- /path/to/repo`.
