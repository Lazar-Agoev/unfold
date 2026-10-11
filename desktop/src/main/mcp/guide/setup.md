# Setting up: projects, Compile, settings, and boards

Settings shape the graph and live in `.unfold/config.toml`, which the team shares
through the repository. Change them only when the person asks, and tell them the file
changed: it shows in their git diff.

## Projects
- list_projects: every project's name, id, state, languages, branch, and boards. A
  resting project isn't open right now; get_project opens it and reports everything:
  applications, health, settings, boards.
- add_project({path}): an absolute path, or one starting with `~/`. Call it only for a
  folder the person named. Unfold asks them in a dialog ("Claude wants to add
  ~/repos/payments to your workspace."). If they cancel, nothing was added; don't ask
  again unless they bring it up. A folder that already is a project (or another
  worktree of its repository) answers with that project. The result says what a first
  Compile would read and skip.
- You can't remove a project: the person does (File › Remove Project).

## Compile
- compile_project reads the project's TypeScript and Python without running anything,
  writes only inside its `.unfold/` folder (the first Compile creates it), and keeps
  the graph current in the background afterwards. mode "compile" (the default) updates
  what changed; "rebuild" starts from scratch: only when the person asks, or the graph
  looks wrong.
- It waits up to 50 seconds. A longer compile goes on in Unfold: call get_project to
  keep waiting until its state reads current.
- States: current, compiling, updating, paused, not compiled, problems (files that
  don't parse; Unfold keeps their last version that parsed), resting, unavailable,
  and failed (the previous graph stays).
- When a file changed since the last compile, read_code and read_file say so and show
  no source. Compile, then read again.

## Settings
update_project_settings takes edits that apply in order, all or nothing, and returns
the change to `.unfold/config.toml` as a diff. An edit that would make the file
invalid, or that a broader rule stands in the way of, is refused with the reason, and
nothing changes. get_project lists the settings now. The graph follows at the next
compile_project or background update.

### Scope
- exclude_from_graph, include_in_graph, and graph_only take a folder or file relative
  to the project's folder (or a glob such as `**/generated/**`); remove_scope_pattern
  removes one pattern exactly as get_project lists it.
- Ignore files and folders such as node_modules and .venv are always left out.
- Excluded code still exists: calls into it are unresolved ("not indexed"), not gone.

### Detectors
`{op: "set_detector", detector, enabled}` turns one detector on or off: package.script,
package.bin, express.route, bullmq.worker, node-cron.schedule, pyproject.script,
fastapi.route, celery.task, apscheduler.job. Declared entrances (manual) can't be
turned off; remove them instead.

### Python projects
set_python_project with its root, sourceRoots (import roots relative to root), and
version (3.12, 3.13, or 3.14); remove_python_project by root.

### Declared entrances
For an entrance the detectors can't see, such as a command dispatched by hand or a
signal handler: add_entrance, replace_entrance, and remove_entrance (by index, from 0,
in get_project's list).
- kind: process, command, http, worker, schedule, or event; trigger (the path, queue,
  schedule, command, or event name); method for http; an optional label.
- handler: locators, middleware first (every kind but process); startup: one locator
  (process only).
- Locators: `path#name` (an export or a top-level declaration), `path#Owner.member`,
  `path#default`, or `path:line`. Prefer names: a line doesn't follow edits.
- ts-orders declares its SIGTERM handler: kind event, trigger SIGTERM, handler
  `src/server.ts#shutdown`.

### The project's name
set_project_name sets the shared name other projects' boards use to name this one;
null goes back to the folder's name. Links and tools use the new name afterwards.

### Evidence files
Files Unfold doesn't compile but can cite: SQL migrations, Prisma schemas, OpenAPI
specs, Terraform, docker-compose files. add_evidence_pattern adds a file, folder, or
glob; remove_evidence_pattern removes one exactly as listed. After the next compile,
read_file shows them as text with line numbers, and resource cards and connections can
anchor to their lines as `{file, lines: [from, to]}`. A file that is already a source,
a TypeScript declaration file, or a configuration input keeps that role.

### Background updates
set_background_updates turns updating the graph as files change on or off. It's kept
in Unfold, not in the file.

### Example
"Add ~/repos/ts-orders to Unfold and compile it. Turn off the cron detector; we don't
use those schedules anymore."
- add_project({path: "~/repos/ts-orders"}); the person clicks Add. The result: a first
  Compile would read 34 TypeScript files and skip 2 JavaScript files.
- update_project_settings({project: "ts-orders", edits: [{op: "set_detector",
  detector: "node-cron.schedule", enabled: false}]}). The result shows the change:
  ```diff
  +[detectors]
  +disabled = ["node-cron.schedule"]
  ```
- compile_project({project: "ts-orders"}): the graph is current, and the result names
  its applications.
- In chat: "ts-orders is compiled. I turned off the node-cron detector, so its two
  schedules don't show. That setting is in .unfold/config.toml, which is shared, so
  it will show in your git diff."

## Boards
- create_board makes a draft, with named pages, optionally in a folder and traced from
  an entrance, in a background tab of the window the person used last: their view
  doesn't move. Pass its board and page ids to the next tools.
- organize_boards: create_folder, rename_folder, delete_folder (an empty one), move a
  board into a folder, rename a board, duplicate a board. Ops run in order and stop at
  the first that fails, saying what was done.
- edit_pages: add, rename, move, duplicate, and delete a board's pages, as one undo step.
- show brings a board, page, or item into the person's view. Use it when they ask to
  see something, or to present finished work, never while building.
- A draft lives on this machine until the person publishes it, and its id changes
  then. Your edits to a published board wait as unpublished changes: say which board
  you changed.
- You can't publish, delete, or discard a board, or move one back to drafts: those
  are the person's, from the board's menu.
