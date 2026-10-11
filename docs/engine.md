# Engine, scope, and entrances

Read the relevant section for updates, discovery, settings, or detectors. This guide owns
shared pipeline and project rules; graph semantics belong to [graph model](graph-model.md),
and language-specific extraction/binding to [TypeScript](languages/typescript.md) and
[Python](languages/python.md).

| Task | Code entry point |
| --- | --- |
| Update pipeline | [update/mod.rs](../crates/unfold-engine/src/update/mod.rs) |
| Discovery/classification | [discovery/mod.rs](../crates/unfold-engine/src/discovery/mod.rs) |
| Settings and hashes | [settings/mod.rs](../crates/unfold-engine/src/settings/mod.rs) |
| Launch-command parsing | [launch/mod.rs](../crates/unfold-engine/src/launch/mod.rs) |
| Manual declarations/locators | [entrances/mod.rs](../crates/unfold-engine/src/entrances/mod.rs), [locator.rs](../crates/unfold-engine/src/locator.rs) |
| Workspace/watcher | [workspace.rs](../crates/unfold-workspace/src/workspace.rs), [watcher.rs](../crates/unfold-workspace/src/watcher.rs) |
| Store commit/version handling | [store entry point](../crates/unfold-store/src/lib.rs), [schema.rs](../crates/unfold-store/src/schema.rs) |

## Updates

`Engine::update` is the single path for Compile, background changes, reopen checks, and
Rebuild. It discovers inputs, snapshots bytes, extracts per-file facts, links each language's
whole program, resolves/pairs/types, detects entrances, and validates the result.

| Action | Reads / cache behavior |
| --- | --- |
| Compile | Full discovery and content hashes; reuse facts with equal cache keys |
| Background | Apply changed paths; check size/mtime before hashing |
| Reopen | Restore stored graph and link basis, then check disk state in the background |
| Rebuild | Full discovery and hashes, ignoring caches and last-good facts |

A changed folder, ignore file, symlink, unreadable path, or changed settings can require full
discovery. Ignore `.unfold/` changes except shared config. Per-file extraction is cached;
linking, resolution, pairings, and type links are recomputed from the whole program. Avoid
promising dependency-level incremental relinking that the implementation does not provide.

`prepare` computes a candidate without replacing engine state. Workspace commits it first,
then `adopt` installs it. Adoption rejects a candidate prepared against an older generation.
A failure or cancellation retains the previous engine/store/reader state. Publish events
only for committed/adopted results; readers must not observe a partial transaction.

Parallel extraction preserves input order; graph assembly sorts deterministically. Long loops
check cancellation. Compile uses foreground capacity; background work has its own limited
pool. Progress, changes, problems, and completeness are separate outputs, not inferred from
the fact that a command returned.

## Facts cache and failures

The facts cache key includes path, content hash, engine version, and extraction settings.
Language payload versions participate in producer compatibility. Version constants live in
code; do not copy current numbers into guides.

Keep each file's current and last-good facts. A syntax/read failure may retain last-good facts
and report `last_good`; a file without usable facts is `failed`. The graph is incomplete,
and displayed source must still verify against the facts hash. Rebuild deliberately removes
last-good fallback. A failed update never advances the committed graph.

The SQLite cache is rebuilt when schema/engine compatibility fails or it is damaged; there
are no migrations. Deltas commit atomically, including projections and metadata. A writer
owns key/file-ID mirrors that must roll back with failed transactions. WAL readers see
committed snapshots; multi-query reads use a read transaction.

## Shared config

`.unfold/config.toml` is strict, UTF-8 TOML. Reject unknown keys, wrong types, invalid globs,
unsupported values, and invalid manual declarations with source positions; never apply a
partly valid config. A BOM is accepted while source spans retain its bytes.

```toml
version = 1
languages = ["typescript", "python"]

[project]
name = "orders"

[scope]
include = ["src"]
exclude = ["**/generated/**"]

[evidence]
files = ["schema.sql", "prisma/**"]

[detectors]
disabled = ["node-cron.schedule"]

[[python.project]]
root = "."
source_roots = ["src"]

[[entrance]]
kind = "command"
trigger = "report"
handler = "src/cli.ts#report"
```

Omitted config uses defaults: TypeScript/Python enabled, all discovered sources in scope,
all available detectors enabled, no manual entrances or evidence patterns. An explicit empty
language list is invalid. Known detector IDs only; `manual` cannot be disabled.

Shared project names are 1–64 ASCII letters/digits/`.`/`_`/`-`, starting with a letter/digit.
They identify cross-project board references. The default is derived from the main worktree's
folder; renaming a project does not re-extract source.

Python projects choose grammar and source roots; the deepest containing project owns a file.
Absent explicit roots, use `src/` for a src layout, otherwise the pyproject folder. A source
root can be excluded from indexing and still identify imports as `not_indexed`.

`update_project_config` applies edits together, preserves formatting/comments, validates
before writing, and refuses conflicts. Settings UI stages changes until Save. Background
update preferences are local state, not shared config.

## Discovery and scope

Discovery uses the `ignore` walker, without running Git. Respect nested `.gitignore`, `.ignore`,
Git excludes, and applicable parent rules. Hidden files are ordinary candidates. Always skip
`node_modules`, virtualenvs, Python tool caches, `.git`, and `.unfold`; generated directories
such as `dist` need ignore/scope rules. The exact excluded set is owned by discovery code.

Never follow directory symlinks. A file symlink is usable only when it resolves inside the
project. Invalid UTF-8 paths, unreadable entries, and malformed ignore rules are reported;
the walk continues. Results use normalized project-relative paths and deterministic order.

Scope applies on top of ignore rules. A pattern names a file, a containing folder, or a
`/`-aware glob; `*` stays within a component and `**` crosses components. An empty include
list means all; any exclude wins. Reject empty, absolute, dot-component, and invalid patterns.

Classification distinguishes indexed source, context/declaration files, configuration inputs,
evidence, unindexed source-like files, and irrelevant files. Scope changes indexing, not
existence: an excluded target is `not_indexed`, a missing target `not_found`. Skipped-file
reports explain classification without implying every discovered file is compiled.

Evidence patterns select files that are not already source/context/config inputs. They obey
ignore/always-excluded rules but not source scope. Hash them as `evidence` inputs without
parsing or executing them. Changed evidence bytes update anchors without relinking code.

Settings hashes depend on canonical values, not comments/order/positions. Extraction hashes
include only settings affecting that file; project names and manual entrances need no source
re-extraction. The config file is also fingerprinted by its actual bytes as an input.

## Launch-command grammar

Package scripts are interpreted statically, never executed. The accepted subset handles
literal words/quotes, supported sequential shell steps, assignments, `cd`, and script
references. Refuse expansions, substitutions, unsupported operators, shell control constructs,
and execution wrappers rather than guessing their effect. Preparatory commands do not supply
proven code targets; the final launch step decides the process entrance.

Recognize supported Node/TypeScript/Python file launchers and package script references.
Resolve a written emitted TypeScript path through the emit map before output exists. Guard
script-reference recursion, working directories, ignored lifecycle hooks, and project bounds.
A launch outside the project or through `node_modules` supplies no entrance. Exact grammar,
launcher flags, recursion limits, and examples are covered by the launch module and its tests.

## Manual entrances and locators

`[[entrance]]` supports `process`, `command`, `http`, `worker`, `schedule`, and `event`.
A process requires one `startup`; every other kind requires a locator or nonempty ordered
`handler` list. Handler lists cannot mix languages. Only HTTP may have a supported uppercase
`method`; an omitted trigger is unknown, an empty trigger invalid. Unknown keys are errors.

| Locator | Meaning |
| --- | --- |
| `path` | File root for a startup; unsupported as a handler |
| `path#name`, `path#default` | Runtime export/re-export first, otherwise supported top-level declaration |
| `path#Owner.member` | Direct member of a named class or constant object; no inherited lookup |
| `path:line` | Outermost permitted block starting on the one-based line |

A locator has at most one selector, confined path syntax, valid identifier names, and canonical
positive decimal lines. Parsing checks syntax; each compile resolves it again. Keep unresolved
entrances rather than silently deleting them. Missing/unindexed/ambiguous/external targets
remain explicit. Class handlers and external startups are unsupported.

Constant object-literal function members agree with ordinary call resolution where that call
has evidence. Annotated or mutable object cases may differ deliberately: a manual locator
selects a written direct member, while a call must prove its runtime binding. Line locators
follow line numbers after edits; prefer names. Evidence is the config table's exact range and
`/entrance/<index>` pointer.

## Entrance detectors

Detection requires proven framework identity, not lookalike names. Follow supported stable
origins through imports/aliases/factories/annotations. An uncertain trigger or handler keeps
its declared entrance with explicit status; hidden mutations/registrations may invalidate
handler body proof. `exact`, `partial`, and `unknown` describe written trigger knowledge.

| Detector | Recognizes | Key limits |
| --- | --- | --- |
| `package.script` | Supported launch in the final step of a package script | Ignore lifecycle hooks; literal launches, emit-map resolution, bounded script references |
| `package.bin` | String/object bins in strict package JSON | Named nonempty paths; package-relative startup resolution |
| `express.route` | HTTP method registrations on proven Express app/router | Supported literal mounts; handler order preserved; incomplete mount paths are partial |
| `bullmq.worker` | Proven Worker construction | Written queue and processor; unsupported options/wrappers may prevent body proof |
| `node-cron.schedule` | Proven cron schedule registration | Literal schedule text; shared handler policy |
| `pyproject.script` | Python project console-script declarations | Static module/member resolution through configured roots |
| `fastapi.route` | Supported decorators/applied factories on proven app/router | Declared dependencies before endpoint; uncertain Depends/mounts stay explicit |
| `celery.task` | Supported task decorators/applied factories on proven Celery app | Queue is trigger, not task name; shared registry/configuration uncertainty refuses body proof |
| `apscheduler.job` | APScheduler 3 BlockingScheduler `add_job` | Exact cron only from proven five-field `from_crontab`; mutation/hidden registration invalidates body proof |
| `manual` | Valid shared declarations | Never disabled; resolve locators on every compile |

Express/FastAPI mount traversal is bounded (five levels; FastAPI also limits branch expansion).
Cycles, uncertain prefixes, and unmounted routers retain partial local paths rather than
inventing complete routes. Framework-specific dependency, mutation, and argument rules live
with each detector, alongside its entrance tests and `testdata/detectors/<detector>/` cases.

Inspect [TypeScript detectors](../crates/unfold-lang-typescript/src/detect/mod.rs) or
[Python detectors](../crates/unfold-lang-python/src/detect/mod.rs) when changing a detector;
read only the relevant implementation and fixtures. Mixed projects compile languages
independently; a detector never resolves a handler by guessing across language boundaries.

## Project lifecycle

[Projects](../crates/unfold-workspace/src/projects.rs) owns the machine-wide workspace.
A connected project starts resting. Listing/searching resting projects reads committed data
without starting an engine or watcher. Using graph content wakes the project and extends its
interest lease. Window selection/open boards and active requests contribute interest.

A live [Workspace](../crates/unfold-workspace/src/workspace.rs) owns its lock, engine thread,
writer, reader cache, watcher, update scheduling, status, and local layout. First Compile
creates local files; merely connecting a never-compiled folder writes nothing there.

The watcher batches changes, cancels superseded work, and supports pause/resume. Unsupported
watching falls back to scans; locks prevent conflicting writers. Rest releases engines,
watchers, locks, and in-memory programs after interest expires. Reopen restores committed
facts before checking disk. Source verification still protects reads while checks run.

Read-only folders and unsafe network volumes use the OS-cache fallback. Shared config and
published board writes still require a writable project. Crash/cancel paths retain the last
committed graph and expose current state/problems; they must not disguise stale source as
fresh. IPC distinguishes commands that wake projects from resting reads.
