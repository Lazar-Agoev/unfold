# Entrances

This document describes how Unfold finds entrances: the declared ways a process or unit of work
starts ([architecture §6.6](architecture.md#66-entrance-detectors)). The entrance record itself
(kinds, triggers, evidence, target roles, ordering) is defined in [the graph model](graph-model.md).

Each detector gets its own section here, written alongside the detector. This document covers the
two pieces every detector shares or builds on: the launch-command grammar that the `package.script`
detector uses to read `package.json` scripts, and manual entrances with their locators; then the
[TypeScript detectors](#typescript-detectors) (`package.script`, `package.bin`, `express.route`,
`bullmq.worker`, `node-cron.schedule`), the [Python detectors](#python-detectors) (`pyproject.script`,
`fastapi.route`, `celery.task`, `apscheduler.job`), [mixed repositories](#mixed-repositories), and the
[decisions](#decisions) taken while building them.

Both pieces carry the proof of concept's rules over unchanged (its `docs/compiler.md`, sections
"Manual declarations and locators" and "Launch-command grammar"). The code is
`crates/unfold-engine/src/launch/` and `crates/unfold-engine/src/locator.rs`, plus the `[[entrance]]`
parsing in `crates/unfold-engine/src/settings/`.

## Launch-command grammar

Package scripts are interpreted from their JSON text. Nothing is executed: no shell, package
manager, build, or emitted file runs, and emitted files never need to exist. A script whose last step
launches a module becomes a `process` entrance with one `startup` target; a script outside the
grammar produces a debug note and no entrance; a script that is understood but launches nothing
(`tsc -p .`, `vite`, `node --test`) produces nothing.

`interpret_script(packages, root, package, name)` returns one of:

| Outcome | Meaning |
| --- | --- |
| `Launch { path, cwd }` | The last step launches `path`, written relative to the directory `cwd` |
| `NoLaunch` | The script is understood, but its last step launches no module |
| `Unsupported(reason)` | Something in the script is outside the grammar; `reason` is the proof of concept's message |

`path` is the decoded word as written (it becomes the startup target's text). `cwd` is a
`/`-separated directory: normally folder-relative (`.`, `apps/api`), but `cd` can make it absolute or
lead outside the folder. Turning the pair into a file is the backend's job: it rejects paths outside the
folder or through `node_modules`, checks indexed files, then the TypeScript emit map
(`dist/server.js` → `src/server.ts`), then probes extensions before choosing between `not_indexed`
and `not_found` (which fixes PoC defect C-5).

### Shell subset

- **Steps** are separated by `&&` or `;`. An empty step (`; node a.js`, `a;; b`) and a trailing `&&`
  are refused; one trailing `;` is fine. Only spaces and tabs separate words outside quotes.
- **Quoting.** Single quotes keep text literally. Double quotes allow escaping `"`, `\`, `$`, backtick,
  and newline; any other backslash stays literal. An unquoted backslash escapes the next character;
  backslash-newline disappears without creating a word. A quoted empty word (`""`) is still a word.
- **Refused anywhere in a script, including in ignored arguments:** unquoted control characters and
  newlines, NUL, unterminated quotes or escapes, comments (an unquoted word starting with `#`),
  `$(…)` and backtick substitutions, the operators `|`, `&`, `(`, `)`, `<`, `>`, and the non-POSIX
  `$'…'` and `$"…"` quoting. Inside single quotes these characters are literal text.
- **Expansions.** A word containing an unquoted or double-quoted `$NAME`/`${…}`/`$1`-style parameter,
  an unquoted glob character (`*`, `?`, `[`), or a leading unquoted `~` is refused when it is used as a
  command, option, option value, directory, script name, or module path. Leading assignment values
  and ignored program arguments are not interpreted, so they may contain expansions.
- **Assignments.** Leading words whose *written, unquoted* prefix is `[A-Za-z_][A-Za-z0-9_]*=` are
  assignments and are skipped: `FOO="x" node a.js` runs `node`, but `"FOO=x" node a.js` runs a
  command named `FOO=x`. One `env` or `cross-env` wrapper skips assignment arguments, including
  quoted ones. Wrapper options (`env -i`) and wrapped `cd` or builtins are refused; nested wrappers are
  not interpreted further. A step of only assignments launches nothing.
- **Control words** written plainly are refused: `if then else elif fi for while until do done case esac
  in function select time ! { }`.
- **Builtins** are refused even when quoted: `exec eval source . command builtin exit return break
  continue trap pushd popd alias unalias enable`.
- **`cd`** takes exactly one literal, nonempty directory not starting with `-`, relative or absolute. It
  changes only the interpreter's working directory for later steps of the same script.
- Any other command (`tsc`, `vite`, `rm`) is a preparatory step whose effects are not modeled.

### Script references

| Command | Script interpreted |
| --- | --- |
| `npm run name`, `npm run-script name` | `name` in the invoking package |
| `npm start`, `npm stop`, `npm restart`, `npm test` | The script of that name in the invoking package |
| `npm --prefix dir run name` | `name` in `dir/package.json`, with `dir` relative to the current directory or absolute |
| `yarn [run] name`, `pnpm [run] name` | `name` in the invoking package |
| `bun run name` | `name` in the invoking package (a script reference, not a file launch) |

- A changed directory does not select another package; only `--prefix` does, and `--prefix` requires
  `run`. Other npm options are refused (`npm --verbose run x`).
- The referenced package must already be a selected configuration input (`PackageScripts` answers
  for exactly those), and the script must be an own, string-valued member of its `scripts` object.
  Script names may be anything, including `""`, `__proto__`, and `x~/y`.
- Arguments after a reference must start with `--` and are ignored; a script name starting with `-` is
  refused.
- A referenced script starts in its own package directory; its `cd` does not change the caller's.
- Nesting starts at depth 0 and accepts eight references; depth nine is refused. An active
  `(package, script)` pair seen again is a cycle and is refused. One enumerated script interprets at most
  1024 references in all ("Script references exceed 1024"), an Unfold bound: depth and cycle checks alone
  don't bound fan-out, so a small `package.json` could otherwise take exponential time.
- Lifecycle hooks (`prestart`, `postbuild`, …) are never run or expanded by a reference. (The
  `package.script` detector separately skips enumerating lifecycle scripts and `pre`/`post` hooks.)

### The last step decides

Each step replaces the result: `npm run build && node dist/server.js` launches `dist/server.js`;
`node main.js && vite` launches nothing; `node main.js; cd src` launches nothing.

### File launchers

`node`, `tsx` (with one optional `watch` word), `ts-node`, `ts-node-esm`, `nodemon`, and `bun`.
Options are scanned until the first operand; `--` ends options. A known value-taking option consumes
the next word unless written `--name=value`; every other accepted option consumes nothing. The
first operand is the module path; later program arguments are ignored. A bare `-` or no operand
launches nothing; an empty operand (`node ""`) is refused.

| Launcher | Value-taking options |
| --- | --- |
| `node`, `tsx` | `-r --require --import --loader --experimental-loader --env-file -C --conditions --input-type --inspect-port --title` |
| `tsx` additionally | `--tsconfig` |
| `nodemon` | `--watch -w --ext -e --delay --signal` |
| `ts-node`, `ts-node-esm`, `bun` | none |

Special cases: `node --test` launches nothing; `node inspect` is refused; `tsx --test`, `--ignore`,
`--include`, and `--exclude` are refused; `bun` tool subcommands launch nothing (`test build install
add remove update outdated link unlink pm x create init upgrade repl publish check info patch
patch-commit audit help completions`); `bun` options followed by `run` are refused.

Known flags outside the grammar are refused rather than letting their operands become module
paths; unknown flags are treated as operand-free. Flags with `=value` are checked by name. The
refused sets are exact:

- **`node` and `tsx`:** `-e --eval -p --print -c --check -i --interactive -h --help -v --version
  --entry-url --run --build-sea --build-snapshot --build-snapshot-config --completion-bash
  --v8-options --prof-process --allow-fs-read --allow-fs-write --cpu-prof-dir --cpu-prof-interval
  --cpu-prof-name --diagnostic-dir --disable-proto --disable-warning --dns-result-order
  --env-file-if-exists --experimental-config-file --experimental-default-config-file
  --experimental-package-map --experimental-sea-config --experimental-test-tag-filter --heap-prof-dir
  --heap-prof-interval --heap-prof-name --heapsnapshot-near-heap-limit --heapsnapshot-signal
  --icu-data-dir --debug-port --inspect-publish-uid --localstorage-file --max-http-header-size
  --max-old-space-size --max-old-space-size-percentage --network-family-autoselection-attempt-timeout
  --openssl-config --redirect-warnings --report-directory --report-dir --report-filename
  --report-signal --secure-heap --secure-heap-min --snapshot-blob --test-concurrency
  --test-coverage-branches --test-coverage-exclude --test-coverage-functions --test-coverage-include
  --test-coverage-lines --test-global-setup --experimental-test-isolation --test-isolation
  --test-name-pattern --test-random-seed --test-reporter --test-reporter-destination
  --test-rerun-failures --test-shard --test-skip-pattern --test-timeout --tls-cipher-list --tls-keylog
  --trace-event-categories --trace-event-file-pattern --trace-require-module --unhandled-rejections
  --use-largepages --v8-pool-size --watch-kill-signal --watch-path`. Underscore spellings that
  normalize to one of these, or to a hyphenated value-taking flag (`--inspect_port`), are refused too.
- **`ts-node` and `ts-node-esm`:** `-e --eval -p --print -i --interactive -h --help -v -vv -vvv --version
  --showConfig -P --project -O --compilerOptions -C --compiler --transpiler -r --require --cwd
  --scopeDir -D --ignoreDiagnostics -I --ignore --experimentalSpecifierResolution`, plus the
  kebab-case spellings of the camelCase names (`--show-config`, `--compiler-options`, `--scope-dir`,
  `--ignore-diagnostics`, `--experimental-specifier-resolution`).
- **`nodemon`:** the `node` set and all of Node's value-taking options, except nodemon's own options
  (so `-e` keeps its extension-list meaning), plus `--test --exec -x --cwd --config --ignore -i
  --polling-interval -P -d -s --on-change-only -C --dump -?`. The underscore rule applies unless the
  exact spelling is a nodemon option.
- **`bun`:** `-e --eval -p --print -h --help -v --version --interactive --workspaces --parallel
  --sequential --elide-lines --filter -F --shell --title --unhandled-rejections --console-depth
  --watch-kill-signal --inspect --inspect-wait --inspect-brk --preload -r --require --import --install
  --conditions --main-fields --extension-order --tsconfig-override --define -d --drop --loader -l
  --jsx-factory --jsx-fragment --jsx-import-source --jsx-runtime --port --fetch-preconnect
  --max-http-header-size --dns-result-order --user-agent --env-file --cwd --config -c`.
- **Attached short values** are refused: `-e…`/`-p…` for every launcher except nodemon; `-r…`/`-C…` for
  `node` and `tsx`; `-P… -O… -C… -D… -I… -r…` for the ts-node variants; `-w… -e… -x… -i… -d… -s…
  -P… -r… -C…` for nodemon.

### Examples (from `testdata/playgrounds/ts-orders/package.json`)

| Script | Text | Outcome |
| --- | --- | --- |
| `start` | `node dist/server.js` | Launch `dist/server.js` from `.` (the emit map leads to `src/server.ts`) |
| `dev` | `tsx watch src/server.ts` | Launch `src/server.ts` |
| `worker` | `NODE_ENV=production tsx src/worker.ts` | Launch `src/worker.ts` |
| `scheduler` | `cd src && tsx scheduler.ts` | Launch `scheduler.ts` from `src` |
| `legacy:export` | `node scripts/export.js` | Launch `scripts/export.js` (an existing, unindexed JavaScript file: `not_indexed`) |
| `build` | `tsc -p .` | No launch |
| `test` | `node --test` | No launch |
| `dev:all` | `npm run dev & npm run worker` | Unsupported: `Unsupported shell operator &` |

## Manual entrances and locators

`[[entrance]]` tables in `.unfold/config.toml` declare entrances the detectors can't see, such as CLI
dispatch or a signal handler. They are the `manual` detector, which can't be disabled.

```toml
[[entrance]]
kind = "command"
trigger = "report"
handler = "src/cli.ts:12"

[[entrance]]
kind = "http"
method = "POST"
label = "Create order"
handler = ["src/auth.ts#requireUser", "src/routes/orders.ts#createOrder"]

[[entrance]]
kind = "process"
startup = "src/server.ts"
```

### Declarations

Validated when the settings load; any violation makes the whole settings file invalid, with its line
and column ([settings and scope](settings-and-scope.md)).

| Key | Rule |
| --- | --- |
| `kind` | Required: `process`, `command`, `http`, `worker`, `schedule`, or `event` |
| `trigger` | Optional; omitted means unknown. An empty string is invalid |
| `method` | Only on `http`: `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, or `ALL` (uppercase) |
| `label` | Optional display label; may be empty |
| `handler` | Every kind except `process`: one locator or a nonempty list, in order (middleware first). One entrance's handlers can't mix TypeScript and Python files |
| `startup` | `process` only, and required there: one locator |

Unknown keys are rejected. Each table produces one manual entrance, even when its locators resolve
to nothing: a well-formed locator that names nothing keeps the entrance with an unresolved target.

**Evidence.** Path `.unfold/config.toml`; pointer `/entrance/<index>`, the zero-based position of the
table among the file's `[[entrance]]` tables; and a byte range from the `[[` of the table's header
through the end of its last value, token-exact (no trailing comment or whitespace). Comments between
the header and the last value are inside the range. `ManualEntrance` exposes `index`, `pointer()`,
`span`, and the header's `line`. (In the proof of concept the evidence was `codegraph.json` with pointer
`/entrypoints/<index>`; the reference importer maps one to the other.)

### Locator grammar

| Form | Selects |
| --- | --- |
| `path` | The file itself: a startup's module root |
| `path#name` | An exported name (exports and re-exports first), otherwise a top-level declaration |
| `path#default` | The default export |
| `path#Owner.member` | A direct member of a named class or constant object literal |
| `path:line` | The outermost permitted block starting on that one-based line |

- `path` is a folder-relative `/`-separated path: nonempty, no empty, `.`, or `..` component, no
  leading `/`, no backslash, no NUL, and no drive prefix (a locator starting `C:` is invalid).
- A locator has at most one selector: the text contains at most one `#` or `:` in total, so a path can't
  contain either character.
- Names are JavaScript identifier names: `$`, `_`, or a Unicode `ID_Start` character, followed by `$`,
  ZWNJ, ZWJ, or `ID_Continue` characters. Python identifiers are a subset. A member selector has exactly
  two names (`#a.b.c` is invalid). Private names and string, number, or computed names are outside the
  grammar.
- Lines are canonical positive decimals: no sign, no zero, no leading zero, at most 2^53 − 1.

`Locator::parse` checks syntax only and keeps the text as written, which is the target's text.
Resolution against current declarations happens on every compile in the backend
(`crates/unfold-engine/src/entrances/`), following the proof of concept's table:

| Selector | Resolution |
| --- | --- |
| Bare path | The indexed file for a startup; `unsupported` for a handler |
| Name or `default` | Runtime exports and re-exports first, then a unique supported top-level declaration; a constant function, arrow, or class initializer supplies its concrete block |
| `Owner.member` | A direct member of a named class or constant object literal, including direct function or arrow property initializers; no inherited members |
| `:line` | Permitted blocks starting on that line (file for a startup; function, method, `if`, `case`, `default` for a handler); one outermost resolves, several are `ambiguous`; another block there gives `unsupported`, none gives `not_found` |

An existing but unindexed file gives `not_indexed`, a missing one `not_found`. A handler ending at an
external package declaration can be `external`; an external startup is `unsupported`. A class handler
is `unsupported`. Line selectors don't follow declarations after edits; prefer a named selector.

## TypeScript detectors

The five TypeScript detectors carry over the proof of concept's rules (its `docs/compiler.md`, sections "Launch-command
grammar", "Package bin", "Provenance", "Express routes", "BullMQ workers", and "Node-cron schedules") with their exact
bounds. The code is `crates/unfold-lang-typescript/src/detect/`; `Linked::detectors` returns them in the order
`package.script`, `package.bin`, `bullmq.worker`, `express.route`, `node-cron.schedule` (detection order only breaks
ties between otherwise equal entrances), and each can be turned off with `[detectors] disabled`. Every case below is
pinned by a folder under `testdata/detectors/<detector>/` (sources, `tsconfig.json`, `.unfold/config.toml`, and an
`expected-entrances.txt`), which `crates/unfold-lang-typescript/tests/entrances.rs` compiles one by one through the
engine; the same test checks the `ts-orders` playground (25 entrances, 28 targets), `[detectors] disabled`, evidence
ranges and pointers, unreadable packages, and determinism.

Detectors only add entrances: blocks, slots, and edges are the same with them on or off.

### Framework provenance

The source detectors recognize framework objects only through the engine's bounded import provenance
(`DetectContext::provenance`, [the graph model](graph-model.md), "Provenance"): an external package import, a
property step, a unique non-ambient `const` initialized by a call or construction (`()`), or a written annotation
(`:type`), at most three steps. A local lookalike, a `let` factory, a `const` alias of a non-call, a parameter that
shadows the import, a namespace member where a named export is required, a repository module that a `tsconfig.json`
`paths` alias names after the package, and a re-export from an excluded file all prove nothing. A detector runs its
call scan only when some indexed file imports its package (by module label), since every identity it recognizes
starts at such an import.

Linking never sees syntax, so extraction records what the detectors need beyond the value table, per call site
(`payload::Site`): decoded string and no-substitution template literals (through parentheses, `!`, `as`,
`satisfies`, and `<T>`) in the first two argument positions, a route registration's array literals flattened with
their holes and spreads (each element gets a value of its own), and a construction's second argument as written when
it is itself a `new`. Only `new` expressions, calls of a name, and calls of a member named `get`, `head`, `post`,
`put`, `patch`, `delete`, `options`, `all`, `use`, or `schedule` are considered, and a site is recorded only when it
has something beyond its values.

**Handlers** go through the shared handler policy (aliases, members, inline functions, `wrapper` for a call result,
`unsupported` for a class, `external` for a package import, `mutable_binding`, `parameter`, `dynamic`, ...). **Spreads.**
When a spread occupies the trigger position, every explicit handler after it is `unsupported` (its runtime position is
uncertain) with its written text, and the trigger is unknown; a sole spread reveals no handler, so there is no
entrance. A registration with no explicit handler produces no entrance.

### `package.script`

```json
{ "scripts": { "start": "node dist/server.js", "scheduler": "cd src && tsx scheduler.ts" } }
```

- **Inputs.** Every discovered `package.json` outside `node_modules` that scope doesn't exclude is read when
  `package.script` or `package.bin` is enabled, and fingerprinted even when it declares nothing (with both disabled,
  none is read). A package that isn't strict JSON (comments, trailing commas, single quotes, control characters) or
  isn't an object is listed in the Problems view and makes the graph incomplete; the rest of the graph is kept.
- **Enumeration.** Each own member of `scripts`, in written order, except the lifecycle scripts (`preinstall`,
  `install`, `postinstall`, `prepublish`, `preprepare`, `prepare`, `postprepare`, `prepublishOnly`, `prepack`,
  `postpack`, `dependencies`) and `pre<name>`/`post<name>` when `<name>` is an own member (`prefix` stays when there
  is no `fix`). A non-string value, or a `scripts` value that isn't an object, produces nothing.
- **Interpretation.** The [launch-command grammar](#launch-command-grammar) from the package's folder, with every
  selected package available to `npm --prefix`. A script whose last step launches a module is one `process`
  entrance; an unsupported or launch-free script produces nothing.
- **Trigger:** the script name as written (an empty name is unknown).
- **Target:** one `startup` target, the launch path as written (`dist/server.js`), resolved by the backend: an
  indexed file's root block; else the TypeScript emit map (`dist/server.js` → `src/server.ts`, before any output
  exists; an output two sources emit is `ambiguous`); else, for a path without an extension, the probes `x.ts`,
  `x.tsx`, `x.mts`, `x.cts`, `x.js`, `x.mjs`, `x.cjs`, then `x/index.*` through the same lookups; else `not_indexed`
  when the file or any probe exists (C-5 fixed: `node scripts/export` naming an unindexed `scripts/export.js`),
  `not_found` otherwise.
- **Refusals:** a launch that leaves the folder, names the folder itself, or passes through `node_modules`, or whose
  path or directory has NUL, a backslash, or a drive prefix, produces no entrance.
- **Evidence:** the `package.json`, the exact byte range of the member (from its key's opening quote to the end of
  its value, whitespace around the colon included; offsets count a byte-order mark; a duplicate key's last member),
  and the RFC 6901 pointer `/scripts/<name>` (`~` written `~0`, `/` written `~1`; `__proto__` and `""` are ordinary
  names).

### `package.bin`

```json
{ "name": "@acme/orders", "bin": "dist/cli.js" }
{ "bin": { "orders": "dist/cli.js" } }
```

The same inputs, target resolution, refusals, and evidence rules as `package.script`; paths are relative to the
package's folder.

- **String `bin`:** one entrance named after the package's string `name`, without its scope (`@acme/orders` →
  `orders`; `@acme` without a slash stays whole); pointer `/bin`. A missing or non-string name, or an empty path,
  produces nothing.
- **Object `bin`:** one entrance per member with a nonempty string path, named after its key; pointer
  `/bin/<key>`. Other members, and any other `bin` value (an array, a number), produce nothing.
- An empty name or key is an unknown trigger. A command name implies nothing about the code it runs.

### `express.route`

```ts
const app = express();
app.get("/health", healthCheck);
app.use("/orders", ordersRouter);                  // ordersRouter = express.Router()
ordersRouter.post("/", requireUser, validateBody, createOrder);
```

`get`, `head`, `post`, `put`, `patch`, `delete`, `options`, and `all` (the method, uppercased) called on a recognized
receiver:

| Receiver provenance | Receiver |
| --- | --- |
| `express` default, `()` (`express()`) | Application |
| `Express` or `Application`, `:type` | Application |
| `Router`, `()` or `:type` (`Router()`, `new Router()`, `router: Router`) | Router |
| `express` default, `Router` `()` (`express.Router()`) | Router |

- **Mounts.** `parent.use(router)` and `parent.use(path, router)` on a recognized parent, with exactly one or two
  arguments and no leading spread, mount a router whose name denotes one indexed, non-ambient `const` initialized by
  a call or construction with router-factory provenance, also across files (imports and re-exports). An
  annotation-only router, a `let`, an array, `use(handler)`, and extra middleware arguments mount nothing.
- **Trigger.** An application route's literal path is exact. A router route follows each mount branch, in
  registration order and duplicates kept (each branch is its own entrance), to an application, at most five mounts
  deep; the literal prefixes and the local path are joined with empty parts dropped, repeated slashes collapsed, and
  one trailing slash removed (an empty result is `/`). An unmounted router, a cycle, a sixth mount, a dynamic prefix,
  or a branch that never reaches an application keeps the local path as written, `partial`. A route whose mounts fan
  out into more than 256 branches (an Unfold bound against multiplying entrances; the proof of concept has none)
  gets one entrance whose trigger is its local path, `partial`. A non-literal or empty local path is unknown on
  every branch.
- **Targets:** the arguments after the path in written order, with array literals flattened (nested ones too): each
  through the handler policy, except a spread (`unsupported`, text `(…)`) and a hole (`dynamic`, text `(…)`).
  Middleware come first, as written; a wrapper (`asyncHandler(getOrder)`) is `wrapper`.
- **Evidence:** the route call. `use` calls produce no entrance of their own.
- **Not recognized:** a computed method (`app["get"]`), `route(...).get(...)` chains, `param`, receivers reached
  through arbitrary fields or element accesses, and other packages' routers.

### `bullmq.worker`

```ts
import { Worker } from "bullmq";
new Worker("orders", processOrder, { connection });
```

- **Recognized:** a construction whose constructor's provenance is exactly `bullmq`'s named `Worker` (aliases and
  transparent wrappers included). A namespace `bullmq.Worker`, a default import, a local `Worker` class, `Queue`,
  `QueueEvents`, `FlowProducer`, a call without `new`, and a `const` alias are not.
- **Trigger:** the first argument's literal (an escaped name is decoded); a non-literal or empty queue is unknown.
- **Target:** the second argument, required, as one handler: a function or reference through the handler policy; a
  string or template (a sandboxed processor file) stays `unsupported` with its decoded text; a `new` whose constructor
  provenance is exactly `node:url`'s named `URL` stays `unsupported` with its whole written text (parentheses and
  comments included), its path never evaluated. A local `URL` class goes through the handler policy (`dynamic`).
- **Evidence:** the construction.

### `node-cron.schedule`

```ts
import cron, { schedule } from "node-cron";
cron.schedule("0 2 * * *", cleanupExpiredOrders);
schedule("*/15 * * * *", () => reportMetrics());
```

- **Recognized:** a call (not a construction) whose callee's provenance is `node-cron`'s named `schedule` (any local
  alias) or `schedule` read from its default or namespace import. A literal-key or computed access, other exports
  (`createTask`), calling the default or namespace itself, a local `schedule`, other packages, and a `const` alias are
  not.
- **Trigger:** the first argument's literal; a non-literal or empty expression is unknown.
- **Target:** the second argument, required, as one handler through the handler policy (a string is `dynamic`).
- **Evidence:** the call.

### Differences from the proof of concept

- **Problems instead of failures.** The proof of concept failed the whole build on a `package.json` that wasn't JSON
  or wasn't an object; Unfold lists it in the Problems view, marks the graph incomplete, and keeps everything else.
- **No debug notes.** The proof of concept wrote a debug note for each unsupported script and malformed `scripts`
  or `bin` value. Unfold has no note channel; those produce no entrance and nothing else.
- **Which packages.** The proof of concept read the Git-visible `package.json` files (or, from an explicit file
  list, those above each TypeScript source) minus its excludes. Unfold reads every discovered one that scope doesn't
  exclude; a `package.json` that only fails `include` patterns is still read, as the proof of concept's includes only
  added candidates.
- **C-5 fixed.** An extensionless launch whose probed file exists but isn't indexed is `not_indexed`, not
  `not_found`.
- **Lone surrogates.** An escaped lone surrogate in a `package.json` string (`"\ud800"`), which a Rust string can't
  hold, decodes to U+FFFD.

## Python detectors

The four Python detectors carry over the proof of concept's rules (its `docs/compiler.md`, sections "Python
console scripts", "Python framework provenance", "FastAPI routes", "Celery tasks", and "APScheduler jobs") with
their exact bounds. The code is `crates/unfold-lang-python/src/detect/`; `Linked::detectors` returns them in this
order, and each can be turned off with `[detectors] disabled` (a disabled detector reads nothing, so a disabled
`pyproject.script` doesn't fingerprint `pyproject.toml` either). Every case below is pinned by a folder under
`testdata/detectors/<detector>/` (sources, `.unfold/config.toml`, and an `expected-entrances.txt`), which
`crates/unfold-lang-python/tests/entrances.rs` compiles one by one.

Detectors only add entrances: blocks, slots, and edges are the same with any detector on or off.

### Framework provenance

The source detectors recognize framework objects by **proven imported identity**, the binder's external origin of
an expression (`fastapi.FastAPI`, `celery.app.base.Celery`, `apscheduler.schedulers.blocking.BlockingScheduler`),
never by spelling:

- a **receiver** is a call of a framework class (`FastAPI()`), or a name or attribute proven to be one stable
  declaration initialized by such a call, through aliases, re-exports, and module attributes
  (`from exports import application as app`), or a parameter or variable whose written annotation has that
  identity (**annotation-only**: it proves an API but never a concrete object, so it can't be mounted, can't
  prove a task or job body, and becomes uncertain when any indexed class subclasses the framework class);
- a local class named `FastAPI`, a rebound import (`FastAPI = other`), a subclass (`class Custom(FastAPI)`), a
  module of the repository that shadows the package (`src/fastapi.py`), a rebound receiver (`app = other`), and a
  module holding a proven `exec`/`eval` prove nothing.

A **negative census** (`detect/origins/`) then withholds that proof whenever anything could replace the API. It is
deliberately broader than recognition and follows every possible write, ambiguous and rebound ones included, so
an unknown star import or `exec` can't hide a known write:

- attribute and item writes on anything that may be the receiver (`app.get = fake`, `alias.get = fake`,
  `del alias.get`, `app.router = fake`, `app.__dict__['get'] = fake`, `delegated = app.router; delegated.get = ...`,
  tuple targets, `for app.get in ...`, `with ... as app.get`, augmented assignment), through stored fields
  (`box.api = app; box.api.task = fake`) and literal containers;
- builtin `setattr`/`delattr` by identity (an alias such as `from builtins import setattr as write` counts; a
  local function named `setattr` does not), with expanded arguments tainting everything they hold;
- `dict` mutators by identity (`dict.update(app.__dict__, ...)`) and mutating methods (`update`, `__setitem__`,
  `__delitem__`, `setdefault`, `pop`, `popitem`, `clear`, `__ior__`) called on, or through aliases of, anything
  that may be the API (`state = app.__dict__; mutate = state.update; mutate(...)`).

A write that reaches a proven receiver invalidates that receiver; one that reaches the framework class itself
(`FastAPI.get = fake`, `from fastapi.routing import APIRouter as R; R.get = fake`) or an annotation-only receiver
invalidates the whole family. The census only withholds; when one of its walks runs out of depth (64 nested
steps), every family is withheld.

**Handlers.** Expression handlers go through the shared handler policy (aliases, members, lambdas, `wrapper`,
metaclass and override refusals). A registration decorator's own `def` (and, for FastAPI, an applied reference
to a route-decorated `def`) keeps its original body only when every decorator of the function and of every class
around it is a proven transparent one (`staticmethod`, `classmethod`, `functools.cache`, `functools.lru_cache`,
`typing.final`, `typing.override`) or the registration itself, and the binding is stable and not abstract,
overloaded, or an accessor; an unknown decorator gives `wrapper`.

**Written arguments** are bound as Python binds them: a parameter written both by position and by keyword, a
repeated keyword, or any `*`/`**` expansion makes the binding uncertain. Literal triggers are plain Python string
literals only: bytes, f-strings, and names stay unknown.

### `pyproject.script`

```toml
[project.scripts]
orders-api = "orders_playground.server:main"
```

Every Python file (parsed or failed) belongs to its nearest `pyproject.toml`; an out-of-scope nearest one stops the
search rather than handing the file to a parent project, and a `pyproject.toml` that owns no Python file is never
read. Each owning file is a graph input, fingerprinted even when it declares no scripts.

Each `[project.scripts]` entry is one `process` entrance whose trigger is the script name and whose one `startup`
target is resolved from the literal string, never imported or run:

| Value | Target |
| --- | --- |
| `module:callable`, dotted Python identifiers, resolved against the source roots of the files the project owns | The concrete function or method, or a proven callable instance's `__call__` |
| A module or package path that exists but isn't indexed (a failed file, an excluded source root) | `not_indexed` |
| A missing module or attribute; a prefix that is an ordinary module; a namespace package's attribute; a child module nothing imports | `not_found` |
| A module that two roots supply, a module beside a same-named package, or contexts (source-root sets of the owned files) that disagree | `ambiguous` |
| A decorated function | `wrapper` |
| A re-export from another module (the startup seeds its own module), a class, a module, a non-callable value, an unstable binding, an external import, malformed syntax (`app.main()`, `app:main [extra]`), or a non-string value | `unsupported` |

Module and attribute strings are literal: they are compared with the exact spelling of files (a case-insensitive
disk doesn't make `k:main` find `K.py`) and with the NFKC names of declarations (`app:K` with a Kelvin sign
doesn't name `def K()`).

**Evidence:** the `pyproject.toml`, the exact byte range of the assignment (from its first key as written to the
end of its value: `project.scripts."a/b" = "m:f"`, `a = "m:f"` under `[project.scripts]`, or `"a" = "m:f"` inside an
inline table; no trailing comment or whitespace; offsets count a byte-order mark), and the RFC 6901 pointer
`/project/scripts/<name>` (`~` written `~0`, `/` written `~1`).

**Problems:** an owning file that isn't valid TOML (duplicate keys included), a `project` or `project.scripts`
that isn't a table, or a script with an empty name is listed in the Problems view and makes the graph incomplete;
the rest of the graph is kept.

### `fastapi.route`

`get`, `post`, `put`, `patch`, `delete`, `options`, and `head` on a proven `FastAPI` (`fastapi.FastAPI`,
`fastapi.applications.FastAPI`) or `APIRouter` (`fastapi.APIRouter`, `fastapi.routing.APIRouter`), as a function
decorator or an immediately applied route factory:

```python
@app.get("/health")                      # decorator: the decorated def is the handler
def health_check(): ...

orders_router.post("", dependencies=[Depends(require_user)])(create_order)   # applied
```

- **Evidence:** the registration call: the decorator's call (without the `@`) or the outer applied call.
- **Trigger:** the literal path (`path` positionally or by keyword), prefixed by every concrete `include_router`
  mount chain up to five mounts deep, with Python string concatenation (`'/first' + '/outer' + ...`); each mount
  branch is its own entrance (two identical mounts give two). A router with no mount, a cycle, a deeper chain, a
  non-literal or bytes prefix, or an annotation-only receiver keeps its local path as `partial`. A route whose
  mounts fan out into more than 256 branches gets one entrance whose trigger is its local path, `partial` (an Unfold
  bound; the proof of concept has none). An application route is `exact`; a non-literal, bytes, or f-string path is
  `unknown`.
- **Targets:** first the literal `dependencies=[Depends(...)]` lists of the application, the mounts (outermost
  first), the router, and the route, in declaration order (declaration order, not execution order), then the
  endpoint. Each `Depends` factory needs proven identity (`fastapi.Depends`, `fastapi.param_functions.Depends`,
  `fastapi.params.Depends`); a local `Depends`, an expansion (`*extra`), a non-literal list, or a mutated
  `Depends` implementation stays an explicit `unsupported` target. An applied endpoint that is itself
  route-decorated keeps its body; a call result (`async_handler(get_order)`) is `wrapper`.
- **Refusals:** an uncertain route factory (`app.get('/x', path='/y')`, `**settings`) makes the trigger `unknown`
  and every target `unsupported`; an applied call with expanded, extra, or unexpected arguments keeps each
  argument as an `unsupported` target; an applied call with no argument produces no entrance.
- **Not recognized:** `api_route`, `add_api_route`, other HTTP method names, parameter-default or `Annotated`
  dependencies, and APIs reached through arbitrary object fields.

### `celery.task`

Parameterized task decorators and immediately applied decorator factories on a proven `Celery` application
(`celery.Celery`, `celery.app.Celery`, `celery.app.base.Celery`):

```python
@celery_app.task(name="emails.send", queue="emails")
def email_task(payload): ...

process_order_task = celery_app.task(name="orders.process", queue="orders")(process_order)
```

- **Evidence:** the decorator's call, or the outer applied call.
- **Trigger:** the written, nonempty literal `queue`; absent, dynamic, bytes, empty, or uncertain queues are
  `unknown`. The task name is never a fallback.
- **Targets:** the decorated `def`, or the applied handler (`fun`, positionally or by keyword). An existing Task
  (a function already decorated by a registration) is `wrapper`: it is never unwrapped for a second registration.
- **Body proof:** a target keeps its body only when the application's constructor uses at most `main` (one
  positional), `broker`, and `backend`; the registration uses only a nonempty literal `name` and `queue`; the name is
  unique among all registrations and doesn't start with `celery.`; and nothing else could register, configure, or
  replace tasks. Celery shares task constructors between applications by default, so the indexed program is one
  registration domain: a duplicate or generated name anywhere, a bare `@app.task`, `app.task(function)`, a stored
  or container-held factory, `register_task`, `_task_from_fun`, `autodiscover_tasks`, `app.tasks.register` or
  `unregister`, `shared_task` (also through historical, rebound, or held aliases), or a configuration loader
  (`config_from_object`, `config_from_envvar`, `config_from_cmdline`, `add_defaults`, directly or through aliases)
  keeps every affected target `unsupported`. One allowance: a direct `app.conf.update(...)` with only the unique
  keywords `task_always_eager`, `task_eager_propagates`, `task_serializer`, and `accept_content`.
- **Not recognized:** bare decorators, `task(function)`, `shared_task`, task invocations (`delay`,
  `apply_async`, `signature`, `send_task`), and subclasses or annotation-only applications (an annotation-only
  application's tasks are listed but stay `unsupported`).

### `apscheduler.job`

APScheduler 3 `add_job` on a proven `BlockingScheduler` (`apscheduler.schedulers.blocking.BlockingScheduler`),
with the function and trigger as the first two positional arguments or named:

```python
scheduler.add_job(cleanup_expired_orders, CronTrigger.from_crontab("0 2 * * *", timezone="UTC"), id="cleanup")
```

- **Evidence:** the `add_job` call.
- **Trigger:** exact only for a proven `CronTrigger.from_crontab(...)` call (aliases of the factory included) whose
  `expr` is a literal string of exactly five whitespace-separated fields, with at most `expr` and `timezone`; the
  string is kept as written (`" 0  2 * * * "`). Any other trigger form, a dynamic or bytes string, uncertain
  arguments, or a mutated `CronTrigger` is `unknown`; cron expressions are never synthesized from fields.
- **Targets:** the `func` expression through the shared handler policy; a string function reference
  (`'app:first'`) is `unsupported` (never imported); a missing function keeps the call as an `unsupported`
  target.
- **Body proof:** the scheduler's constructor sets at most `timezone`; the job uses known `add_job` options, at most
  two positional arguments, and the default job store and executor; its literal ID is unique in its scheduler
  (omitted, empty, and `None` IDs keep APScheduler's generated identity; `replace_existing` doesn't help; a
  dynamic ID makes the scheduler uncertain); and no configuration change (`configure`, `add_jobstore`,
  `add_executor`, ...), hidden registration (`register = scheduler.add_job`, `scheduled_job`, stored or
  container-held methods), scheduler change (`modify_job`, `remove_job`, `reschedule_job`, `import_jobs`, ...), or
  change to a returned or looked-up `Job` (`job.modify(...)`, `job.func = ...`, `setattr(job, ...)`,
  `job.__setstate__(...)`, through containers and aliases) reaches that scheduler. Reading jobs (`get_jobs`,
  `print_jobs`, `export_jobs`, `__getstate__`) is fine. Otherwise the target stays `unsupported`.
- **Not recognized:** other scheduler classes, subclasses, `scheduled_job` decorators, and APScheduler 4 APIs.

### Differences from the proof of concept

- **Problems instead of failures.** The proof of concept failed the whole build on an unreadable owning
  `pyproject.toml`; Unfold lists it in the Problems view, marks the graph incomplete, and keeps everything else.
  A script with an empty name is listed the same way (its pointer would be `/project/scripts/`, which the
  graph model rejects).
- **Target texts of a few refusals.** A target's text is the expression's written label. Two refusals name a
  whole call that Unfold records only by its callee: a Celery applied call with no handler argument and an
  `add_job` call with no function. Their text is rebuilt as `callee()` (exact for a call without arguments) or
  `callee(…)`. Expanded arguments are named by their expression (`handlers`, not `*handlers`), as in value labels.
- **Sets.** The negative census follows list, tuple, and dict displays; a set display (`{app.task}`) is not
  followed.

## Mixed repositories

A folder with both TypeScript and Python files compiles into **one graph**: both languages' files, blocks,
slots, edges, and entrances, in one canonical order (files by path; entrances by evidence path, start, end,
detector, kind, method, trigger). Each language links only its own files, and each language's detectors read only
its own files and inputs (`package.json` for the TypeScript detectors; for Python, the `pyproject.toml` nearest to
each Python file). Nothing ever connects the two: there is no edge between a TypeScript and a Python block,
and spelling the same name in both languages supplies no evidence. A manual entrance's handlers can't mix
languages (the settings reject it); each locator resolves in its own language.

The mixed playground (`testdata/playgrounds/` opened as one folder) has 50 entrances: the TypeScript playground's
25 and the Python playground's 25, each with its `ts-orders/` or `py-orders/` prefix and otherwise unchanged
(`testdata/playgrounds/expected-entrances.txt`).

## Decisions

Decision C-8 (a binding destructured from an awaited dynamic import resolves) concerns calls, not entrances;
it is recorded with the TypeScript front end ([TypeScript, Decisions](languages/typescript.md#decisions)).

### C-9: property locators and calls

**Question** (the proof of concept's open defect C-9). A function-valued property of a constant object literal was a
manual locator target (`src/api.ts#api.arrow`), but an ordinary call of it (`api.arrow()`) was `not_concrete`. Should
the two rules be one?

**Decision.** They agree wherever a call has evidence, and a locator stays an explicit declaration selector. Unfold's
fix of C-2 already makes a call of a constant object literal's function or method member resolve to the same block
the locator names, so the original inconsistency is gone. The remaining differences are deliberate: a locator names a
declaration as written (the person who wrote it asserts what runs), while a call needs runtime evidence, so

| `main.ts` | Locator `main.ts#<owner>.arrow` | Call `<owner>.arrow()` |
| --- | --- | --- |
| `const api = { arrow: () => {}, method() {} }` | the arrow (and `#api.method` the method) | the same block |
| `const typed: Api = { arrow: () => {} }` (annotated) | the arrow | `unknown_receiver` (the annotation, not the literal, is the receiver's type) |
| `const written = { arrow: () => {} }; written.arrow = other` | the written arrow | `mutable_binding` (the mutation census) |
| `let mutable = { method() {} }`, `obj.alias` (an identifier value), `obj.data` | `unsupported` | not by the C-2 rule (left to the other rules) |

Locators never follow identifier-valued properties, inherited members, or nested objects, and calls never resolve
through an annotation or a reassigned member. Fixtures: `testdata/detectors/manual/c9-property-locators` (the locators)
and `members` (the refusals); `crates/unfold-lang-typescript/tests/entrances.rs`
(`property_locators_and_calls_agree_where_calls_have_evidence`) pins the calls' edges.
