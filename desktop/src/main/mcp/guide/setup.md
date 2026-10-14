# Setting up: projects, Compile, settings, and boards

Settings shape the graph and live in `unfold.toml`, which the team shares
through the repository. Change them only when the person asks, and tell them the file
changed: it shows in their git diff.

## Projects
- list_projects: every project's name, id, state, languages, and boards. A
  resting project isn't open right now; get_project opens it and reports everything:
  apps, health, settings, boards.
- add_project({path}): an absolute path, or one starting with `~/`. Call it only for a
  folder the person named. Unfold asks them in a dialog ("Claude wants to add
  ~/repos/payments to your workspace."). If they cancel, nothing was added; don't ask
  again unless they bring it up. A folder that already is a project answers with that
  project. Unfold needs to write `.unfold/` in the folder: a read-only folder, or one on
  a network volume, is refused, and the error says so.
- The project compiles as it is added. The result waits up to 50 seconds for that
  compile, as compile_project does.
- You can't remove a project: the person does (the project menu's Remove from workspace…).

## Compile
- compile_project reads the project's TypeScript and Python without running anything,
  writes only inside its `.unfold/` folder, brings the graph up to date with what
  changed, and keeps it current in the background afterwards.
- It waits up to 50 seconds. A longer compile goes on in Unfold: call get_project to
  keep waiting until its state reads current.
- States: current, compiling, updating, not compiled, problems (files that
  don't parse; Unfold keeps their last version that parsed), resting, unavailable,
  and failed (the previous graph stays).
- When a file changed since the last compile, read_code and read_file say so and show
  no source. Compile, then read again.

## Settings
update_project_settings takes edits that apply in order, all or nothing, and returns
the change to `unfold.toml` as a diff. An edit that would make the file
invalid, or that a broader rule stands in the way of, is refused with the reason, and
nothing changes. get_project lists the settings now. The graph follows at the next
compile_project or background update.

### Scope
- exclude_from_graph, include_in_graph, and graph_only take a folder or file relative
  to the project's folder (or a glob such as `**/generated/**`); remove_scope_pattern
  removes one pattern exactly as get_project lists it.
- Ignore files and folders such as node_modules and .venv are always left out.
- Excluded code still exists: calls into it are unresolved ("not indexed"), not gone.

### Python projects
set_python_project with its root, sourceRoots (import roots relative to root), and
version (3.12, 3.13, or 3.14); remove_python_project by root.

### Apps and entrances
Apps and their entrances are declared in `unfold.toml` as `[[app]]` tables, never
detected. list_entrances lists them with each code key's state; the person edits them
in `unfold.toml`.

### Evidence files
Files Unfold doesn't compile but can cite: SQL migrations, Prisma schemas, OpenAPI
specs, Terraform, docker-compose files. add_evidence_pattern adds a file, folder, or
glob; remove_evidence_pattern removes one exactly as listed. After the next compile,
read_file shows them as text with line numbers, and resource cards and connections can
anchor to their lines as `{file, lines: [from, to]}`. A file that is already a source,
a TypeScript declaration file, or a configuration input keeps that role.

### Example
"Add ~/repos/ts-orders to Unfold. Leave the scripts folder out; it's one-off tooling."
- add_project({path: "~/repos/ts-orders"}); the person clicks Add. The project
  compiles, and the result names its apps.
- update_project_settings({project: "ts-orders", edits: [{op: "exclude_from_graph",
  path: "scripts"}]}). The result shows the change:
  ```diff
  +[scope]
  +exclude = ["scripts"]
  ```
- compile_project({project: "ts-orders"}): the graph is current without scripts.
- In chat: "ts-orders is compiled, without the scripts folder. That setting is in
  unfold.toml, which is shared, so it will show in your git diff."

## Boards
- A board is a file in the project's .unfold/boards/, saved as it is edited. Git is
  how a team shares it.
- create_board makes one, with named pages, optionally in a folder and traced from an
  entrance. Unfold loads it without moving the person's view. Pass its board and page
  ids to the next tools.
- organize_boards: create_folder, rename_folder, delete_folder (an empty one), move a
  board into a folder, rename a board. Folders are one level deep. Ops run in order
  and stop at the first that fails, saying what was done.
- edit_pages: add, rename, move, and delete a board's pages, as one undo step.
- show brings a board, page, or item into the person's view. Use it when they ask to
  see something, or to present finished work, never while building.
- A page shows one project. A service in another repository is an external resource
  card, with a connection to it that cites the client code.
- You can't delete a board: that is the person's, from the board's menu.
