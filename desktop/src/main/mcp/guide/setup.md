# Setting up: projects, Compile, settings, apps, and boards

Settings, apps, and entrances live in `unfold.toml`, which the team shares through the
repository. Change it only when the person asks, or says yes to apps and entrances you
proposed, and tell them it changed: it shows in their git diff.

## Projects
- list_projects: every project's name, id, state, languages, and boards. A resting
  project isn't open right now; get_project opens it and reports its apps, health,
  settings, and boards.
- add_project({path}): an absolute path, or one starting with `~/`, for a folder the
  person named. Unfold asks them in a dialog; if they cancel, nothing is added, so don't
  ask again unless they bring it up. Unfold needs to write `.unfold/` in the folder: a
  read-only folder, or one on a network volume, is refused.
- The project compiles as it is added; the result waits up to 50 seconds.
- You can't remove a project: the person does, from the project menu.

## Compile
- compile_project reads the project without running anything, writes only inside its
  `.unfold/` folder, and keeps the graph current afterwards. It waits up to 50
  seconds; for a longer compile, call get_project until the state reads current.
- States: current, compiling, updating, not compiled, problems (files that don't
  parse; Unfold keeps their last version that parsed), resting, unavailable, and failed.
- When a file changed since the last compile, read_code and read_file say so. Compile,
  then read again.

## Settings
update_project_settings takes edits that apply in order, all or nothing, and returns
the change to `unfold.toml` as a diff. get_project lists the settings now. The graph
follows at the next compile.
- **Scope**: exclude_from_graph, include_in_graph, and graph_only take a folder, file,
  or glob relative to the project's folder; remove_scope_pattern removes a pattern
  exactly as get_project lists it. Folders such as node_modules and .venv are always
  left out. Calls into excluded code are unknown ("not indexed"), not gone.
- **Python projects**: set_python_project with its root, sourceRoots, and version;
  remove_python_project by root.
- **Evidence files**: files Unfold doesn't compile but boards can cite, such as SQL
  migrations, Prisma schemas, OpenAPI specs, or docker-compose files.
  add_evidence_pattern and remove_evidence_pattern. After the next compile, read_file
  shows them with line numbers, and anchors can cite `{file, lines: [from, to]}`.

## Apps and entrances
An app is something you run (a server, a worker, a command-line tool); an entrance is
something it handles (a route, a queue, a schedule, a signal). Both are declared in
`[[app]]` tables, never detected: a project without them has no entrances.
- list_entrances: each app with its `run` and `code`, and its entrances with their
  key, `on`, and `code`. Each code key has a state: found, missing, not analyzed, not a
  start, or stale, with a fix when there is exactly one candidate.
- set_app writes an app's run, code, and about; set_entrance adds or changes one
  entrance (and creates its app if it is new); remove_entrance removes one, or a whole
  app. Each returns the diff. Only when the person asks, or said yes to your proposal.
- `run` and `on` are text as written; Unfold never runs or parses them. `code` is a
  start's key: a module (`src/server.ts#`), a definition
  (`src/routes/orders.ts#createOrder`), an inline handler
  (`src/routes/health.ts#/call:healthRouter.get("/live")/callback[0]`), or a list of
  them in the order they run. Never a call, branch, or exit.
- Keys come from names: the app Server is `entrance:server`, its entrance Place an
  order `entrance:server/place-an-order`.
- Writing them never recompiles. Then list_entrances: every code should read found.

To set up a project, read its routes, workers, schedules, and scripts, and take each
`run` from package.json, pyproject.toml, a Procfile, or a Dockerfile. In ts-orders:
```
set_app({project: "ts-orders", app: "Server", run: "npm run start", code: "src/server.ts#"})
set_entrance({project: "ts-orders", app: "Server", name: "Place an order",
  on: "POST /orders", code: ["src/routes/middleware.ts#requireUser",
  "src/routes/middleware.ts#validateBody", "src/routes/orders.ts#createOrder"]})
```
In chat: what you declared, and that `unfold.toml` changed.

## Proposing entrances
A page starts from entrances, so when the code a question is about has none
(list_entrances says the project declares no apps, or none reaches that code), propose
them before building anything.
- Find them in the repository: deploy config (render.yaml, fly.toml, a Procfile, a
  Dockerfile, docker-compose), crontab or cron config, package.json and pyproject.toml
  scripts, and the framework's route and worker registrations.
- Propose them in one message: each app's name, run, and code; each entrance's name,
  on, and code, with the file:line that shows it.
- Ask once; a yes covers all of them. Then set_app and set_entrance, list_entrances
  (every code found), and trace them.
- Never invent one the code or its config doesn't show. If the person says no, build
  from the functions themselves.

## Boards
- A board is a file in the project's `.unfold/boards/`, saved as it is edited. Git is
  how a team shares it.
- create_board makes one with named pages, optionally in a folder and traced from
  its entrances. Unfold loads it without moving the person's view. Pass its board and
  page ids to the next tools.
- organize_boards: create_folder, rename_folder, delete_folder (an empty one), move a
  board, rename a board. Folders are one level deep. Ops run in order and stop at the
  first that fails.
- edit_pages: add, rename, move, and delete pages, as one undo step.
- show brings a board, page, or item into the person's view: only when they ask, or to
  present finished work.
- You can't delete a board: that is the person's, from the board's menu.
