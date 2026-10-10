# Settings and scope

This document describes `.unfold/config.toml`, how Unfold discovers the files in a project's folder, which
folders it always skips, how `[scope]` and `languages` narrow what is graphed, and how settings are
fingerprinted for the per-file facts cache. It expands [architecture §9.2](architecture.md#92-project-settings),
[§7.7](architecture.md#77-settings-and-inputs), [U-16](decisions-and-tradeoffs.md#u-16-discovery-and-scope-without-running-git),
and [U-23](decisions-and-tradeoffs.md#u-23-settings-in-toml-in-three-places).

The code is `crates/unfold-engine/src/settings/` (the model, parsing, scope matching, hashing) and
`crates/unfold-engine/src/discovery/` (the walk, classification, the skipped-files report).

## `.unfold/config.toml`

The shared, committed settings of one project. The file is optional: an absent file means the
defaults, and the app writes it only when a setting differs from them. Editing it from the app while
preserving comments is the workspace's job ([How the app edits it](#how-the-app-edits-it)); this section
describes what the engine reads.

```toml
version = 1
languages = ["typescript", "python"]     # optional; default: every supported language found

[project]
name = "orders"                           # optional; the shared name other projects' boards use

[scope]
include = []                              # optional; when set, only these folders or globs are graphed
exclude = ["**/generated/**"]

[detectors]
disabled = ["node-cron.schedule"]

[[python.project]]
root = "py-orders"
source_roots = ["src", "legacy"]
version = "3.12"

[[entrance]]
kind = "event"
trigger = "SIGTERM"
handler = "src/server.ts#shutdown"
```

| Key | Type | Default | Rules |
| --- | --- | --- | --- |
| `version` | integer | `1` | Must be `1` |
| `languages` | array of strings | every language | `typescript` and/or `python`; nonempty; duplicates are ignored |
| `project.name` | string | from the folder's name | See [Project name](#project-name) |
| `scope.include` | array of globs | `[]` (everything) | See [Scope](#scope) |
| `scope.exclude` | array of globs | `[]` | See [Scope](#scope) |
| `detectors.disabled` | array of detector IDs | `[]` | Known IDs only ([graph model](graph-model.md)); `manual` can't be disabled |
| `python.project` | array of tables | none | See [Python projects](#python-projects) |
| `entrance` | array of tables | none | See [manual entrances](entrances.md#manual-entrances-and-locators) |

### Strictness and errors

Parsing is strict. Unknown keys anywhere (including inside `[project]`, `[scope]`, `[detectors]`, `[python]`,
`[[python.project]]`, and `[[entrance]]`), wrong types, malformed TOML, and invalid values are
errors. Every error names the file, a one-based line, and a one-based column counted in
characters, and carries the byte span of the offending key or value:

```text
.unfold/config.toml:4:15: the `manual` detector can't be disabled; remove the `[[entrance]]` tables instead
```

The file must be UTF-8; a leading byte-order mark is skipped (spans still count its bytes). Settings
errors are never partially applied: the project's compile reports the error instead.

### Project name

`[project] name` is the project's shared name: other projects' boards name this one by it, so it is
committed with the folder. It is 1 to 64 ASCII letters, digits, `.`, `_`, or `-`, starting with a letter
or digit (`/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/`); anything else is an error at the value.

Without it, the workspace uses a default made from a folder's name (for a repository's root, the main
worktree's folder, so every worktree of a repository shares it). A folder name that isn't a valid project
name is slugged the way a board's file name is (`My Repo` becomes `my-repo`, `Café` becomes `cafe`,
nothing left becomes `project`): `settings::default_project_name`. Setting the name to nothing from the
app removes the `[project]` table.

The name changes no compile, so it is in neither settings hash ([Settings hashing](#settings-hashing)).

### How the app edits it

The app changes the file only through `update_project_config` ([IPC](ipc.md#project-home-and-settings)): a list
of edits applied in order, all or nothing, that keep the file's comments and formatting and write it only when
its text changes. An edit that would make the file invalid is refused with the error (`invalid_settings`), and one that a broader rule stands in the way of is refused as a
conflict (`settings_conflict`). The edits come from:

- **Project settings** (a dialog from the project menu or Home): **General** sets or removes `[project] name`
  (an empty name means the default); **Scope** is a tree of the project's folders whose check boxes become
  `exclude` and `include` patterns (each a folder path, escaped as a glob) and whose rules list removes a pattern as
  written; **Detectors** turns detectors on and off; **Python projects** sets and removes `[[python.project]]`
  tables. Changes are staged and sent together with Save. Background updates, also in General, live in the
  project's `state.json`, not here.
- **The not-compiled Home's scope tree**, whose changes are kept in memory and written just before the first
  Compile, so nothing is written to a folder that was never compiled.
- **The manual entrance editor**, which adds, replaces, or removes one `[[entrance]]` table.

When the file doesn't parse, Project settings shows the error with Open in editor, and the edits that need a
valid file wait until it is fixed.

### Python projects

Each `[[python.project]]` table configures the Python files under its `root`:

| Key | Default | Rules |
| --- | --- | --- |
| `root` | `.` | A folder relative to the project's folder; no `..`, no leading `/`. Two tables can't share a root |
| `source_roots` | Python front end's default | Nonempty list of folders relative to `root`, joined onto it; no duplicates after normalization |
| `version` | `"3.14"` | `"3.12"`, `"3.13"`, or `"3.14"` (strings) |

Folder paths are normalized (`./src/` becomes `src`; `.` is the folder itself). A file belongs to the
project with the deepest root containing it (`Settings::python_project_for`). Without
`source_roots`, or without any project, the Python front end uses `src/` for a src layout and otherwise
the folder that holds `pyproject.toml` ([architecture §6.4](architecture.md#64-python)).

A source root may lie outside `[scope]`. Modules found there exist but aren't indexed, so imports and
console scripts that reach them resolve as `not_indexed` rather than `not_found`. The `py-orders`
playground uses exactly this: `legacy` is a source root and `legacy/**` is excluded.

When more than one source root (or a module and a package of the same name) supplies an imported module, the
import's edge is `ambiguous` and the import is listed in Problems, without making the graph incomplete
([Python](languages/python.md#entrance-detectors-and-problems)).

## Discovery

Discovery walks the project's folder with the `ignore` crate (ripgrep's walker). It never runs Git and
works the same in a plain folder as in a repository.

- **Ignore files.** `.gitignore` files at every level, `.ignore` files, `.git/info/exclude`, and the
  global Git excludes file (`core.excludesFile`, found by reading Git's config files) apply, with
  ripgrep's semantics. `.gitignore` applies even when the folder isn't a Git repository, and ignore
  files in folders above the project's folder apply too. Rare differences from Git's own rules are
  accepted (U-16): for example, when you open a folder that a parent repository ignores, Git would
  ignore everything in it while the walk does not.
- **Always excluded.** Folders named `node_modules`, `.venv`, `venv`, `__pycache__`, `.mypy_cache`,
  `.pytest_cache`, `.ruff_cache`, `.git`, or `.unfold` are never entered, at any depth. A nested
  folder's `.unfold/` is therefore never read when you open a parent folder. Other generated folders,
  such as `dist`, need an ignore rule or a scope exclusion.
- **Hidden files** are walked like any other (Git tracks them too).
- **Symlinks.** A symlinked folder is never followed (no cycles, no duplicates, no escapes). A
  symlinked file counts, under its own path, only when its target is a file inside the project's folder;
  its size and modification time are the target's. Dangling symlinks and symlinks to files outside
  the folder are skipped.
- **Names.** Paths are folder-relative, `/`-separated, and case-preserving, and they sort by their UTF-8
  bytes. A name that isn't UTF-8, or can't be such a path (it contains a backslash, say), is skipped
  and reported.
- **Unreadable entries** (a folder without permission, a malformed ignore file) become problems in the
  result; the walk carries on.
- **Speed.** The walk is parallel and stats only files that matter (sources, context files, unindexed
  source-like files, configuration inputs). Its output is sorted, so it doesn't depend on thread
  count. On the proof of concept's `compilers/typescript` checkout (82,000 files outside
  `node_modules`, 2,900 folders, 20,036 TypeScript sources) a warm walk takes about 150 ms on a
  10-core Mac, about 280 ms on one thread.

### What each file is

| Class | Files | Notes |
| --- | --- | --- |
| Source | `.ts`, `.tsx`, `.mts`, `.cts`; `.py` | Indexed when its language is on and it is in scope |
| Context | `.d.ts`, `.d.mts`, `.d.cts` | Read by TypeScript for declarations; never get blocks. Listed only when TypeScript is on, with their scope status |
| Unindexed | Sources out of scope or with their language off; JavaScript (`.js`, `.mjs`, `.cjs`, `.jsx`) | They exist, so references into them are `not_indexed`, not `not_found`. JavaScript comes in V2 |
| Input | `.unfold/config.toml` (at the folder root only), `tsconfig*.json`, `package.json`, `pyproject.toml` | Fingerprinted ([§7.7](architecture.md#77-settings-and-inputs)); each carries its scope status |
| Other | Everything else | Not tracked |

Classification is by file-name suffix, as in the proof of concept. `.pyi` stubs are not sources: the
proof of concept's Python front end reads only `.py` files, and stubs carry no runtime bodies.

Inputs carry their scope status rather than being dropped, because their consumers differ: the
`package.script`, `package.bin`, and `pyproject.script` detectors skip excluded manifests (as the proof
of concept did), while a file's nearest `tsconfig.json` applies regardless of scope. Which inputs a
compile actually fingerprints is decided by the languages and detectors that read them.

Only walked files are listed. Files under ignored or always-excluded folders aren't, so a resolver
that must tell `not_indexed` from `not_found` for such a path (a launch of a gitignored
`dist/legacy.js`, say) asks `exists_in_root`, which checks the filesystem without following a symlink
out of the folder.

Every file listed carries its size and modification time, the cheap check that decides whether a file
is hashed again on reopen or on a watcher event ([U-17](decisions-and-tradeoffs.md#u-17-freshness-verify-before-showing-cheap-checks-first)).

### Single paths

Watchers report individual paths. `IgnoreRules` answers "is this path skipped?" for one path with the
same rules as the walk (the always-excluded folders plus every applicable ignore file, read once and
cached per folder), and `classify` says what the path is under the settings. Rebuild `IgnoreRules`
after an ignore file changes.

### The skipped-files report

A project's not-compiled Home shows what was left out ("Skipped: node_modules, .venv, dist (from .gitignore
and defaults)"). Discovery reports it aggregated by folder:

| Reason | Meaning |
| --- | --- |
| `default` | An always-excluded folder (`.git` and `.unfold` are not listed: they are always there) |
| `ignored` | An ignore-file rule |
| `excluded` | A `[scope] exclude` pattern |
| `not_included` | Outside the `[scope] include` patterns |
| `language_off` | The language is left out of `languages` |
| `javascript` | JavaScript, which V1 doesn't graph |
| `symlink` | A symlinked folder, or a symlinked file that dangles or leaves the folder |
| `invalid_path` | A name that can't be a folder path |

Each line is a folder, a reason, whether the folder itself was skipped without being walked (then its
contents aren't counted), and the number of skipped source-like files. Skipped files are attributed
to the highest folder below the root that holds no indexed source, and then folded into an ancestor
line with the same reason, so `legacy/**` excluded in `py-orders` reports one line: `legacy`, excluded,
2 files. Ignored folders are found by comparing each walked folder's entries with what the walker
yielded.

## Scope

`[scope] include` and `exclude` are glob patterns relative to the project's folder. They apply on top of
the ignore files and the always-excluded folders.

- A pattern matches a path when it equals the path, names a folder that contains it (`docs` matches
  `docs/a.ts`), or matches it as a glob. A trailing `/` is allowed.
- Globs are `/`-aware: `*` and `?` stay inside one path component, `**` crosses components, `[…]` and
  `{a,b}` work as usual, and `\` escapes the next character. `src/*.ts` matches `src/a.ts` but not
  `src/deep/a.ts`; `**/generated/**` matches a `generated` folder at any depth.
- With no `include` patterns every file is included. With `include` patterns, only files they match
  are. Any matching `exclude` pattern wins over `include`.
- Patterns are checked when the settings load: an empty pattern, one starting with `/`, one with a `.`
  or `..` component, or invalid glob syntax is an error at that pattern's position.

Scope decides indexing only. Excluded sources still exist (`not_indexed`), and a Python source root
may be excluded.

## Settings hashing

The facts cache is keyed by path, content hash, engine version, and a settings hash
([architecture §6.1](architecture.md#61-pipeline)). Settings hashes are SHA-256 over a domain name
and length-prefixed canonical JSON of the settings values (fixed field order, sorted collections), so
they depend only on values: comments, whitespace, key order, and the byte positions of tables don't
matter.

- `Settings::hash()` covers everything that can change a compile: languages, scope, disabled
  detectors, Python projects, and manual entrances (without their byte spans). The project name is
  left out: renaming the project re-extracts nothing.
- Languages hash only what affects extracting one file, so a settings edit re-extracts only the files it
  touches. `Settings::python_hash(path)` covers the owning Python project (root, source roots, grammar
  version) or its absence: changing one project's grammar re-extracts that project's files only, and
  adding a manual entrance re-extracts nothing.
- `SettingsHasher` builds other hashes the same way (a domain plus serializable parts or raw bytes,
  such as a `tsconfig` chain's content hashes).

The settings file itself is also a fingerprinted input (role `unfold-config`), hashed by its bytes like
every other input.
