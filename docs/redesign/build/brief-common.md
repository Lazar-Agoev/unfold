# Rules for every implementation agent (studio redesign)

Repo: `/Users/lazaragoev/Desktop/repos/worktrees/unfold/youthful-lift-4622eda8` (a git worktree; work only here).

## Hard rules
- **No git operations at all** — not even `git rm`, `git mv`, `git add` or `git rm --cached`; delete and move files with plain `rm`/`mv`. No commit, branch, stash, checkout, restore, reset, push. The owner does git.
- Never touch `~/Desktop/repos/graph-compiler` (the frozen proof of concept) except reading it.
- Never run database migrations. Never edit the Paper design file (read-only Paper tools only).
- Never hand-edit `ui/src/ipc/generated/index.ts`; regenerate it with `cargo xtask gen-ipc`.
- Rust: cargo isn't on PATH. Prefix with `source "$HOME/.cargo/env" &&`. Use ONLY the `CARGO_TARGET_DIR` your brief
  names (never create new target dirs). Don't run `cargo clean`.
- Other agents edit other parts of the repo at the same time. Edit only the files your brief scopes to you. If you
  must touch something outside your scope, keep it minimal and say so in your report.

## Sources of truth
1. `docs/redesign/studio-redesign.md` (the redesign; every R-1..R-16 recommendation is accepted).
2. `docs/redesign/build/contract.md` (the engine/IPC/board contract, including §11 amendments, which win).
3. `docs/redesign/build/decisions.md` (build decisions V-n, A-n, P-n) and `docs/redesign/build/design/*.md` (exact
   design values extracted from Paper).
4. The existing docs for everything the redesign doesn't change (`docs/architecture.md`, `docs/ipc.md`, `docs/reader.md`,
   `docs/decisions-and-tradeoffs.md`, `docs/testing.md`, `desktop/README.md`).

## Ground rules from the owner
- Unfold has never shipped: **no backward compatibility** of any kind — no migration from old formats, no readers for
  old shapes, no shims, adapters, deprecated aliases, feature flags or old ids. Delete what the new design doesn't use.
  Rewrite rather than adapt when adapting means contortions.
- **No dead code** when you finish: every module, export, type, function, CSS rule, command, event, Rust item,
  dependency and test you leave must be used. Delete orphans you create or expose.
- Engine guarantees unchanged: reads files only, never runs the code it reads (nor Git), installs nothing, writes only
  in `.unfold/` and app data. Compilers, linking, resolution, detectors and graph semantics unchanged; the parity and
  oracle gates must pass unchanged.
- Security unchanged: context isolation, sandboxed renderers, CSP, `app://`, fuses, trusted commands; path confinement
  per project; the UI uses no Node APIs and no browser storage.
- Efficient by construction: Rust computes, TypeScript lays out; filter and paginate in Rust; small payloads, no
  per-row IPC round trips; cross-project search is one fan-out in Rust; resting projects hold no watcher.
- Code quality: clean, idiomatic, match the surrounding code's style, comment density and naming. Document public
  Rust items (docs become generated JSDoc for IPC types). Tests for what you build.

## Gates
`docs/redesign/build/gates.sh <gate>...` runs gates and prints PASS/FAIL (gates: `rust`, `ipc`, `js`, `addon`,
`parity`, `oracles`, `all`); logs in `$GATE_LOGS` (default `target/gate-logs`). Set `CARGO_TARGET_DIR` and
`GATE_LOGS` to your own dirs when you run it. Run the gates your brief names before you report; report their real
output. Never claim a gate passes without running it.

## Report
Finish with: what you changed (files), what you deleted, tests added, gate results (verbatim PASS/FAIL lines), any
deviation from the contract or design and why, and anything left undone.
