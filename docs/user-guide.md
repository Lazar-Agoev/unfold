# Using Unfold

Unfold helps you read a codebase by following how it starts and where it goes. It reads TypeScript and Python. It
never runs your code, installs anything, or runs Git. Shortcuts below are for macOS; on Windows and Linux use Ctrl
where you see ⌘. The full list is in [the keyboard map](ui-shell.md#keyboard).

## Projects and boards

Everything you connect lives in one **workspace**. Each **project** in it is a folder with its own graph, usually a
repository. You explain the code on **boards**: a board is a story, such as "Order lifecycle", and holds **pages**,
each one trace, such as "Placing an order". A page holds **cards**: an entrance or a function, shown as code or as a
diagram. Projects hold code; boards hold stories.

## Add your first project

The first time you open Unfold, Home is empty. Press **Add project…** (⌘O) and pick a folder, or drop a folder
anywhere on the window. The folder joins the workspace as a project, and its Home opens. Adding a project writes
nothing to it. Folders you removed recently are listed under "Opened before", one click from coming back.

You can add as many projects as you like (up to 64): **+** next to Projects in the sidebar, or ⌘O, adds another.

## The first Compile

A project you haven't compiled yet shows what Unfold is about to do: how many TypeScript and Python files it will
read, and which folders it skips and why (for example `node_modules`, `dist`, or anything your `.gitignore`
ignores). For a large repository, clear the check boxes of the folders you don't care about.

Press **Compile** (⇧⌘B). Unfold reads the files, works out what calls what, and finds the ways the code starts. It
creates a `.unfold/` folder in the project to keep the graph and your boards. From then on the graph keeps itself
current: you don't need to compile again after editing.

## Home

Home is the first tab of every window. It shows the project selected in the sidebar:

- **How it starts:** the project's applications, named after how each one starts (`server`, `worker`), with their
  entrances: launch scripts and commands from `package.json` and `pyproject.toml`; HTTP routes (Express, FastAPI);
  queue workers and tasks (BullMQ, Celery); schedules (node-cron, APScheduler); and entrances you declare yourself
  ([below](#settings)). Hover an entrance and click **Trace** to open it on a new board, with the functions that
  handle it.
- **Boards:** the boards people made about this project, published and drafts.
- **Health:** one line that says whether anything needs a look, such as a file that doesn't parse or a board whose
  code changed. **Health and activity** shows the whole list and the log of recent updates.

The line under the project's name says where things stand: "Graph current · 84 files · TypeScript + Python ·
updated 2 min ago", "Updating · 2 files changed", "1 file doesn't parse", and so on. The project's row in the
sidebar shows the same thing as one small mark ([every state](../crates/unfold-workspace/src/status.rs)).
The project menu (`⋯` on its row or on Home) has Compile, Rebuild from scratch, Pause background updates, Settings,
Health and activity, and Remove from workspace.

**Recents** and **Drafts**, at the top of the sidebar, show boards from every project: the ones you opened
recently, and every draft and unpublished change on this machine.

## Boards and pages

A board opens in a tab at the top of the window, and the sidebar switches to it:

- **Pages:** the board's pages. **+** adds one (⌥⌘N); a page's `⋯` menu renames, duplicates, reorders, or deletes
  it.
- **On this page:** the cards in the order you followed them, indented by call depth, with their groups and links,
  and [frames](#frames) as headings with their cards under them.
- **Selected:** the card you selected: where it is, its description, who calls it, which entrances reach it, and the
  calls you haven't opened yet (**+ Open**). For a frame: its description, what it holds, and the links in and out.

The bar above the canvas shows the page, whether the board is a draft or published, the **Diagram | Code** lens,
**Share**, and **Publish**. The dock at the bottom has **Add**, Back and Forward, the trail of calls you followed,
**Arrange**, and the zoom. ⌘B hides the sidebar.

## Following calls

Each function on a page is a card. In a card, calls you can follow are links:

- **hover** a call to see what it calls and where;
- **click** it to open that function's card next to this one;
- **⌥-click** it to read the called function inline, without opening a card.

A call Unfold can't follow has a dashed underline; hover it to see why (for example, the target depends on a value
only known when the program runs). A call into a library shows the library's name. Unfold only links a call when it
can prove where it goes, so a link is always right.

Use **Back** (⌘[) and **Forward** (⌘]) to retrace your steps, **Fit** (⇧1) to see everything, and ⌘= / ⌘- to zoom.
**Show callers**, in a card's menu, adds the functions that call it to its left.

## Diagram and Code

The **Diagram | Code** lens above the canvas (⌘1, ⌘2) sets how the page reads. A new page opens in Diagram. Switching
to Code and back keeps where you were, what is selected, and what you expanded.

- **Code** shows each function's exact source, with folds and links.
- **Diagram** shows each function and entrance compact or expanded. The chevron at the start of a card's header
  expands only that card, and that choice is saved with the board (⌘Z undoes it):
  - **compact**, one small box: what it is (`ƒ` for a function, or an entrance's kind, such as `HTTP`), its name,
    where it is, and its description. Its calls to another card are one arrow from its edge, labeled with the call's
    signature, or "3 calls" when there are several: click that to list them in Selected and open each one;
  - **expanded**, a function's written flow ("Paths from written structure"): its steps, branches, and loops, with
    each call's arrow leaving from its own step. An expanded entrance shows where it is declared and its targets in
    order, under **Starts** for what a launch starts or **Handlers, in order** for a route's handlers.

So a page can show the functions around the one that matters as boxes and that one expanded.

### Expanding a call in place

In an expanded function, a call Unfold can follow and each callback has its own small chevron. It shows the called
function's steps right under the call, titled with its name and `file:line`: click the name to read its definition in
the peek sheet, or `→` to open it as its own card. Calls inside can expand again, three levels deep at most; a call
back to a function already shown has no chevron. A callback whose only step is one call shows that call's function,
captioned "via callback to …", so you still see it is reached through the callback. Library calls and calls Unfold
can't follow keep their explanation and can't be expanded. A step selected inside belongs to the called function in
Selected. To group or describe those steps, open the called function as its own card.

A **type** (an interface, a type alias, an enum, or a class) shows its fields in Diagram view: each field's name and
written type, `?` and `readonly` as written, and the fields it inherits under the type they come from. Click a type
in a field to read its definition in place. When a field's type is another type on the page, an arrow goes from that
field to it, labeled with the type. A type with twelve fields or more shows the first eight; "N more" shows the rest.
Selected says where a type is used ("Used in N places").

## Add

**Add** (`A`, ⌘K, or `/`) finds code in every project and puts it on the page:

- Before you type, it shows every project's applications and their entrances, with the ones already on the page
  marked.
- As you type, it finds entrances, functions, files, and text, with a tab for each and a preview on the right.
  Functions include classes and types (`interface Cart`, `type CartId`, `enum Currency`); a type goes on the page
  showing its fields. On the Text tab you can match case, whole words, or a regular expression.
- **Enter** adds the selection to the page, and **⌘Enter** adds it on a new page. A file or a text match opens in the
  peek sheet instead. From Home, Enter starts a new board.
- Type `>` first to run a command instead: everything the app can do is listed there, with its shortcut. ⇧⌘P opens
  Add this way, ⌘P on Files, and ⌘T on Functions.

## Reading a whole file

A file opens in the **peek sheet**, on the right of the canvas: the whole file, read-only, with its calls as links.
The `⊕` beside a function, class, or type adds it to the page, and so does a click on a call. Pin the sheet to keep it open while
you work; Escape closes it. Its menu opens the file in your own editor (⇧⌘O) or shows it in Finder.

### View code

**View code** opens the actual source of what is selected in the same sheet, with its lines tinted and, for a
step, its exact expression marked: a condition's own test rather than the whole `if`, a call's expression, else the
statement. Press `C` with something selected on the canvas, click the small `</>` on a hovered or selected step or
on a combined block, choose View code from a step's or a group's menu (right-click, or ⇧F10), or click the location
Selected shows ("line 27"). A card's Open source does the same for the whole function.

- A function or type card opens at its definition. A combined block shows the span of its statements.
- A step inside a call expanded in place opens the called function's file; the inset's title opens its definition.
- An entrance's `</>`, beside where it is declared, opens that registration or configuration; each resolved
  target's own `</>` opens that handler's or startup's definition. Clicking a target's name still opens its card.
- A resource card or a connection opens the first source it cites.

While the sheet is open from View code, selecting another item on the page moves it there; the page's selection,
expansions, and camera stay as they are, and an item without source leaves the sheet where it is. A file changed
since the last compile shows as it is now, saying "Changed since the last compile · the range isn't shown"
rather than marking lines that may have moved. External and unresolved calls or targets have no source to open.

## Describing, labeling, and grouping

Boards are for explaining a path to someone else (or to yourself later):

- Select a **card** and write what the function does in Selected, or double-click the description strip on the card.
- Select a **Flow node** (a step of an expanded card) and give it a plain-language label in place of its code
  ("Queue the order" instead of `ordersQueue.add(...)`). "N Flow steps unlabeled · Label them", under On this page,
  steps you through the nodes that still show code.
- To **group** lines, drag over the line numbers of a Code card and give the group a title; it shows as a colored
  rail beside those lines. In an expanded card's flow the same group is a **step group**: a titled region around its
  steps. Its chevron collapses it, in Code and Diagram alike: in the flow it becomes one step with the group's title,
  marked when something inside can return or throw. Right-click a group to describe it or ungroup it.
- To **combine** steps of an expanded function, click the first step and Shift-click the last: Selected says how
  many steps and which lines, and whether they can be combined. Right-click the run and choose **Combine…**, give
  the block a short title, and it becomes one collapsed **COMBINED** step with its step count, chevron, and `</>`;
  the chevron shows the original steps again. Combine is a group, by the same rule as dragging over line numbers
  (and as an agent's `group_steps`): whole consecutive statements of one body, never two branches, a nested
  function, or lines another group holds; Combine… says why when it can't. Steps shown inside a call expanded in
  place are combined on that function's own card. Select a combined block to retitle or describe it, view its
  code, or Ungroup it, which restores the steps; neither changes your code.

Everything you write is signed with your name. ⌘Z and ⇧⌘Z undo and redo board edits, on any page.

### Proposed steps

A **proposed step** marks a change you intend, where it would go, without pretending the code does it. Right-click a
real step of an expanded function's own flow and choose **Add proposed step after…**; after a decision, choose one of
its paths ("on yes…", "on no…", "each…", "done…"). A return, throw, break, or continue offers none, nor does a step
shown inside a call expanded in place (propose it on that function's own card) or a collapsed combined block (expand
it first). The step appears in place with a dashed green outline and a **PROPOSED** row, and its title is selected in
Selected for you to replace. Dashed green connectors show where it would go; the solid compiled path still runs
beside it.

In Selected, give it a one-line title and an optional **Why**; whoever last changed the words is shown as its author,
with the agent mark when an agent wrote them. **Where** names the step it goes after, and that step's `file:line`
opens its code: a proposed step has no code of its own. **Copy link** points an agent at it, and **Remove proposal**
deletes it (one undo step) without touching your code. On this page lists it under its card, marked "proposed", and a
compact card shows how many it holds ("1 proposal").

If the step it goes after disappears or is held for review, the proposal keeps its words and says so on the card.
**Review** offers the explicit choices: **Reconnect** it to a step you pick from its card, **Accept** the current code
(when held), or remove it. It is never moved to a similarly named step for you. Mermaid export leaves proposed steps
out; page images include them.

## Frames

A **frame** is a named box around cards: a unit of a few functions ("Request checks"), a service ("Order API"), or,
drawn as a tinted **zone** with a large title, an area that holds services ("Backend"). Its icon says what it is (an
API, a worker, a schedule, a command line, the web, a library, or a system). Frames nest up to three deep. An agent
draws them for you ([Connecting an agent](#connecting-an-agent)); you adjust them:

- **Collapse** a frame with its chevron: it becomes one box with its title, description, and what it holds ("3
  functions · 1 table"), and the arrows and links of what's inside attach to it. The chevron expands it again.
  Nothing collapses by itself.
- **Rename** it by double-clicking its title. Right-click the title for its menu: Rename…, Describe… (write its
  description in Selected), Collapse or Expand, Copy link, and **Remove frame (keep cards)**, which leaves its cards
  where they are, in the frame around it if there is one. What you write is yours.
- **Move** it by dragging its title: everything in it moves with it, as one undo step.
- **Arrange** (⇧A) lays out each frame's cards by call depth, to the right or downward as the frame says, then the
  page around the frames.

A frame's description remembers the code of the cards in it: when that code changes, Selected says so and offers to
keep the text or remove it.

## Tables, queues, connections, and notes

Some of what a page explains isn't a function: the table an order is saved in, the queue between two services, the
customer who starts it all. An agent adds these for you, each citing the code (or the lines of an
[evidence file](engine.md#discovery-and-scope)) that shows it is there; they are drawn in the authored color,
with the agent mark when an agent drew them, and look the same in Diagram and Code.

- **Resource cards** are tables, queues, caches, storage, services outside your code, and actors, each with its
  kind's glyph, its name, and a description. A **table** lists its columns: when the code declares its shape (an
  interface, a dataclass, a model), the columns are that type's fields, so they follow the code, with marks for the
  primary key (`PK`), foreign keys (`FK → customers.id`), and unique columns; up to 12 show, then "N more". Select a
  resource card to see what it cites (click one to open it in the peek sheet), its type (click to open its
  definition), and the lines in and out of it.
- **Connections** are lines that say how two things talk when the graph can't show it: an HTTP request, a queue, an
  event, reads and writes, a call that goes somewhere only at runtime, a relationship between two tables, or
  anything else. Each kind has its own line (long dashes for a queue, dots for an event, crow's feet for a
  relationship) and a chip that says what travels: `queue · orders · OrderJob`. Click the type's name on the chip to
  read its definition. A connection leaves from the line where it happens (in Code and in an expanded card) and from
  the card's edge when it is compact. Right-click a link for **Kind ▸** to change what it is (a drawn link becomes a connection), edit
  its label and description in Selected, or remove it. Connections are never compiled: Called by and Reached from
  list only what the code proves.
- **Notes** are a sentence or two on the page: a caveat, a reading order, an open question. A note attached to a
  card, frame, table or other resource card, or link moves with it; drag it to put it elsewhere beside its item.
  Double-click a note to edit it, right-click to remove it. A note whose item is removed stays where it was.

**Arrange** puts a table after the code that writes it and before the code that reads it, each note attached to a
card, resource card, or frame under its item, and each note on a link beside the middle of its line, clear of the
cards. A note whose card has grown over it since is shown under the card.

**When the code changes.** Descriptions and labels remember the code they were written for. If that code changes,
the card shows "Changed since it was described · Review", which lists what happened and offers repairs. A resource
card or connection remembers the code it cites the same way: "Changed since it was drawn · Review" offers to keep it
as it is, reconnect it to the code it moved to, or remove it; a connection's chip shows a review dot, and Selected
its repairs. Nothing is ever silently deleted.

## Drafts and publishing

A new board (⌘N) is a **draft**: it lives only on your machine, in the project's `.unfold/local/drafts/`, and saves
as you work. Organize a project's boards in folders: **Move to folder** in the board menu (its **New folder…** makes one), or
right-click a folder in the sidebar.

When a board is ready to share, press **Publish** (⇧⌘E). Unfold writes it to `.unfold/boards/<name>.json` inside the
repository. Commit that file like code; teammates who pull it see the board on their Home.

**Edits to a published board** stay on your machine as **unpublished changes** ("Published · 2 unpublished changes")
until you press **Publish** again, so your team never sees half-finished edits in `git diff`. **Discard changes**, in
the board menu, goes back to the published version. If someone else's version arrived since you started (you pulled
a change to the same board), Publish asks whether to keep yours or take theirs.

**Move back to drafts**, in the board menu, takes a board out of the repository again. **Share** exports the page as
a Mermaid diagram (`.mmd`), which renders in GitHub, GitLab, and many wikis, or copies it. **Share › Export PNG…**
saves the page as an image, drawn as the board shows it (your unsaved changes included) at twice its size, named
`<board> - <page>.png`.

## Boards across projects

A page can hold cards from any project in your workspace, for example the web app's checkout handler next to the
API route it calls: add them with Add, which searches every project. The graph never links code across
repositories, so draw the link yourself: **Draw link…** in a card's menu, then click the other card, and give the
link a label such as "calls over HTTP". Right-click it for **Kind ▸ HTTP** to make it a typed connection.

A teammate who hasn't added one of those projects sees its cards as placeholders, with **Connect project** to add it.
Projects are matched by name: a project's name is its folder's name unless you change it in its settings. A board
can't hold cards from a project with the same name as its own; rename one of them first.

## Several windows

You can open as many windows as you like: **New Window** (⇧⌘N). Every window shows the whole workspace and shares
one graph per project, so an edit shows up in all of them. **Open in new window**, on a tab or in the board menu,
moves a board, with its undo history, into a window of its own. A board can be open in only one window at a time;
opening it elsewhere brings its window forward. When you start Unfold again, your windows come back where you left
them, with their tabs.

## Branches and worktrees

A project's Home shows its branch. If the repository has several worktrees, the branch button switches the project
between them. The graph and your drafts belong to each worktree's folder, so they change when you switch, and the
published boards are the ones on that branch. Unfold reads Git's files to find these; it never runs Git.

## Background updates and the engine

While you use a project, Unfold notices when its files change (when you edit, switch branches, or pull) and updates
the graph in the background; open cards update by themselves. Projects you aren't using rest, and catch up when you
open them again. **Pause background updates**, in the project menu, stops the updates for that project; Home then
counts the files that changed since.

If Unfold's engine has to restart, a message says "Reconnecting to the engine…". Your boards stay open, and unsaved
edits are saved once it's back.

"Using fallback location" means the folder can't be written to (it's read-only or on a network drive), so Unfold
keeps the graph and drafts in its own app folder instead; publishing needs a writable folder.

## Broken files

A file that doesn't parse (say, halfway through an edit) doesn't break the graph. Unfold keeps the last version of
that file that did parse, and Home says so ("1 file doesn't parse · showing its last version that parsed"). When you
fix the file, it comes back to normal on the next update. A file Unfold can't read at all is listed in Health too.

Health lists only things that need attention: broken or unreadable files, settings errors, and boards that need
review. Calls Unfold can't follow are facts about the code, not problems.

## Settings

Settings for a project live in `unfold.toml`, which is shared with your team through the repository. Open
them with **Settings** on Home or in the project menu:

- **General:** the project's name, which boards of other projects use for it, and whether background updates are on.
- **Scope:** which folders are graphed. Unfold already skips what your ignore files skip, plus folders such as
  `node_modules`.
- **Detectors:** turn individual entrance detectors (Express routes, Celery tasks, and so on) on or off.
- **Python projects:** where a Python project's imports start, its source roots, and the Python version.
- **Evidence files:** files Unfold doesn't compile but boards may cite, such as SQL migrations or a Prisma schema
  (`db/migrations/*.sql`). The peek sheet shows them as text with line numbers.

Changes are saved together when you press **Save**. **Launches and manual entrances:** describe how your code starts
with **Add launch** on Home or **Add entrance…** in the project menu. **Starts a process** takes the command as you
deploy it and the folder it runs in; beside it, Unfold shows what it reads from the command (what it starts and where
its imports resolve from) and what saving changes. If Unfold can't read the command (such as `sh -c "…"`), set the
target and import paths under **Analysis details**. **Handles a route, queue, or event** declares a handler entrance:
its kind, trigger, and the functions that handle it, in order; **Add as entrance…** in a function card's menu starts
one from that function. Either can join an application, which only groups them on Home and in Add. Unfold writes each
to `unfold.toml` and never runs the command. Configured launches and entrances are edited or removed from their menu
on Home; a detected launch's **Edit…** changes only its name, application, schedule, and description.

**Remove from workspace**, in the project menu, takes a project out of Unfold. It never deletes the project's
`.unfold/` folder.

## Preferences

App-wide **Preferences** (⌘,, or the gear at the bottom of Home's sidebar) set the theme, the command that opens a file
in your own editor, whether background updates start on for new projects, how many threads a Compile uses, whether
Unfold checks for new versions (the only time it uses the network; nothing about you or your code is sent), and your
own key bindings, and whether agents can connect ([below](#connecting-an-agent)). **Copy Diagnostics**, in the Help menu
or the project menu, copies a summary for a bug report: versions and counts only, no code and no paths.

## Connecting an agent

Agents such as Claude Code and Codex connect to Unfold through its MCP server, which runs inside the app: they read the
guide to working in Unfold, what you're looking at, your projects and boards, and build pages that explain the code.

1. Open **Preferences**, turn on **Let agents connect** under **Agents**, and save. It's off until you do.
2. Copy the command for your agent (Preferences shows both, with the port you chose) and run it once:

   ```sh
   # Claude Code, for every repository
   claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp

   # Codex
   codex mcp add unfold --url http://127.0.0.1:29180/mcp
   ```

3. Ask your agent about a project. Unfold has to be running; if it wasn't when the agent's session started, reconnect
   from the agent's MCP menu (`/mcp` in Claude Code).

The server listens on port 29180 unless you choose another; Preferences says so when another program has the port.
It also lists the agents that connected, with when each last acted, and turning the switch off disconnects them all.
The server answers only programs on this computer, never web pages, and an agent can add a folder to your workspace
only after you confirm it in Unfold.

What an agent can set up and read:

- **Projects.** Add a folder (Unfold asks you first: "Claude wants to add `~/repos/payments` to your workspace."),
  compile it, and change its settings: scope, detectors, Python projects, manual entrances, the project's name,
  background updates, and evidence files. Settings live in `unfold.toml`, which your team shares, so an agent
  changes them only when you ask, and shows you the change.
- **Boards.** Create a draft, empty or traced from an entrance, in a background tab of the window you used last, so
  your view doesn't move; add, rename, move, copy, and delete pages; file boards into folders, rename them, and copy
  them. A change to a board is one undo step, and a page with your words on it is deleted only when you asked for
  that. When you ask to see something, or the work is done, the agent shows
  you the board, page, or card: the window comes forward with it selected, and Back returns you where you were.
- **Code.** Search names and text, list how a project starts, and read a function (its source, Flow steps, throws,
  and each call with where it goes and its label), a type's fields, an entrance's handlers, a call's contract, a file,
  and who calls what. Reading changes nothing and never moves your view, and an agent sees source only when it is
  the file Unfold compiled.

What an agent can build:

- **Pages.** Put functions, types, and entrances on a page by key, following calls, callers, or a type's fields to
  a depth (it says where following stopped); choose each card's view and fold; arrange the page or one frame; and
  take items off it. Then shape it: step groups that make a long Flow read in a few named steps, frames around
  units, services, and zones, tables, queues, and other resource cards, typed connections (HTTP, queues, events,
  reads and writes), and notes; and explain it: descriptions and Flow labels.
- **Grounded, every time.** A table, queue, or other resource card an agent adds (an actor aside), and an HTTP,
  RPC, queue, event, or runtime-call connection, must cite code (or lines of an evidence file) whose text names it,
  and Unfold refuses what it can't find there, so the agent says what it found in the chat instead of drawing it;
  checking the page flags any other connection that cites nothing. Everything an agent writes carries the agent
  mark.
- **One step at a time.** Each change is one undo step, saved at once.
  While it works, the page's top bar says "Claude is working…" and what it changed is outlined, until it finishes
  and its summary shows as a toast. It never replaces or removes your words unless you asked.
- **Reviews.** An agent reads a page (everything on it, with the arrows Unfold draws and the Flow steps still
  unlabeled), checks it (health problems and layout and wording issues, each with a fix), and looks at an image of
  it drawn the way you see it, without moving your view.
- **Repairs.** After a refactor, an agent can reconnect cards to renamed code and remove what's gone. It re-signs or
  accepts only what an agent wrote: what you wrote is yours to sign off, so it tells you instead.

Agents can't publish, discard, or delete boards, move them back to drafts, remove projects, change Preferences, or
edit your code.

How an agent learns this: when it connects, Unfold tells it what Unfold is and its rules, and it reads Unfold's
guide (how to work, how to build a page that reads well, how to draw connections and data models, what the graph
proves, how to write on a board, how to set things up, and how to repair) before it does each kind of job. So you
can ask in your own words. Before building a page it posts a short brief (the question the page answers, who it's
for, what it covers, and how it's shaped) so you can steer, and after each part it checks the page and looks at it.

In Claude Code, six recipes are also commands. Their arguments are single words (a route's method and path are two),
and you can leave them out for what you're looking at:

| Command | What you get |
| --- | --- |
| `/unfold:explain-entrance POST /orders` | A page that explains an entrance end to end, for someone new |
| `/unfold:simplify-function createOrder` | Step groups and labels until the function's Flow reads in a few steps |
| `/unfold:service-map ts-orders` | A page with a frame per service, its entrances, and how the services talk (several projects: `ts-orders,web`) |
| `/unfold:data-model` | The tables or types, their relationships, and which code reads and writes each table |
| `/unfold:tour` | An onboarding board: how the project starts, then a page for each main path |
| `/unfold:fix-boards` | Repairs after a refactor, and a list of what's yours to sign off |

## What to commit

| Path | Commit it? |
| --- | --- |
| `unfold.toml` | Yes: your team's shared settings. It appears only when you change a setting |
| `.unfold/boards/` | Yes: your published boards |
| `.unfold/local/` | No. It holds the graph cache, your drafts and unpublished changes, and view state, and it contains its own `.gitignore`, so Git ignores it automatically |

After a first Compile with default settings, Git sees nothing new. If your team would rather not share anything,
add `unfold.toml` and `.unfold/` to the repository's `.gitignore`; settings and published boards then simply stay on each machine. The graph cache
(`.unfold/local/cache/`) is safe to delete at any time; drafts are not, so leave `.unfold/local/drafts/` alone.
