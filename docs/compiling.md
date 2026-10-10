# Compiling and updates

How `crates/unfold-engine` turns a folder into a graph and keeps it current
([architecture §4.3, §6.1, §7](architecture.md#7-compiling-and-keeping-the-graph-current); decisions
[U-13](decisions-and-tradeoffs.md#u-13-compile-once-then-update-in-the-background),
[U-14](decisions-and-tradeoffs.md#u-14-what-gets-recompiled-per-file-facts-in-v1-early-cutoff-in-v2),
[U-15](decisions-and-tradeoffs.md#u-15-a-file-that-stops-parsing-keeps-its-last-good-facts),
[U-17](decisions-and-tradeoffs.md#u-17-freshness-verify-before-showing-cheap-checks-first)). The code is
`crates/unfold-engine/src/update/`. The engine reads files and writes nothing; the workspace commits the
result to the store.

## One entry point

An `Engine` is the in-memory program of one live project's folder: every file's facts and disk state, the input
fingerprints, the effective settings, and the committed graph. Languages are registered as
`Arc<dyn Language>`; the facts cache is an `Arc<dyn FactsCache>`. Every action is one call to
`Engine::update(request)`:

| Action | Request | Changes | Checked by | Threads |
| --- | --- | --- | --- | --- |
| First Compile | `UpdateRequest::compile()` | Everything (every file is new) | Contents | All cores but one |
| Compile | `UpdateRequest::compile()` | Everything | Contents (re-hash every file) | All cores but one |
| Background update | `UpdateRequest::background(paths)` | The watcher's paths | Size and modification time, then contents | Background pool (2) |
| Reopen check | `Engine::restore(graph, disks, basis)`, then `UpdateRequest::reopen()` | Everything | Size and modification time, then contents | Background pool |
| Rebuild from scratch | `UpdateRequest::rebuild()` | Everything, without cache or last-good facts | Contents | All cores but one |

Because every action shares this code path, a graph built by many updates equals the graph a fresh
compile builds from the same effective inputs. Tests check exactly that.

A successful update returns an `UpdateOutcome`: the previous and new graphs (`Arc<Graph>`), a
`ChangeSummary`, `UpdateStats`, problems, and whether the graph is complete. The engine's state is replaced
only then.

**Prepare, then adopt.** `Engine::update` is two steps that callers can also take apart.
`Engine::prepare(request)` runs the whole update and returns a `PreparedUpdate` (its `outcome()`, plus the
settings and disk states the store needs) **without** changing the engine. `Engine::adopt(prepared)` then
installs its state and returns the outcome. The workspace commits the prepared outcome to the store first and
adopts it only after the commit succeeds, so a commit that fails or is cancelled leaves the engine, the store,
and readers on the same previous graph. Each adopted state bumps a generation; adopting an update prepared
against an older state fails with `UpdateError::Superseded` and changes nothing.

## Stages

| Stage | Runs over | Does |
| --- | --- | --- |
| Discover | The settings file, then the folder or the changed paths | Loads `.unfold/config.toml` when it may have changed; runs a full discovery, or applies the changed paths to the previous one |
| Snapshot | Files and inputs that may have changed | Reads each once, hashes it (SHA-256), and decodes it strictly as UTF-8, in parallel |
| Extract | Files whose bytes or settings hash changed | Facts from the cache by key, or from the language front end, in parallel |
| Link | The whole program | Each enabled language links its files from cached facts, with the project inputs |
| Resolve | Every call site, construction, import, and reference | The shared policy, in parallel, one `Resolver` per language over the shared `FactsSet` |
| Detect | Manual entrances, then each language's detectors | Candidates, then shared assembly: target resolution, canonical order, keys |
| Validate | The new graph | The core validator plus each language's census |

Parallel work is reassembled by input index, files are in path order, and languages run in language order,
so nothing depends on thread scheduling or the order files arrive in.

**Changed paths.** A background update applies each path to the previous discovery: a deleted path removes
that file or every file under that folder; an existing file is checked against the ignore rules and
classified by the settings. A change that can alter other files' scope falls back to a full discovery: an
ignore file (`.gitignore`, `.ignore`, `.git/info/exclude`), a folder, a symlink, a path that can't be
examined, or settings that changed in any way. Paths under `.unfold/` other than the settings file are
ignored.

**Which files are read.** A file is read when it is new, when its settings hash changed, or when it may have
changed and the check says so: Compile and Rebuild read every file; background updates and the reopen check
compare size and modification time with the last check first (U-17). A file read with the same bytes and
settings hash as before keeps its facts; nothing is extracted.

**Link-time decisions.** Two facts about a file depend on other files, so extraction can't settle them
and every link decides them again (nothing about them is cached):

- *Invocations.* Every `CallFact` records an edge kind and a target context. A language may override both
  for one call with `Linked::invocation` (the default keeps the fact's). Python uses it to make a call whose
  callee has proven class identity a `constructs` edge resolved in the construct context (a direct concrete
  `__init__`, else the class), while the block stays a `call`. Validation still checks that the edge kind
  fits the block kind for the file's language.
- *Conditional import blocks.* Extraction may select `import` blocks tentatively and mark them conditional
  (`GraphFacts::with_conditional`; each must be a leaf `import` block with its own `ImportFact`). Python
  does this for every name of a `from` import that could load a child module. After resolution, the build
  stage asks the owning language which it keeps (`Linked::keeps_import`, default: none) and, only for
  files that have unkept ones, drops them and renumbers the file's blocks, parents, keys (with the core
  `KeyBuilder`), slots, facts, and edge sources (`GraphFacts::drop_conditional`). Resolution runs over the
  facts as extracted: dropping a leaf `import` block only changes the keys of later `import` siblings
  under the same parent, and `import` blocks are never edge or entrance targets, so every target key stays
  valid. The language's census lists only kept imports, and `Linked::module_dependencies` answers for
  every import fact, kept or not. The facts cache stores the extracted facts, candidates included.

**No-op updates.** When a background update changes nothing that shapes the graph (no file's facts, file
set, unindexed set, input, or setting), the previous graph is kept as is and the link stages don't run.
Compile and Rebuild always relink.

## The facts cache

`FactsCache` stores serialized `FileFacts` (postcard) keyed by path, content hash, engine version
(`ENGINE_VERSION`), and the file's settings hash, with a status: parsed, or failed (the failure itself). For
each path it keeps the **current** entry and, when that one failed, the **last good** one: putting a parsed
entry drops every other entry for its path; putting a failed entry keeps the parsed one. The store's
`file_facts` table has the same rules; `MemoryFactsCache` implements them in memory for tests and headless
tools.

- Lookups are batched per update. A decoded entry is re-checked (`FileFacts::from_bytes`); one that
  doesn't decode is a miss.
- Freshly extracted facts and failures are put in one batch after the extract stage, even if the update is
  later cancelled or fails: entries are content-addressed, so they can only help.
- A failing cache is reported as a problem (`ProblemKind::Cache`); the update goes on without it.
- A front end that panics on a file fails that file ("internal error in …"), and the failure isn't cached.
- Reverting a file to earlier bytes extracts it again unless those bytes are its last good entry; the cache
  is not a history.

## Files that don't parse (U-15)

| Before | Now | Result |
| --- | --- | --- |
| Current or last good, same settings hash | Doesn't parse, or can't be read | `last_good`: the last good facts and their hash, with the new failure text |
| No facts in memory | Doesn't parse | `last_good` from the cache's last good entry for this path and settings hash, if any; otherwise `failed` |
| Any | Parses | `current` |

A `last_good` file keeps its blocks, slots, and edges; references to it still resolve. A `failed` file has no
blocks, and imports or calls into it are `not_indexed`. The first compile never aborts on files that don't
parse. Last-good facts are kept only under the settings they were extracted with. Rebuild from scratch
ignores the cache and memory, so a file that doesn't parse then becomes `failed`, and once the rebuilt graph is
committed its cached last-good facts are forgotten ([Workspace lifecycle](#workspace-lifecycle)).

The graph is **complete** when no file is `failed`, no input is unreadable, discovery read the whole folder,
and the settings are valid. Unreadable files (permissions, invalid UTF-8, more than 4 GiB) are failures like
syntax errors.

**Errors fail the update.** A language breaking its contract with the backend (`UpdateError::Backend`), a
validation failure (`UpdateError::Validation`, every violation listed), a discovery error on the folder
itself, cancellation, or adopting a stale prepared update (`UpdateError::Superseded`) leave the previous graph, facts, settings, and fingerprints untouched.

## Settings and inputs

`.unfold/config.toml` is read whenever it may have changed. If it can't be read or is invalid, the previous
valid settings stay in effect (the defaults on a first compile) and a `ProblemKind::Settings` problem is
reported. When the settings change in any way, the update runs a full discovery and recomputes every file's
settings hash; exactly the files whose hash changed are read and extracted again (for example, changing one
Python project's grammar touches that project's files only, and adding a manual entrance touches none).

Configuration inputs (`tsconfig*.json`, `package.json`, `pyproject.toml`) are found by discovery and
fingerprinted like sources. Languages get all of them, with the settings file, in `ProjectInputs`, along with
the unindexed paths, the context files discovery found (TypeScript declaration files, whatever their scope;
adding or removing one relinks), and the folder root (`ProjectInputs::exists` answers for paths discovery
doesn't walk). A language may ask for more before linking (`Language::requested_inputs`, such as a
`tsconfig` `extends` target under `node_modules`); the engine reads them and asks again, for up to 16
rounds. Any input change relinks.

Inputs are read before sources, because a file's settings hash can depend on them: the engine asks each
language for the extraction options of its files in one batch (`Language::extraction_options`, which defaults
to `settings_hash`), uses each result's hash as the file's settings hash, and hands the options back to
`Language::extract_with`. Options are recomputed for every file when the settings or any input changed, and
otherwise only for new files and the files being extracted; exactly the files whose hash changed are
extracted again (a `tsconfig.json` edit touches the files that use it). Problems a language finds in its
inputs (`Language::input_problems`, such as an `extends` naming a package that isn't installed) are listed as
input problems and make the graph incomplete; they never fail the update.

The graph's `inputs` are those that shaped it: the settings file when the settings came from it, what each
language reports through `Linked::inputs_read`, and every input an entrance cites as evidence.

## Validation

The structural, cross-file, entrance, and census checks always cover the whole graph. Checks against bytes
need snapshots: when every current file was read in this update (and none is `last_good`), the whole graph is
validated against them in one pass. Otherwise the files read now and every input are checked against their
snapshots in a second pass, and files that weren't read keep facts that were checked against their bytes
when they were extracted. Last-good facts are never checked against the current bytes.

## Cancellation and threads

A `CancelToken` is checked between stages and before every file read, extraction, and occurrence
resolution, so a cancellation is acknowledged within one item's work (well under 250 ms). A cancelled update
returns `UpdateError::Cancelled` and changes nothing.

Cancellation reaches past the engine into the store commit, but only for explicit work:

| Work | Cancelled while the engine runs | Cancelled while committing |
| --- | --- | --- |
| Compile, Rebuild from scratch | Stops; nothing changes | Stops: `Writer::commit_unless` / `replace_unless` ask between files and once more before `COMMIT`, then roll back with `StoreError::Cancelled`; nothing changes |
| Background update, scan, reopen check | Stops; its paths go back in the queue | Not cancelled: once a commit starts it finishes ([architecture §7.5](architecture.md#75-triggers-batching-and-cancellation)), so a stream of edits can't starve it |

An explicit Compile or Rebuild can therefore be cancelled until its commit finishes. A cancelled commit rolls
back, the engine never adopts the prepared update, and the folder keeps the graph, facts, and status it had
(fresh facts already put in the cache stay, which only helps). Measured on the 602-file corpus (release
build), a cancel is acknowledged in 18–133 ms ([testing](testing.md#budgets-measured)).

Compile and Rebuild run on a pool of every core but one; background updates on a small pool (two threads by
default). Both are configurable in `EngineOptions`, and every engine thread has a 16 MiB stack as defense in
depth against deeply nested syntax (front ends also guard their recursion). Thread priority is left to the
operating system.

## Progress, statistics, and the change summary

- **Progress.** `UpdateRequest::with_progress` reports `Progress { stage, done, total }` from any engine
  thread through the snapshot and extract stages, about a hundred times per stage ("Compiling · 12 of 84
  files").
- **Statistics.** `UpdateStats` is the Index log entry: files by language and state, files read, taken from
  the cache, extracted, and removed, inputs read, time per stage, and counts by resolution, reason, entrance
  kind, detector, and target resolution. Its `Display` is the log text.
- **Change summary.** `ChangeSummary` names the files added or changed, the files removed, the block keys
  added, removed, or changed (block, slots, or outgoing edges), the entrance keys added, removed, or changed,
  and whether the inputs changed. It drives `graph_updated`. It is not the store's delta: the workspace builds
  the store's `Update` from the outcome's previous and new graphs with `FileChange::between` and
  `EntranceChanges::between`, using `Engine::disk_state` for each file's disk stamp.

## Reopening a folder

`Engine::restore(graph, disks, basis)` rebuilds the in-memory program from the stored graph and the cache
without parsing: each file's facts come back by the hash they came from and its current settings hash. Files
the cache can't supply are left for the next update. `UpdateRequest::reopen()` then discovers the folder and
hashes only files whose size or modification time differs. It relinks only when something changed: a file's
facts, the file set, or what the graph was linked from. That is the `LinkBasis` every update reports
(`UpdateOutcome::basis`) and the store keeps with the graph: a fingerprint of the engine and front-end
versions, the folder, the effective settings and the settings file, every configuration input, and the
unindexed and context files, with the problems linking found. A graph restored without its basis is relinked
once. When nothing changed, the restored graph and its link problems are kept, and the workspace commits
nothing (every disk stamp and the metadata are already stored). When the engine version changed, nothing
matches and the reopen check recompiles in the background.

## Determinism

The canonical export is identical under shuffled file order, any thread count, and any sequence of updates
that ends with the same effective inputs. "Effective" includes last-good facts: a file that doesn't parse
contributes the facts of its last good version, which a fresh compile sharing the same cache sees too. The
tests in `crates/unfold-engine/tests/backend/update/` check this with the alien language, including a property
test of random edit sequences (write, break, fix, delete files and folders, change settings) against fresh
compiles.

## Workspace lifecycle

`crates/unfold-workspace` drives the engine for each live project and persists what it produces
([architecture §7–8](architecture.md#7-compiling-and-keeping-the-graph-current)). Its module documentation
is the detailed reference; this is the outline.

**Opening.** The projects registry opens a project's `Workspace` when the project wakes, and closes it when
the project rests ([architecture §11.2](architecture.md#112-live-and-resting-projects)). Every window shares
it: one engine thread, one store writer, one watcher. Opening a folder that was never compiled writes
nothing. A folder compiled before (its `local/` exists) takes the folder lock in `local/cache/lock` — a second
copy of Unfold gets `open_elsewhere` — and opens `graph.sqlite`, whose graph is readable at once. The engine
thread then loads the whole stored graph (`Reader::graph`) and its link basis, calls `Engine::restore`, and
runs `UpdateRequest::reopen()` in the background; a check that changed nothing the store holds isn't
committed. A database the store recreated (engine or front-end version
changed, damaged, or deleted) is compiled again in the background with a fresh engine.

**The first Compile** creates `.unfold/local/` with its `.gitignore` (`*`), `cache/`, and `drafts/`, then
`state.json`; nothing else in `.unfold/` appears until a setting differs from the defaults or a board is
published. A folder that refuses the write (read-only), or sits on a network volume (detected with `statfs`),
or where SQLite refuses write-ahead logging, keeps `local/` in `<fallbackCacheDir>/<hash of the path>/`
instead, and the project's Home says so.

**Every update** runs on the engine thread: `Engine::prepare`, then one store transaction built from the
prepared outcome's previous and new graphs (`FileChange::between`, `EntranceChanges::between`, disk stamps from
`Engine::disk_state`, reader projections from `unfold_reader::project`, and the graph metadata), then a fresh
`ReaderIndex::from_graph` in the `ReaderCache` (only after `Engine::adopt`, which runs once the transaction
commits), then events: `graph_updated` (changed files, keys, and
entrances; everything after a full commit), `problems_changed`, and `project_state` when the project's state
changes. The facts cache is the
store's `file_facts` table, through the same writer behind a mutex. The store's version string combines
`ENGINE_VERSION` with every front end's producer version, so a front-end change also starts the cache over.

**Background updates.** `notify` with `notify-debouncer-full` watches the folder; events pass discovery's
ignore rules, `.git/` and `.unfold/` (other than `config.toml`, `boards/`, and `local/drafts/`) are dropped.
An update starts about 300 ms after the last change; a newer change cancels an update that is still
preparing, merges the paths, and restarts it. A background update that has started committing finishes its
commit (architecture §7.5); the newer change then runs as the next update. Background updates use the engine's small pool; an explicit Compile uses
every core but one. Event overflow asks for a full check; an inotify limit, a watcher that can't start, or a
network volume switches the folder to a periodic scan (`UpdateRequest::reopen()` every few seconds), reported
as "watching by scanning". Background updates can be paused per project (`state.json`); a paused project
counts the paths changed since the last update.

**Recovery.** If `local/cache/` disappears while the folder is open (the watcher notices, and every piece of
work checks), the engine thread recreates it, takes the lock again, and compiles everything with a fresh
engine. A failed or cancelled commit rolls back inside its transaction: the store still holds the previous
graph, the engine doesn't adopt the update, and the next commit is a delta against that graph as before.

**Rebuild from scratch** runs `UpdateRequest::rebuild()`, which reads no cached facts and no last-good facts,
and then **replaces** the stored graph inside the commit (`Writer::replace_unless`) instead of clearing the
store first. Only after that commit succeeds does it drop the cached facts of the files that are now
`failed` (`Writer::forget_facts`), so their last good versions can't come back later; every other file's
entry was already replaced by the fresh one. A rebuild that is cancelled or fails therefore keeps the old
graph and its facts, last-good facts included.

**Closing.** A project rests once nothing needs it, its lease has run out, and its engine is idle; it also
closes when it is removed from the workspace, switched to another worktree, or the app quits. Its last state is
written to `workspace.json`, the engine thread stops, the watcher stops, the lock is released, and the
project's command-side state (the file list, rendered files, board sessions) is dropped. Windows save their
boards before releasing them, so nothing unsaved lives in a closing workspace.
