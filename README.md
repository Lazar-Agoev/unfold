# Unfold

Unfold is a desktop app for reading TypeScript and Python as a graph. Connect repositories,
Compile, start from routes/workers/scripts, and follow calls as cards on board pages.
Explain code with descriptions, types, resources, and connections, then publish boards into
the repository. Unfold statically analyzes code; it never runs the projects it reads.

## Working on the repo

Start with [AGENTS.md](AGENTS.md) and choose the relevant section from
[the task map](docs/README.md). Read the owning code and a nearby test; there is no required
tour of the docs. The [architecture overview](docs/architecture.md) explains process boundaries,
and the [user guide](docs/user-guide.md) explains the product.

| Area | Location |
| --- | --- |
| Graph/compiler/languages/store/reader/workspace | `crates/` |
| Electron main/preload/engine host/MCP | `desktop/` |
| React UI | `ui/` |
| Fixtures/reference/oracles | `testdata/`, `tools/oracles/` |
| Developer CLI | `tools/xtask/` (`cargo xtask`) |
| App journeys | `e2e/` |

Each crate's `src/lib.rs` and module comments explain its local contracts. Exact schemas,
command types, vocabulary lists, and version numbers live in source, not parallel doc copies.

## Run

Install Rust through rustup, Node (the supported version in `package.json`), the pinned pnpm
version, and `just`. `rust-toolchain.toml` pins Rust. See `just --list` for recipes.

```sh
pnpm install
just dev
```

`just dev` builds the debug engine addon and starts the UI/Electron dev servers. To run the
built app, use `just js-build`, then `pnpm --filter @unfold/desktop start -- /path/to/project`.

## Validate

Use [check selection](docs/testing.md#choose-checks-for-a-local-change). Focused crate/Vitest
checks come first. Docs/design tasks need no application build; E2E is conditional on changes
that require the running app. `just e2e` invokes release-addon, UI, and desktop builds, so
select a matching journey and reuse current builds for repeated runs.

Broad checks are `just test`, `just check`, `just js-check`, `just addon-test`, and the
parity/oracle/generated-type gates documented in testing. CI matrices are configured under
`.github/workflows/`; configuration alone does not establish that a gate passed.

## Package

`just bundle` builds installers, `just bundle-dir` the unpacked app, and `just smoke` checks
that packaged runtime. Packaging, signing, hardening, and platform verification belong to
[desktop](desktop/README.md#packaging). `cargo xtask notices` updates third-party notices.

## Reference data

Frozen compiler/reader outputs and independent oracle answers live under `testdata/reference/`.
They are checked by parity, precision/recall, and divergence policies. Consult
[their guide](testdata/reference/README.md) for that work; ordinary features do not require
reading the sibling proof-of-concept repository or its history.
