# Configure entrances in Unfold

An **entrance** is a place where execution starts: a process launch, HTTP route,
queue worker, scheduled task, CLI command, or event handler. Configuring entrances
gives you useful starting points under **How it starts** on Home and in **Add** on a
board. Click **Trace** to follow one into its code.

Unfold reads **`unfold.toml`** at the root of the folder you opened as a project.
The format is TOML; `unfold.yaml` and `unfold.yml` are not project settings files.
Configuration describes your code and commands without running them.

This guide is for people editing the file and coding agents setting up a repository.
For connecting an agent, see [MCP setup](../README.md#connect-an-agent-with-mcp).
For the complete configuration semantics, see the
[engine reference](engine.md#shared-config); the
[parser](../crates/unfold-engine/src/settings/parse.rs) owns the accepted fields.

## Start with what Unfold already finds

Open the project and **Compile** before adding declarations. With no configuration,
Unfold enables its supported languages and detectors and analyzes sources allowed
by the project's ignore rules. It detects package scripts and binaries, Python
console scripts, and supported routes, workers, and schedules.

Use the smallest configuration that fills the gaps:

| What you need | Configuration |
| --- | --- |
| Explain a detected route, worker, or script | Trace the existing entrance |
| Add a launch Unfold does not discover | `[[entrance]]` with `kind = "process"` |
| Add a handler Unfold does not discover | `[[entrance]]` with its kind, trigger, and handler |
| Give a detected launch a better name or application | `[[entrance_override]]` |
| Group related launches and handlers on Home | `[[application]]`, referenced by `application` |
| Fix a target excluded from analysis | Review scope, ignore rules, and Python import roots |

An application groups entrances for navigation. Architectural connections between
services belong on a board. Declaring an entrance for an already detected launch
can produce a second entrance; use an override when you only want to change its
presentation.

You can also use **Add launch** on Home or **Add entrance…** in the project menu.
The editor previews what Unfold can read from a command and writes the same TOML
file. A function card's **Add as entrance…** starts with that function as its handler.

## A small service configuration

Suppose your TypeScript service starts with `tsx src/server.ts`, and
`src/server.ts` declares a `shutdown` function registered for `SIGTERM`. If its launch
is not already detected, this file adds it and the signal handler:

```toml
[project]
name = "orders"

[[application]]
id = "orders-api"
name = "Orders API"
description = "Accepts orders and dispatches background work."

[[entrance]]
id = "orders-api-start"
kind = "process"
application = "orders-api"
label = "Start API"
description = "Starts the HTTP server."
cwd = "."
command = "tsx src/server.ts"

[[entrance]]
id = "orders-api-shutdown"
kind = "event"
application = "orders-api"
label = "Graceful shutdown"
description = "Closes the server when SIGTERM arrives."
trigger = "SIGTERM"
handler = "src/server.ts#shutdown"
```

Replace the names, paths, commands, and triggers with ones you have verified in your
repository. The settings file is optional; add only the sections you need.

- **`id`** is a stable identifier. Keep it when a label, path, or command changes so
  board references continue to identify the entrance. IDs must be unique within
  their table type; use short names such as `orders-api-start`.
- **`label`** is what a reader sees. Use a short action such as “Import orders” or
  “Handle payment webhook”.
- **`description`** explains the role in a sentence. It must contain text when set.
- **`application`** refers to a declared application's `id`; its `name` is the
  human-readable heading on Home.
- **`[project].name`** names the project in cross-project boards. It does not create
  an application.

## Process launches

A `process` entrance describes how an application starts. Prefer the actual command
and working directory from your package metadata or deployment configuration.
Unfold can interpret supported Node/TypeScript launchers, package-script references,
Python scripts and modules, and Uvicorn commands.

`cwd` is relative to the opened project folder and defaults to `.`. Command paths
are interpreted from that working directory. For a monorepo, a launch might have
`cwd = "apps/api"` and `command = "npm run start"`, which reads that folder's
`package.json`.

For TypeScript emitted to JavaScript, a command such as `node dist/server.js` can
resolve back to its source through the project's TypeScript emit mapping. You do
not need to build or run the analyzed service to create that mapping.

When you only need to identify a startup file or function, use `startup`:

```toml
[[entrance]]
id = "api-start"
kind = "process"
label = "Start API"
startup = "src/server.ts"
```

A process requires at least one of `command`, `startup`, or `target`. It uses
startup fields instead of `handler`. `startup` and `target` are mutually exclusive.
Explicit startup locators and file targets are relative to the project root.

Unfold supports a limited command grammar. A command that starts multiple processes,
or wraps execution in `sh -c`, may remain unsupported. Keep the command as deployed
and set verified analysis details when needed; the editor shows the problem.
See [launch-command grammar](engine.md#launch-command-grammar) for the supported forms.

### Python launches and import roots

For a service whose `orders_api` package lives under `src/`:

```toml
[[python.project]]
root = "."
source_roots = ["src"]

[[entrance]]
id = "python-api-start"
kind = "process"
label = "Start Python API"
cwd = "."
command = "python -m uvicorn orders_api.main:app --app-dir src --port 8000"
```

`source_roots` configures Python import roots relative to `python.project.root`.
A launch's import context also follows its command: script folder, working folder,
Uvicorn's app directory, and a declared `PYTHONPATH` as applicable. Unfold reads the
command and declared `env` values; it does not read your shell's environment.

If the actual launch uses a wrapper Unfold cannot interpret, you can provide its
target and import paths explicitly after verifying them:

```toml
[[entrance]]
id = "python-api-wrapper"
kind = "process"
label = "Start Python API"
cwd = "."
command = 'sh -c "python -m uvicorn orders_api.main:app --port 8000"'
target = { type = "python_asgi", module = "orders_api.main", attribute = "app", factory = false }
python_paths = ["src"]
```

This is an alternative declaration for the wrapped launch. `python_paths` contains
project-relative folders in import-search order and replaces the entire inferred
list. For a readable command, an explicit target must agree with what the command
starts. Other target forms cover files, Python modules, and Python callables; see
[process fields](engine.md#manual-entrances-and-locators) for their definitions.

## Handler entrances

Every kind other than `process` needs a `handler`: a single locator or a nonempty
ordered list. Use `trigger` for the path, queue, schedule, command, or event name.
If the trigger is unknown, omit it; an empty string is invalid.

| Kind | Typical trigger |
| --- | --- |
| `http` | Route path, with an uppercase `method` |
| `worker` | Queue name |
| `schedule` | Schedule expression |
| `command` | CLI subcommand |
| `event` | Signal or event name |

### HTTP route with middleware

For a route that your framework's detector does not recognize:

```toml
[[entrance]]
id = "create-order"
kind = "http"
label = "Place an order"
description = "Authenticates the request and creates an order."
method = "POST"
trigger = "/orders"
handler = [
  "src/middleware/auth.ts#requireAuth",
  "src/routes/orders.ts#createOrder",
]
```

List middleware first, in the order it actually runs. All handlers in one entrance
must use the same language. `method` is valid only for `http` and uses supported
uppercase HTTP methods.

### Worker, scheduled task, and CLI command

Add only the declarations that match your code:

```toml
[[entrance]]
id = "process-orders"
kind = "worker"
label = "Process queued orders"
trigger = "orders"
handler = "src/workers/orders.ts#processOrder"

[[entrance]]
id = "nightly-reconciliation"
kind = "schedule"
label = "Reconcile orders nightly"
trigger = "0 2 * * *"
handler = "src/jobs/reconcile.ts#reconcileOrders"

[[entrance]]
id = "import-orders"
kind = "command"
label = "Import orders"
trigger = "import-orders"
handler = "src/cli.ts#importOrders"
```

For Python, use a Python file locator such as
`src/orders_api/jobs/import_orders.py#import_orders`.

`trigger` describes what invokes a handler. The `schedule` field available on a
process launch or detected-launch override is presentation metadata. Unfold does
not install timers, register routes, or connect queues.

## Point at the right code with locators

A locator identifies a file and, for a handler, a function or member inside it.
Paths use `/` and are relative to the opened project folder, including in a monorepo.

| Locator | Selects |
| --- | --- |
| `src/server.ts` | The file root; valid for a process startup |
| `src/routes/orders.ts#createOrder` | An export/re-export or supported top-level declaration |
| `src/handler.ts#default` | The default export |
| `src/orders.ts#OrderService.create` | A direct class or constant-object member |
| `src/cli.ts:12` | The outermost permitted block starting on one-based line 12 |

Prefer named locators because line numbers shift when code is edited. A handler
needs a function or supported member; a bare file path is insufficient. Absolute
paths, `../` paths, inherited-member lookup, and chained selectors such as
`#Service.client.send` are unsupported.

A valid locator is resolved again at compile time. Its target can still be missing,
excluded, external, or ambiguous. Unfold retains that uncertainty so you can correct
the declaration or scope. Verify the actual source before adding a locator.

## Improve detected launches with overrides

To rename and group a launch already detected from `package.json`, use an override:

```toml
[[application]]
id = "orders-api"
name = "Orders API"

[[entrance_override]]
match = { detector = "package.script", path = "package.json", pointer = "/scripts/start" }
application = "orders-api"
label = "Start API"
description = "Starts the production HTTP server."
```

The match names the detector, project-relative manifest path, and JSON pointer to
the manifest entry. For a monorepo script, the path might be
`apps/api/package.json`. Verify the exact entry before writing the match.

Overrides support launches detected by `package.script`, `package.bin`, or
`pyproject.script`. They change the application, label, description, or displayed
schedule. They do not replace the detected command or code target. An unmatched
override appears as a problem to review.

The same edit is available through a detected launch's **Edit…** menu on Home.
Existing [TypeScript](../testdata/playgrounds/ts-orders/unfold.toml) and
[Python](../testdata/playgrounds/py-orders/unfold.toml) configurations provide more
examples of manual entrances.

## Apply and verify the configuration

1. Preserve existing settings and comments. Add or update the relevant tables;
   keep existing entrance IDs when changing their presentation or targets.
2. Save `unfold.toml`. Background updates pick it up for active projects with
   updates enabled; click **Compile** to apply it explicitly.
3. Check **Health and activity**. Invalid TOML, unknown keys, duplicate IDs,
   undeclared application IDs, and illegal field combinations are errors. Unfold
   validates the whole file instead of applying only its valid sections.
4. Check **How it starts** for the expected application and entrance labels.
   Open **Trace**, then inspect the startup or handlers and their source.
5. Review unresolved targets. Confirm the file exists, the function or member
   matches, the file is in scope, and Python imports have the right roots.

Scope cannot override ignored or always-excluded folders. If generated output should
be excluded, use ignore rules or `[scope].exclude`; see
[discovery and scope](engine.md#discovery-and-scope). Shared settings belong in the
repository so teammates get the same starting points.

## Instructions for AI agents

Read this guide and inspect the repository before changing settings. Establish the
actual launch command, working directory, registrations, and handler definitions.
Preserve uncertainty where the source does not establish an entry point. Keep the
change focused and show the resulting config diff to the user.

### Through Unfold MCP

Read `get_guide` with `topic = "setup"`, use `list_projects` and `get_project` to
identify the project, and inspect existing entrances with `list_entrances`.
Use `update_project_settings` for edits supported by its advertised schema.
For example, adding a verified signal handler uses this tool payload:

```json
{
  "project": "orders",
  "edits": [
    {
      "op": "add_entrance",
      "entrance": {
        "id": "shutdown",
        "kind": "event",
        "label": "Graceful shutdown",
        "trigger": "SIGTERM",
        "handler": ["src/server.ts#shutdown"]
      }
    }
  ]
}
```

Confirm the ID is new before using `add_entrance`. The tool uses a handler **array**;
the TOML file also accepts a single string. Python tool edits use `sourceRoots`,
while the TOML spelling is `source_roots`.

The current [MCP settings tool](../desktop/src/main/mcp/tools/setup.ts) exposes
handler declarations and simple process `startup` locators, scope, detectors, and
Python settings. Its entrance schema does not expose application groups,
descriptions, launch commands, working directories, explicit targets, environment,
import-path overrides, or detected-launch overrides.

For those fields, use Unfold's launch editor or edit `unfold.toml` with a coding
agent that has repository file access. An MCP `replace_entrance` supplies a complete
replacement using that tool's smaller schema; it cannot preserve richer fields it
does not expose. Use the editor or a focused file edit for those declarations.

After an MCP settings edit, call `compile_project`, then check `get_project` and
`list_entrances`. A long compile can continue after the tool returns; wait until the
project finishes compiling before judging the result. Read the entrance with
`read_code` to verify its startup or handlers.

### A prompt for a coding agent

```text
Read docs/entrances.md and configure unfold.toml for this repository.
Inspect existing settings, package/deployment commands, framework registrations,
and handler definitions. Keep existing detected entrances and stable IDs.
Add only missing entry points, use application groups and concise labels, and
preserve comments and unrelated settings. Verify each path and target, compile
the project in Unfold, and report unresolved targets and the config diff.
```

Use a path to this guide that the agent can access. If it only has Unfold MCP,
restrict the request to fields its tools expose or provide the richer TOML edit
for a person to apply.
