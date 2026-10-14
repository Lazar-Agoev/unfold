# Unfold developer commands. Run `just --list` to see them.

# --- Rust recipes ---

# Run every Rust test in the workspace.
test:
    cargo test --workspace

# Check formatting and lints (warnings are errors).
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings

# Run a developer task: `just xtask gen-ipc --check`.
xtask *args:
    cargo xtask {{args}}

# --- Engine addon (crates/unfold-node) ---

# Build the engine addon (debug) where the app loads it: crates/unfold-node/unfold-node.<platform>.node.
addon:
    pnpm --filter @unfold/node build:debug

# Build the engine addon with the release profile (what packaging ships).
addon-release:
    pnpm --filter @unfold/node build

# Test the built addon through its JavaScript surface.
addon-test: addon
    pnpm --filter @unfold/node test

# --- JS recipes (desktop/ui) ---

# Install the JavaScript workspace (desktop, ui, the addon's build tooling).
js-install:
    pnpm install

# Build the engine addon, then run the app with hot reload for the UI (Vite dev server + electron-vite).
dev: addon dev-stop
    #!/usr/bin/env bash
    set -euo pipefail
    electron="{{justfile_directory()}}/node_modules/.pnpm/electron@"
    pnpm --filter @unfold/ui dev &
    ui=$!
    # Ctrl+C or the app quitting ends the whole session. The app quits gracefully on Ctrl+C (landing
    # board writes); anything still running after 8 s is stopped, so no Electron outlives the session.
    stop_session() {
        kill "$ui" 2>/dev/null || true
        for _ in $(seq 80); do pgrep -f "$electron" >/dev/null || return 0; sleep 0.1; done
        pkill -f "$electron" 2>/dev/null || true
    }
    trap stop_session EXIT
    UNFOLD_UI_DEV_URL=http://localhost:5173 pnpm --filter @unfold/desktop dev

# A leftover dev app holds the single-instance lock, so a new session would exit at once.
# Stop the Unfold dev app an earlier `just dev` left running (only this checkout's Electron).
dev-stop:
    #!/usr/bin/env bash
    electron="{{justfile_directory()}}/node_modules/.pnpm/electron@"
    pgrep -f "$electron" >/dev/null || exit 0
    echo "Stopping the Unfold dev app left running by an earlier session…"
    pkill -f "$electron" || true
    for _ in $(seq 25); do pgrep -f "$electron" >/dev/null || exit 0; sleep 0.2; done
    pkill -9 -f "$electron" || true

# Typecheck, lint, and unit-test the desktop shell and the UI.
js-check:
    pnpm -r typecheck
    pnpm -r lint
    pnpm -r --filter "!@unfold/e2e" test

# Build the UI and the Electron app (run it with `pnpm --filter @unfold/desktop start`).
js-build:
    pnpm --filter @unfold/ui build
    pnpm --filter @unfold/desktop build

# Build the release addon, the UI, and the desktop shell, then run the end-to-end journeys (e2e/)
# on the unpackaged app. Pass Playwright arguments through: `just e2e journeys/slice.spec.ts`.
e2e *args: addon-release js-build
    pnpm --filter @unfold/desktop exec node -e "require('electron')"
    pnpm --filter @unfold/e2e test {{args}}

# --- Packaging (desktop/electron-builder.yml) ---

# macOS signing only with a certificate named in the environment (CSC_LINK or CSC_NAME), never with
# whichever identity happens to be in the login keychain; without one the build is unsigned.
csc_discovery := if env("CSC_LINK", "") + env("CSC_NAME", "") == "" { "false" } else { env("CSC_IDENTITY_AUTO_DISCOVERY", "true") }

# The unpacked app (desktop/dist/mac*/Unfold.app).
bundle-dir $CSC_IDENTITY_AUTO_DISCOVERY=csc_discovery: _bundle-inputs
    pnpm --filter @unfold/desktop run bundle:dir

# Write THIRD_PARTY_NOTICES.md (it ships inside the app).
notices:
    cargo xtask notices

_bundle-inputs: addon-release
    pnpm --filter @unfold/ui build
    pnpm --filter @unfold/desktop build
    cargo xtask notices --check
    pnpm --filter @unfold/desktop exec node -e "require('electron')"
