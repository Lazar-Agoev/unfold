# Unfold

Unfold is a desktop app for reading a codebase as a graph. You open a folder and press **Compile**. Unfold shows the ways the code can start (HTTP routes, queue workers, schedules, commands, and launch scripts) and lets you follow each one, call by call, as cards on a board. Each card shows a function either as its exact source (**Code**) or as a diagram of its structure (**Flow**). You can explain a path with descriptions and labels, then publish the board into the repository. After the first Compile, the graph keeps itself current in the background. Unfold reads TypeScript and Python, and it never runs the code it reads.

**Status:** V1 is implemented: the Rust engine (TypeScript and Python), the reader and boards, and the Electron app with several windows. It has been built and verified only on macOS (Apple silicon), locally: the Rust tests, parity with the proof of concept's reference, the precision and recall gates against the TypeScript checker and Pyright ([results](docs/testing.md#precision-and-recall-gates)), the UI, desktop, and addon tests, and the packaged smoke test of the unsigned app from `just bundle` (Electron fuses, window hardening, and a Compile with no Node or Python on `PATH`). The Playwright journeys and budget measurements live in `e2e/` ([budgets](docs/testing.md#budgets-measured)). Configured but not yet run: the Windows (NSIS), Linux (AppImage, `.deb`, `.rpm`), and Intel Mac builds; the GitHub Actions workflows (no CI run has happened yet); code signing and notarization; and self-updates. The bundle identifier, update feed, and publisher names are placeholders until the trademark check of the name ([U-3](docs/decisions-and-tradeoffs.md#u-3-names-and-words)).

## The docs

| Doc | Read it for | What it explains |
| --- | --- | --- |
| [Architecture](docs/architecture.md) | What we're building. Start here | The product and its guarantees, the graph model, and the Rust engine (oxc for TypeScript, Ruff's parser for Python). How Compile and background updates work, the `.unfold/` folder and the SQLite graph, and the board and settings formats. The reader, the windows, the Electron app's processes and the commands between the UI and the engine, and the repository layout. Testing, performance targets, and risks |
| [Decisions and trade-offs](docs/decisions-and-tradeoffs.md) | Why it's built this way | How the design came about, then every decision (U-1 to U-37) with the options we weighed and their pros and cons, such as why the engine is Rust and the shell is Electron. It ends with what we rejected and what's still open |
| [Migration plan](docs/migration-plan.md) | The order we build it in | What moves over from the proof of concept and what stays behind. The migration step by step: Steps 0–11 move each part of the proof of concept into Unfold and end with V1 on macOS, Windows, and Linux, followed by the V2 tracks. What V1 and V2 contain, the gates each step must pass, and what we do if a risky part fails |
| [User guide](docs/user-guide.md) | Using the app | Opening a folder, the first Compile, entrances, following calls, Code and Flow, explaining a board, drafts and Publish, several windows, background updates, settings, and what to commit |
| [UI shell](docs/ui-shell.md) | The window as built | Every part of the window, its commands, and the keyboard map |
| [Testing](docs/testing.md) | How it's checked | The kinds of tests, how to run every gate, the golden and divergence policies, and measured results against the proof of concept |
| [All docs](docs/README.md) | Finding a fact | A map of every document and what each owns, including the reference documents below |

## Working on Unfold

### Prerequisites

- **Rust**, through [rustup](https://rustup.rs). The toolchain is pinned in `rust-toolchain.toml` and installs itself on first use. If `cargo` isn't on your `PATH`, run `source ~/.cargo/env` first.
- **Node.js 24 or later** and **pnpm** (the version named by `packageManager` in `package.json`; for example `npm install -g pnpm@11.9.0`).
- **[just](https://github.com/casey/just)**, the command runner. `just --list` shows every recipe.
- On Linux, `rpm` to build the `.rpm` package.

### Build and run

```sh
pnpm install          # the JavaScript workspace: desktop, ui, the addon's build tooling
just dev              # builds the engine addon (debug), then runs the app with hot reload for the UI
```

Pass a folder to open it: `pnpm --filter @unfold/desktop start -- /path/to/repo` runs the built app (`just js-build` builds it).

### Test and check

```sh
just test             # every Rust test in the workspace
just check            # rustfmt and clippy (warnings are errors)
just js-check         # TypeScript typecheck, ESLint, and Vitest for desktop and ui
just addon-test       # the engine addon's Node tests
just e2e              # the Playwright journeys on the built app (just e2e-budgets measures the budgets)
cargo xtask parity mixed-playground       # parity with the proof of concept's reference (also playground-ts-native, python-playground, entrances)
cargo xtask oracles check                 # the precision and recall gates
cargo xtask gen-ipc --check               # the UI's generated command types are current
cargo deny check      # licenses, advisories, and sources of the Rust dependencies
```

[docs/testing.md](docs/testing.md) lists every kind of test and gate.

### Package

```sh
just bundle           # installers for this platform in desktop/dist (unsigned without certificates)
just bundle-dir       # only the unpacked app, for smoke tests
just smoke            # launches the unpacked app: fuses, window hardening, a Compile, nothing written outside .unfold/
just notices          # rewrites THIRD_PARTY_NOTICES.md, which ships inside the app
```

Packaging is configured in `desktop/electron-builder.yml`; signing certificates come from the environment, as described there and in `.github/workflows/release.yml`. CI is configured to run `pr.yml` on every pull request (macOS, Windows, and Linux), `nightly.yml` every night, and `release.yml` on version tags; none of the workflows has run yet.

### Developer tasks

`cargo xtask` is the developer command line. It is never shipped.

| Command | Does |
| --- | --- |
| `cargo xtask compile <folder>` | Compiles a folder headlessly and prints what the Index log would show |
| `cargo xtask export <folder>` | Compiles a folder and writes its canonical export |
| `cargo xtask export-store <folder>` | Writes the canonical export of the graph a folder's workspace committed, to compare background updates with a fresh export |
| `cargo xtask parity <corpus>` | Compiles a reference corpus and compares it with the proof of concept's frozen output and the divergence ledger |
| `cargo xtask oracles check` / `refresh` | The precision and recall gates against the cached TypeScript checker and Pyright answers, or regenerating those answers |
| `cargo xtask canonical-check` / `canonical-diff` | Checks that a canonical export round-trips, or compares two exports record by record |
| `cargo xtask gen-lib-tables` | Regenerates the TypeScript front end's tables of library globals and Node built-ins |
| `cargo xtask gen-ipc` | Regenerates the TypeScript types of the command table (`--check` in CI) |
| `cargo xtask reference import` | Converts the frozen proof-of-concept outputs into the reference under `testdata/reference/` |
| `cargo xtask notices` | Writes `THIRD_PARTY_NOTICES.md` (`--check` in CI) |

### Where things live

- `crates/`: the Rust engine (`unfold-core`, `-engine`, `-lang-typescript`, `-lang-python`, `-store`, `-reader`, `-workspace`) and the Node addon (`unfold-node`).
- `desktop/`: the Electron main process, preload, and engine host, and the packaging; see [desktop/README.md](desktop/README.md).
- `ui/`: the React UI. `testdata/`: fixtures, playgrounds, and the reference. `tools/xtask/`: developer tasks.
- `docs/`: the documents above ([the map](docs/README.md)), plus reference documents for the [graph model](docs/graph-model.md), [compiling](docs/compiling.md), [settings and scope](docs/settings-and-scope.md), [entrances](docs/entrances.md), [TypeScript](docs/languages/typescript.md) and [Python](docs/languages/python.md), the [reader](docs/reader.md), and the [commands and events](docs/ipc.md).

## The proof of concept

Unfold rebuilds a proof of concept, [`graph-compiler`](../graph-compiler), which sits next to this repository at `~/Desktop/repos/graph-compiler`. Its product was called Codegraph. It was built to prove one idea: that TypeScript and Python can be compiled, without running them, into a graph of the code that people can trust and read. It proved it. It was never deployed, and it is now a frozen reference.

**What it built**

- **A compiler** that reads TypeScript and Python and records three things:
  - the code's structure: functions, classes, branches, loops, and calls;
  - what each call targets;
  - how the code can start, from package scripts and bins, Express, BullMQ, node-cron, Python console scripts, FastAPI, Celery, APScheduler, and manual declarations.

  A call is linked only when there is proof of exactly one target. Otherwise, the graph records why the call can't be followed.
- **A reader** that draws the graph as cards, checks each file's hash before showing its code, and saves boards of explanations.
- **Evidence that the approach holds.** 854 compiler tests, 57 reader tests, and 3 browser tests passed, and every linked call was checked against an independent oracle: the TypeScript type checker or Pyright.

**How it was built differently**

| Proof of concept | Unfold |
| --- | --- |
| A compiler CLI and a separate browser reader, joined by files and a local HTTP server | One desktop app (Electron) with the Rust engine inside it, and as many windows as you want |
| Written in TypeScript. It used a fork of TypeScript 6, a second TypeScript analyzer backed by the type checker, and vendored Pyright | A Rust engine, using oxc for TypeScript and Ruff's parser for Python |
| The graph written as CSV files into `.codecodegraph/`, with drafts and boards in two more folders at the repository root | One `.unfold/` folder: shared settings and published boards, plus a local graph cache and drafts that Git ignores |
| Re-index by hand after changes | Compile once, then background updates as files change |
| One sidebar with a flat list of definitions | An IDE-style window: files, entrances, search, boards, tabs, and an inspector |

**What carries over**

- Its rules: link a call only with proof, record only what the code declares, show code only after verifying it, produce the same graph from the same input, and never run anything.
- Its reader design.
- Its playgrounds and fixtures.
- Its frozen outputs, which become Unfold's test reference.

Its code doesn't carry over. This repository rebuilds it from scratch in its new form: one desktop app, a Rust engine, and a graph that keeps itself current.
