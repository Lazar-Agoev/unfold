# Engine, settings, and entrances

Read the section for updates, the facts cache, `unfold.toml`, entrances, discovery, or the project lifecycle. Graph
semantics belong to [Graph model](graph-model.md); language extraction and binding to
[TypeScript](languages/typescript.md) and [Python](languages/python.md).

| Task | Code entry point |
| --- | --- |
| Update pipeline | [update/mod.rs](../crates/unfold-engine/src/update/mod.rs) |
| Facts cache | [update/cache.rs](../crates/unfold-engine/src/update/cache.rs), [facts/mod.rs](../crates/unfold-engine/src/facts/mod.rs) |
| Settings parsing and hashes | [settings/mod.rs](../crates/unfold-engine/src/settings/mod.rs), [settings/apps.rs](../crates/unfold-engine/src/settings/apps.rs) |
| Settings edits | [settings/edit/mod.rs](../crates/unfold-engine/src/settings/edit/mod.rs) |
| Resolving entrances | [entrances.rs](../crates/unfold-engine/src/entrances.rs) |
| Discovery and scope | [discovery/mod.rs](../crates/unfold-engine/src/discovery/mod.rs), [settings/scope.rs](../crates/unfold-engine/src/settings/scope.rs) |
| Resolution policy, labels, pairing | [resolve/](../crates/unfold-engine/src/resolve/mod.rs), [labels.rs](../crates/unfold-engine/src/labels.rs), [pairing.rs](../crates/unfold-engine/src/pairing.rs) |
| Projects, engine thread, watcher | [lib.rs](../crates/unfold-workspace/src/lib.rs), [projects.rs](../crates/unfold-workspace/src/projects.rs), [engine_thread.rs](../crates/unfold-workspace/src/engine_thread.rs), [watcher.rs](../crates/unfold-workspace/src/watcher.rs) |
| Store commits | [commit.rs](../crates/unfold-workspace/src/commit.rs), [store entry point](../crates/unfold-store/src/lib.rs), [schema.rs](../crates/unfold-store/src/schema.rs) |

## Updates

`Engine::update` is the single path that keeps one folder's graph current. Each language implements two traits
([language.rs](../crates/unfold-engine/src/language.rs)): `Language` for per-file extraction (lowering to nodes,
unresolved links, typed records, and an opaque binder payload), and `Linked` for the questions the shared backend
asks about the whole program. A language answers narrow questions; the backend owns the policy.

| Action | Request | What it reads |
| --- | --- | --- |
| Compile (first or later) | `UpdateRequest::compile` | Full discovery; every file re-read and re-hashed; only changed bytes extracted |
| Background update | `UpdateRequest::background(paths)` | The watcher's paths, checked by size and modification time first |
| Reopen check | `Engine::restore`, then `UpdateRequest::reopen` | Every file by size and modification time; hashes only those that differ |
| Entrance edit | `Engine::resolve_entrances` | Only the `[[app]]` tables, resolved again against the current graph |

An update runs its stages in order: **discover** (settings, then a full discovery or the changed paths),
**snapshot** (read and hash changed files once), **extract** (per-file facts through the facts cache, in parallel),
**link** (each language over the whole program), **resolve** (every link's outcome and every argument's pairing),
and **validate** (the core validator and each language's census). After a successful update the apps of
`unfold.toml` are resolved against the new graph ([Resolving `code`](#resolving-code)).

- **Full discovery** runs for a Compile, the first update, a settings change, or a changed path that can't be applied
  on its own: an ignore file, a folder, or a symlink.
- **Relinking.** A Compile always relinks the whole program. A background update relinks unless the files' facts
  and everything else linking reads are unchanged: the effective settings, the configuration inputs, and the
  unindexed and context files. The bytes of `unfold.toml`, its comments, its `[[app]]` tables, and evidence files
  are not among them, so editing those never relinks. Per-file extraction is cached; linking, resolution, pairing,
  and typed links are always recomputed from the whole program, never per dependency.
- **Prepare, commit, adopt.** `Engine::prepare` computes a candidate beside the engine's state; the workspace commits
  it to the store in one transaction, then `Engine::adopt` installs it. Adoption refuses a candidate prepared against
  an older state. A failure or cancellation keeps the previous engine, store, and reader state. Events go out only
  for committed results.
- **Determinism.** Parallel extraction keeps input order and assembly sorts, so thread scheduling never changes the
  graph. An incremental update must equal a fresh compile.
- **Threads.** A Compile uses every core but one; background work has its own small pool. Long loops check
  cancellation. Progress, changes, problems, and completeness are separate outputs.

## Facts cache and failures

Facts are cached per file by path, content hash, `ENGINE_VERSION`, and the file's settings hash (only the settings
that affect that file, such as its Python project). Each language hashes its payload version into its settings hash,
so a new payload format misses the cache. Version constants live in code; never copy their values into docs.

Each path keeps at most a current and a last good entry. A file that stops parsing keeps its last good facts and
is `last_good`; a file with no usable facts is `failed`, and the graph is incomplete. Last-good facts refer to their
own hash, and displayed source must still verify against it. A failed update never advances the committed graph.
The cache is an accelerator: a read error, an entry that doesn't decode, or one whose payload its language can't read
(`Language::reads_payload`) is a miss, and a failed write is a problem, never a failed update.

The store is a cache too ([schema.rs](../crates/unfold-store/src/schema.rs)): a database with another schema or
engine version, or a damaged one, is discarded and rebuilt on open. There are no migrations. Deltas commit in one
transaction; one writer per folder, on the engine thread; readers see committed snapshots through write-ahead
logging.

## Shared config

`unfold.toml` at the project's root is the project's one settings file, shared through git and edited by people and
by the app. It is strict UTF-8 TOML: unknown tables and keys, wrong types, and invalid values are errors with a line,
column, and span. A BOM is accepted, and spans still count it. An absent file means the defaults.

```toml
languages = ["typescript", "python"]

[scope]
include = ["src"]
exclude = ["**/generated/**"]

[[python.project]]
root = "."
source_roots = ["src"]
version = "3.13"

[evidence]
files = ["schema.sql", "prisma/**"]

[[app]]
name = "Shop API"
run  = "uvicorn shop.api:app --port 8000"
code = "shop/api.py#"
entrances = [
  { name = "Place an order", on = "POST /orders/{order_id}", code = "shop/api.py#create_order" },
]
```

| Table | Fields | Default |
| --- | --- | --- |
| `languages` | `typescript`, `python` | Both; an empty list is invalid |
| `[scope]` | `include`, `exclude` glob lists | Everything discovered |
| `[[python.project]]` | `root`, `source_roots`, `version` (`3.12`, `3.13`, `3.14`) | None; roots must be distinct |
| `[evidence]` | `files` glob list | None |
| `[[app]]` | `name`, `run`, `code`, `about`, `entrances` | None: no entrances |

The compile settings (`languages`, `[scope]`, `[[python.project]]`, `[evidence]`) must all be valid, or the file is
invalid. **A file that doesn't parse keeps the last good settings and apps** in effect, shows the error, and marks
the graph incomplete. The text of the last good file is stored with the graph, so reopening keeps it too, and an
engine that starts over in a recreated `local/cache/` keeps the settings it had.

`[[python.project]]` picks the grammar and source roots; the deepest project containing a file owns it. Source roots
join onto `root`, and one may lie outside `[scope]`: its modules then resolve as `not_indexed`. Defaults are in
[Python › Imports and source roots](languages/python.md#imports-and-source-roots).

`[evidence] files` names files boards may cite, such as SQL migrations or a Prisma schema. They are hashed as
`evidence` inputs, never parsed or linked; ignore files and the default exclusions apply to them, `[scope]` doesn't.
A changed evidence file updates its hash without relinking code.

`Settings::hash` covers the compile settings' values, not comments, order, positions, or `[[app]]` tables.
Extraction hashes only the settings that affect each file.

## Entrances

Entrances are declared in `[[app]]` tables, never detected. No `[[app]]` tables means no entrances. Unfold never runs
or parses `run` or `on`, and entrances never change how code compiles.

### The `[[app]]` tables

An **app** is something a person runs: a service, a worker, a command-line tool.

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | yes | Its heading on Home; its key is unique in the file |
| `run` | no | The command that starts it, shown as written |
| `code` | no | Where that command starts, usually a module: `shop/api.py#` |
| `about` | no | One sentence for its card |
| `entrances` | no | What it handles, in order: a list of inline tables |

An **entrance** is something an app handles: a route, a queue, a schedule, a signal.

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | yes | What a person calls it; its key is unique within its app |
| `on` | no | What starts it, as written: `POST /orders/{order_id}`, `queue emails`, `every 5 minutes`, `SIGTERM` |
| `code` | yes | The block key it starts at, or a list of keys in the order they run |
| `about` | no | One sentence for its card |

An app can be only a heading (no `run` or `code`) or only a start (no `entrances`). There are no kinds: `on` says
what starts an entrance. `code` is any block key a board could use that names a start: a module (`shop/cli.py#`), a
definition (`src/repo.ts#Repo.save`), or an inline handler
(`src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]`). To start at one branch of a dispatcher,
name the function that branch calls. A key that contains `"` is written as a single-quoted TOML string. The
playgrounds' `unfold.toml` files are full examples (`testdata/playgrounds/`).

### App and entrance keys

| Points at | Key |
| --- | --- |
| An app's start | `entrance:shop-api` |
| An entrance | `entrance:shop-api/place-an-order` |

Each name is lowercased, every run of characters other than letters and digits becomes `-`, and leading and
trailing `-` are dropped: `"1. Collect incoming email"` is `1-collect-incoming-email`. A name with no letters or
digits makes no key. Boards and agents use these keys; renaming an entrance or moving it to another app makes a
board's reference missing, and the board then offers the entrance with the same `code`.

### Entry problems

One bad app or entrance marks only itself, with its span, and stays out of the graph; the rest of the file works.
An app's problem keeps its entrances out too. The problems are an unknown field, a wrong type, a missing `name`, an
entrance with no `code` (or an empty list), a name that makes no key or an empty `code` key, and a duplicate key.
The first entry without a problem claims a key; a later one with the same key is the duplicate, and an entry that
already has a problem claims nothing, so it never knocks out a valid one. The `entrances` command lists every entry
as written, with its problem.

### Resolving `code`

After every update, and when only the `[[app]]` tables change, each `code` key is looked up in the graph:

| State | When |
| --- | --- |
| `found` | The key names a module, definition, or inline function |
| `missing` | No block has the key, or it isn't a block key |
| `not_analyzed` | The key's file exists but isn't compiled: scope or ignore rules exclude it, or Unfold doesn't index its kind of file |
| `not_a_start` | The key names a call, exit, control block, or part |
| `stale` | The key names a start in a file that stopped parsing: its last good block |

**Fixes are offered, never applied,** and only when exactly one candidate fits a `missing` key:

| Fix | Candidate |
| --- | --- |
| Moved definition | The one definition in another file with the same qualified name |
| Edited route | For an inline handler of a call, the one other call under the same parent to the same callee, with a handler in the same place, that no entrance names |
| Moved module | The one module in another file with the same file name and the same top-level definitions the missing module had when last seen |

Otherwise the person chooses the code again. An entrance never silently opens something else.

When only `[[app]]` tables change, `Engine::resolve_entrances` re-reads `unfold.toml` and resolves the apps against
the current graph without extracting or linking; the workspace then sends only `entrances_changed`, when anything
changed. If the compile
settings changed too, it refuses (`CompileNeeded`) and an update runs instead.

## Settings edits

[settings/edit/](../crates/unfold-engine/src/settings/edit/mod.rs) changes `unfold.toml` through `toml_edit`, so
everything an edit doesn't touch (comments, blank lines, key order, quoting, a BOM) stays byte for byte, and **a
write changes only its own lines**. The result is parsed with the strict parser; an edit that would make the file
invalid, or leave the entry it sets with an empty key, is refused with the parser's message and position, and so is
any edit of a file that doesn't parse. `apply_edits` writes nothing: it returns the new text, what it holds, a
unified diff, and what each app and entrance edit did (added, changed, or removed).

| Edit | Effect |
| --- | --- |
| Exclude from graph, include in graph, graph only this folder, remove a scope pattern | Change `[scope]`; a broader `exclude` than the path is a conflict, reported, never rewritten |
| Set or remove a Python project | Adds or replaces the `[[python.project]]` table with that root, or removes it |
| Add or remove an evidence pattern | Changes `[evidence] files`; an emptied list's key goes, and so does an emptied table |
| `set_app` | Adds an `[[app]]` table, or sets or removes an existing app's `run`, `code`, and `about`; its entrances stay |
| `set_entrance` | Adds one line to an app's `entrances`, creating the app when it is new, or changes the entrance addressed |
| `add_entrance` | Adds one line, refusing when the app already has an entrance with that key |
| `remove_entrance` | Removes one entrance's line, or a whole app |

Apps and entrances are addressed by key, or by a name that makes that key: `"shop api"` addresses the app named
`"Shop API"`. An edit never renames: a changed entry keeps its name, and a new one takes the name given. A field left
out keeps what the file says; null removes it.

The workspace applies edits under one lock and writes the file atomically when it changes, never creating a file
that would hold only defaults. An edit that leaves the compile settings as they were resolves the entrances again
without compiling; any other edit is picked up by the next background update.

## Discovery and scope

Discovery walks the folder with the `ignore` crate, without running Git and whether or not it is a repository:
nested `.gitignore`, `.ignore`, `.git/info/exclude`, the global Git excludes file, and applicable parent rules apply.
Hidden files are ordinary candidates. Folders named `node_modules`, `.venv`, `venv`, `__pycache__`, the Python tool
caches, `.git`, and `.unfold` are never entered ([classify.rs](../crates/unfold-engine/src/discovery/classify.rs));
generated folders such as `dist` need ignore or scope rules.

Directory symlinks are never followed; a file symlink counts only when it resolves to a file inside the project.
Invalid UTF-8 paths, unreadable entries, and malformed ignore rules are reported, and the walk continues. Results
are normalized project-relative paths in sorted order.

Classification is by path alone:

| Class | Files |
| --- | --- |
| Source | `.ts`, `.tsx`, `.mts`, `.cts`, `.py` in scope, for an enabled language |
| Context | TypeScript declaration files (`.d.ts`, `.d.mts`, `.d.cts`): read for names, never given nodes |
| Configuration input | `unfold.toml` (at the root), `package.json`, `pyproject.toml`, `tsconfig*.json`, whatever the scope says |
| Evidence input | A file matching `[evidence] files` that is nothing else |
| Unindexed | A source-like file that exists but isn't indexed: excluded, not included, its language off, or JavaScript (`.js`, `.mjs`, `.cjs`, `.jsx`) |

Scope applies on top of ignore rules. A pattern names a file, a folder that contains it, or a `/`-aware glob (`*`
and `?` stay within one component, `**` crosses components). No `include` patterns means everything; any `exclude`
wins. Empty, absolute, `.`/`..`, and invalid patterns are rejected. Scope changes indexing, not existence: a call
into an excluded file is `not_indexed`, into a missing one `not_found`.

## Project lifecycle

[Projects](../crates/unfold-workspace/src/projects.rs) owns the machine-wide workspace (`workspace.json` in app
data); a live [Workspace](../crates/unfold-workspace/src/workspace.rs) owns one project's lock, engine thread, store
writer, reader cache, watcher, and update scheduling.

1. **Added.** `add_project` records the folder and compiles it at once. A folder Unfold can't write `.unfold/` into
   is refused: read-only, on a network volume (where SQLite's write-ahead log is unsafe), or with `.unfold/` behind a
   symlink.
2. **Woken.** A command that uses the project's engine, selecting it on Home, or opening one of its boards opens its
   Workspace. A folder compiled before takes its lock (a second copy of Unfold gets `open_elsewhere`), opens its
   store, and is readable at once; the engine thread restores the stored graph and runs the reopen check in the
   background. When nothing the graph was linked from changed while the folder was closed, the check neither
   relinks nor commits. Opening writes nothing into a folder that was never compiled.
3. **First Compile** creates `.unfold/local/` (with its `.gitignore`), takes the lock, opens the store, compiles, and
   commits.
4. **Background updates.** The watcher feeds changed paths to the scheduler; an update starts after a short quiet
   period, and a newer change cancels and restarts it. Without usable file-system events the folder is scanned
   periodically. If `local/cache/` disappears, the next piece of work recreates it and compiles everything again.
5. **Commits** write only what changed, in one transaction, install a new reader index, and send `graph_updated`,
   `problems_changed`, `project_state`, and `entrances_changed` when the apps changed.
6. **Rest.** Once nothing keeps the project live (no open board, no Home selection, no running compile) and its
   lease runs out, it remembers its row state and closes: no watcher, engine thread, or lock. Commands that read
   from disk serve it from its stored graph, read-only, without waking it.

Crash and cancel paths keep the last committed graph and show current state and problems; stale source is never
shown as fresh, because reads verify source against the facts hash. IPC tells commands that wake a project from
resting reads ([IPC](ipc.md)).
