# Using Unfold

Unfold helps you read a codebase by how it starts and where it goes. It reads TypeScript and Python, runs on macOS,
and never runs your code, installs anything, or runs Git.

## Projects and boards

Everything you add lives in one **workspace**. Each **project** in it is a folder, usually a repository, with its own
graph. You explain code on **boards**: a board is a story, such as "Order lifecycle", and holds **pages**, each one
view of it, such as "Placing an order". A page holds **cards**: an entrance, a function, a class, a type, or a module.
Folders group boards, one level deep.

## Add a project

The first time you open Unfold, press **Add project** and pick a folder (also **File › Add Project…**, or **Add
project** at the bottom of the sidebar). It compiles at once: Unfold reads the files, works out what calls what, and
keeps the graph current from then on as you edit. You can add up to 64 projects.

Unfold keeps its graph and your boards in a `.unfold/` folder inside the project, so it needs to write there. A
read-only folder, or one on a network volume, is refused with a message saying so.

## Home

Home shows the project selected in the sidebar. Under its name, the state line says where things stand: "Graph current
· 84 files · TypeScript + Python · updated 2 min ago", "Compiling", "Updating · 2 files changed", or "Resting" for a
project you haven't looked at in a while (it catches up when you open it). The project's row in the sidebar shows the
same state as a small mark.

- **How it starts** lists the project's apps and what each one handles ([below](#declare-how-it-starts)). Click a row
  to trace it onto a new board.
- **Boards** lists the project's boards, most recently edited first. **New board** makes one.
- **Health** says whether anything needs a look: a file that doesn't parse, a board whose code changed, or an
  entrance whose code isn't found. Click an amber item to go to it.

The project menu (`⋯` on its row or on Home) has **Compile again**, **Cancel** while compiling, **Open
`unfold.toml`**, and **Remove from workspace…**, which never deletes the project's `.unfold/` folder.

## Declare how it starts

Unfold doesn't guess how your code starts. You write it down in `unfold.toml` at the project's root, as `[[app]]`
tables: an **app** is something you run (a server, a worker, a command-line tool), and an **entrance** is something an
app handles (a route, a queue, a schedule, a signal).

```toml
[[app]]
name = "Server"
run = "npm run start"
code = "src/server.ts#"
entrances = [
  { name = "Health check", on = "GET /health", code = "src/server.ts#healthCheck" },
  { name = "Place an order", on = "POST /orders", code = ["src/routes/middleware.ts#requireUser", "src/routes/orders.ts#createOrder"] },
  { name = "Shut down", on = "SIGTERM", code = "src/server.ts#shutdown" },
]

[[app]]
name = "Worker"
run = "npm run worker"
code = "src/worker.ts#"
entrances = [
  { name = "Process an order", on = "queue orders", code = "src/jobs/process-order.ts#processOrder" },
]
```

- `name` is required, and so is an entrance's `code`. `run` and `on` are only shown; Unfold never runs or parses them.
  `about` adds one sentence.
- `code` is where it starts: a module (`src/server.ts#`), a definition (`src/routes/orders.ts#createOrder`), an
  inline handler, or a list of them in the order they run. **Add as entrance…** on a card writes the key for you.
- Editing these tables never recompiles: Unfold only looks the keys up again.

You don't have to edit the file by hand:

- **Add as entrance…**, in a module, function, or class card's menu, asks for the app (or a new one), the name, and
  `on`, and writes one line.
- **Add entrance**, in How it starts' header (or its empty state), does the same, with a name search for the code.
- **Copy agent prompt** copies a request you can paste into Claude Code or Codex to write the tables for you.

When a `code` key stops matching, its row on Home gets an amber dot. Hover it to see what happened: **Moved to …** or
**Route edited** offers **Fix**, which rewrites that one key; **Not found** and **Not a start** offer **Choose
code…**; **Not analyzed** (the file is outside the graph) opens `unfold.toml`, where `[scope]` is. Unfold never applies
a fix by itself, and a fix changes nothing else in the file: if the key changed since you looked, it says so and writes
nothing. If `unfold.toml` stops parsing, the last good settings and apps stay and the error is shown.

## Boards and pages

Open a board from Home or the sidebar; the sidebar then shows the board:

- **The header:** back to Home, the board's title (double-click to rename), and the board menu: **Rename…**, **Move to
  folder ›**, **Export PNG…**, **Copy link**, and **Delete…**.
- **Pages:** **+** adds one. A page's menu has **Rename**, **Copy link**, **Move up**, **Move down**, and **Delete
  page…**.
- **Selected:** what you selected ([below](#selected)).

The bar above the canvas shows the page's name and the **Show all calls** switch. The dock at the bottom has **Add**,
**Arrange**, and the zoom. Right-click the canvas for **Add resource…** and **Fit**.

Boards are saved as you edit, as files in `.unfold/boards/`. Undo and Redo are in the Edit menu (⌘Z, ⇧⌘Z), one history
per board, across its pages. If a save fails, the canvas says **Not saved · Retry**; your work stays in the window.
If the file changes outside Unfold (an agent, your editor, a `git pull`), the board reloads from it and its undo
history starts over. If you have edits that aren't saved yet, Unfold stops saving and asks: **Keep my edits** writes
yours over the file, and **Load the file** takes the file's version. If the file isn't a valid board, such as after a
merge conflict, saving stops until you choose **Save my version** or fix the file; once it's a valid board again, the
board reloads by itself, or asks if you edited meanwhile. Nothing either side wrote is lost without you choosing.

In the sidebar, a folder's menu has **New Board Here**, **Rename…**, and **Delete** (when it's empty).

## Put code on a page

- **Trace an entrance:** click it in How it starts. Unfold makes a board named after it, with the entrance's card and
  the cards for its code, expanded, in the order they run.
- **Add:** press **Add** in the dock and type a name. Functions, classes, types, modules, apps, and entrances of the
  board's project come first, then files. Enter puts the card on the page, or opens a file in the code panel.
- **Show callers**, in a card's menu or in Selected, adds the functions that call it, in a column to its left.
- **From the code panel:** `⊕` beside a definition adds its card.

When a function and the function it calls are both on the page, an arrow joins them: from the call on an expanded
card, or from the edge of a compact one. An entrance's card points at the cards for its code.

## Read a card

A card is **compact** (its name and where it is) or **expanded**; the chevron in its header switches. Expanded, it
shows its signature and docstring, then the **drawn flow**: what the function does, in the order it runs.

- **Calls into your project** are boxes, with what their value is assigned to (`→ order`). **AWAIT** marks an awaited
  call; **SOMETIMES**, dashed, one that runs only sometimes (on the right of `&&`, `||`, `??`, `and`, `or`, or after `?.`).
- **Open in place:** the chevron on a call box draws the callee's flow inside the card, up to three levels deep.
- **Other calls** (libraries, and calls Unfold can't follow) gather into thin rows such as
  `· ordersQueue.add → job, res.status().json ?`. A `?` marks a call Unfold can't follow, and the row counts them.
  The ones written in a test or a loop's header are listed after it, each with its `?`, and the ones written inside a
  call drawn on a test or an exit are listed on it (`+ x.foo ?`).
- **Builtin calls** (`len`, `JSON.parse`) are hidden. **Show all calls** in the top bar shows them on every card.
- **Decisions** draw a diamond per test, with each branch as a lane. A test too long to draw whole ends in `…`; hover
  it to read all of it. **Loops** are frames titled by their header, and a **with** block by WITH and what it
  opens (`WITH open(path) as f`); **try** shows TRY with dashed "on error" arrows to its EXCEPT (or CATCH) sections,
  each titled by its clause; exits say where they go, the last one too (`RETURN order → End`).
- **`⋯ 2 lines`** on an arrow means code is left out there (assignments and other lines that call nothing). Click it
  to read those lines. It names a loop or branch that has nothing drawn inside (`⋯ 3 lines · for loop`).
- **"never runs"**, dimmed, marks code nothing reaches, such as code after a `return`. **"may continue"**, dashed,
  marks a place Unfold couldn't follow to the end.

If a card's file changed since the last compile, or doesn't parse, a line under its header says so, and the card keeps
showing what the last compile read until the graph catches up.

A class's card shows its fields when compact and its class-level code when expanded; its methods are cards of their
own. A type's card shows its fields, inherited fields included.

## The code panel

Source opens in the code panel on the right, beside the diagram. Click `</>` on a block or a card's header, choose
**View code** from a block's menu, click a gap tick, or open a file from Add.

- It shows the block's own lines. A branch shows its whole `if`, `switch`, or `try` with the branch highlighted; a call
  shows its whole statement with the call marked. An entrance's card shows its lines in `unfold.toml`.
- The path above the code widens the view: click a step to see the whole function, or the file. Your selection stays
  marked.
- While it's open, the panel follows what you select, and the block it shows is outlined on the canvas. Hover a line
  to light up its block; click a line to select it.
- **Open in editor** opens the file at that line (see [Settings](#settings)). × or Escape closes the panel.
- If the file changed since the last compile, or doesn't parse, the panel shows the whole file as it is now and marks
  nothing until the graph catches up. If the block's file is now outside the graph (`[scope]` leaves it out), the panel
  shows the whole file marked **Not analyzed**.

## Selected

Selected, at the bottom of the board's sidebar, shows what you clicked: a card's description, who calls it (**Called
by**), and which entrances reach it (**Reached from**); a block's name; a combined step; a connection; a frame; a
resource card; a note; or a proposed step.

Click an **arrow** to see the call's contract: what it sends (each argument beside the parameter it fills), what comes
back and where it goes, and what the callee raises ("caught" when a handler around the call catches it). Click a type
name to open its definition in the code panel.

## Explain it

Right-click a block in an expanded card for its menu:

- **Name…** gives a block your own words; the code's name stays beside it. A name applies on every card of the page
  that draws the block. Naming a library call lifts it out of its thin row.
- **Describe…** writes one sentence on what a card does and why.
- **Combine…** puts consecutive steps under one title, or a whole `if`, loop, or `try`. It draws as one step that keeps
  its ways out (`returns → End`) and counts its unknown calls; open it to see the steps. **Uncombine** undoes it.
- **Add proposed step after…** or **at the start…** draws an intended change, dashed as PROPOSED, with a title and a
  reason. It's never presented as code.

On the canvas:

- **Add resource…** (right-click the canvas) adds a table, queue, cache, storage, outside service, or actor. A table
  takes columns with key marks, or the columns of a type (**From a type…**).
- **Draw link…** (a card's, resource card's, or frame's menu) draws a labeled line to another item. From a call's menu
  in an expanded card, the line leaves from that call. **Kind ▸** on the line makes it HTTP, Queue, Reads, Writes, Calls
  (where an unknown call goes at runtime: draw it from the unknown call), Relates, or Other; a relationship's columns
  and cardinality are set in Selected.
- Frames and notes are added by agents; you can rename, describe, move, collapse, and remove them.

Everything you write is signed with your name, and against the code it describes.

## Keep it true

When code under a description, name, or combined step changes, its card shows "Changed since it was described ·
Review". **Review** offers **Accept** (still true), **Reconnect…** (the code was renamed or moved: pick its new name),
and **Remove from page**. Unfold never moves your words to other code by itself. A call written inside a combined
step joins it and sends it to review; **Accept** keeps the call in the step. A page with something to review has
a mark in Pages, and Home's Health line counts the boards that need review. An entrance's card keeps up
too: its description goes to review when its code is found or changes, and an entrance renamed or removed in
`unfold.toml` shows as missing at once, with **Reconnect…** offering the entrance that has the same code.

Unfold updates the graph in the background as files change. A file that stops parsing keeps its last good facts,
and Health lists it with **Open in editor**.

## Settings

The gear at the bottom of the sidebar opens Settings:

- **Editor command:** how **Open in editor** opens a file, such as `code -g {path}:{line}`. `{path}`, `{line}`, and
  `{column}` are replaced. Empty opens files in the default text editor. If the program isn't found, use its full path:
  an app started from the Dock gets a short `PATH`.
- **Agents:** the commands that connect Claude Code and Codex, and whether an agent has connected.

## Connect an agent

Agents such as Claude Code and Codex connect to Unfold's MCP server, which runs whenever Unfold is open. Run the
command for your agent once (Settings shows both, with a copy button):

```sh
# Claude Code, for every repository
claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp

# Codex
codex mcp add unfold --url http://127.0.0.1:29180/mcp
```

Then copy a page's link (**Copy link** in a page's or board's menu) into your agent's chat and ask it to diagram
something. If Unfold wasn't open when the agent's session started, reconnect from the agent's MCP menu (`/mcp` in
Claude Code). If another program holds port 29180, Settings says so.

The server answers only programs on this computer, never web pages. An agent works with the same actions you have:

- It reads what Unfold compiled: entrances, a function's blocks, a call's contract, files, and pages.
- It builds pages: adds cards, names and combines steps, describes cards, opens calls in place, frames services, adds
  tables and queues, draws connections, and adds notes. Each change is one undo step, saved at once.
- While it works, the top bar says "Claude is working…" and what it changed is outlined; when it finishes, its summary
  shows in a toast. Your view doesn't move unless it shows you something.
- Every resource card and connection it draws cites the code it comes from, and Unfold refuses what it can't find
  there. A call Unfold can't follow stays marked `?`. Its frames and notes carry its mark (`✦ Claude`), and Selected
  says who wrote any description ("by ✦ Claude", "by you").
- It writes `[[app]]` tables and settings in `unfold.toml` only when you ask, and adds a folder to your workspace only
  after you confirm it in Unfold.
- It never edits code, deletes boards, removes projects, changes Settings, or replaces your words unless you asked.

## What to commit

| Path | Commit it? |
| --- | --- |
| `unfold.toml` | Yes: your team's settings and apps |
| `.unfold/boards/` | Yes: your boards. Git is how a team shares them |
| `.unfold/local/` | No. It holds the graph cache and this machine's state, with its own `.gitignore`, so Git ignores it |

The graph cache (`.unfold/local/cache/`) is safe to delete at any time; Unfold rebuilds it.
