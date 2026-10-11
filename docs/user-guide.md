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
sidebar shows the same thing as one small mark ([every state](architecture.md#79-what-a-projects-state-says)).
The project menu (`⋯` on its row or on Home) has Compile, Rebuild from scratch, Pause background updates, Settings,
Health and activity, and Remove from workspace.

**Recents** and **Drafts**, at the top of the sidebar, show boards from every project: the ones you opened
recently, and every draft and unpublished change on this machine.

## Boards and pages

A board opens in a tab at the top of the window, and the sidebar switches to it:

- **Pages:** the board's pages. **+** adds one (⌥⌘N); a page's `⋯` menu renames, duplicates, reorders, or deletes
  it.
- **On this page:** the cards in the order you followed them, indented by call depth, with their groups and links.
- **Selected:** the card you selected: where it is, its description, who calls it, which entrances reach it, and the
  calls you haven't opened yet (**+ Open**).

The bar above the canvas shows the page, whether the board is a draft or published, the **Code | Flow** switch,
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

## Code and Flow

Every card shows a function one of two ways:

- **Code**, its exact source, with folds and links;
- **Flow**, a diagram of its steps, branches, and loops.

The **Code | Flow** switch above the canvas (⌘1, ⌘2) sets it for the page. Each card also has its own switch, which
is saved with the board.

## Add

**Add** (`A`, ⌘K, or `/`) finds code in every project and puts it on the page:

- Before you type, it shows every project's applications and their entrances, with the ones already on the page
  marked.
- As you type, it finds entrances, functions, files, and text, with a tab for each and a preview on the right.
  Functions include classes and types (`interface Cart`, `type CartId`, `enum Currency`); a type goes on the page
  showing its code. On the Text tab you can match case, whole words, or a regular expression.
- **Enter** adds the selection to the page, and **⌘Enter** adds it on a new page. A file or a text match opens in the
  peek sheet instead. From Home, Enter starts a new board.
- Type `>` first to run a command instead: everything the app can do is listed there, with its shortcut. ⇧⌘P opens
  Add this way, ⌘P on Files, and ⌘T on Functions.

## Reading a whole file

A file opens in the **peek sheet**, on the right of the canvas: the whole file, read-only, with its calls as links.
The `⊕` beside a function, class, or type adds it to the page, and so does a click on a call. Pin the sheet to keep it open while
you work; Escape closes it. Its menu opens the file in your own editor (⇧⌘O) or shows it in Finder.

## Describing, labeling, and grouping

Boards are for explaining a path to someone else (or to yourself later):

- Select a **card** and write what the function does in Selected, or double-click the description strip on the card.
- Select a **Flow node** and give it a plain-language label in place of its code ("Queue the order" instead of
  `ordersQueue.add(...)`). "N Flow steps unlabeled · Label them", under On this page, steps you through the nodes
  that still show code.
- To **group** lines, drag over the line numbers of a Code card and give the group a title; it shows as a colored
  rail beside those lines. In Flow the same group is a **step group**: a titled region around its steps. Its chevron
  collapses it, in Code and Flow alike: in Flow it becomes one step with the group's title, marked when something
  inside can return or throw. Right-click a group to describe it or ungroup it.

Everything you write is signed with your name. ⌘Z and ⇧⌘Z undo and redo board edits, on any page.

**When the code changes.** Descriptions and labels remember the code they were written for. If that code changes,
the card shows "Changed since it was described · Review", which lists what happened and offers repairs. Nothing is
ever silently deleted.

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
a Mermaid diagram (`.mmd`), which renders in GitHub, GitLab, and many wikis, or copies it.

## Boards across projects

A page can hold cards from any project in your workspace, for example the web app's checkout handler next to the
API route it calls: add them with Add, which searches every project. The graph never links code across
repositories, so draw the link yourself: **Draw link…** in a card's menu, then click the other card, and give the
link a label such as "calls over HTTP".

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

Settings for a project live in `.unfold/config.toml`, which is shared with your team through the repository. Open
them with **Settings** on Home or in the project menu:

- **General:** the project's name, which boards of other projects use for it, and whether background updates are on.
- **Scope:** which folders are graphed. Unfold already skips what your ignore files skip, plus folders such as
  `node_modules`.
- **Detectors:** turn individual entrance detectors (Express routes, Celery tasks, and so on) on or off.
- **Python projects:** where a Python project's imports start, its source roots, and the Python version.

Changes are saved together when you press **Save**. **Manual entrances:** if your code starts in a way Unfold
doesn't detect, add it yourself with **Add entrance…** (on Home or in the project menu), or **Add as entrance…** in a
function card's menu. You pick the kind (command, HTTP route, worker, schedule, event, or process), its trigger, and
the functions that handle it. Manual entrances can be edited or removed from their menu on Home.

**Remove from workspace**, in the project menu, takes a project out of Unfold. It never deletes the project's
`.unfold/` folder.

## Preferences

App-wide **Preferences** (⌘,, or the gear at the bottom of Home's sidebar) set the theme, the command that opens a file
in your own editor, whether background updates start on for new projects, how many threads a Compile uses, whether
Unfold checks for new versions (the only time it uses the network; nothing about you or your code is sent), and your
own key bindings. **Copy Diagnostics**, in the Help menu or the project menu, copies a summary for a bug report:
versions and counts only, no code and no paths.

## What to commit

| Path | Commit it? |
| --- | --- |
| `.unfold/config.toml` | Yes: your team's shared settings. It appears only when you change a setting |
| `.unfold/boards/` | Yes: your published boards |
| `.unfold/local/` | No. It holds the graph cache, your drafts and unpublished changes, and view state, and it contains its own `.gitignore`, so Git ignores it automatically |

After a first Compile with default settings, Git sees nothing new. If your team would rather not share anything,
add `.unfold/` to the repository's `.gitignore`; published boards then simply stay on each machine. The graph cache
(`.unfold/local/cache/`) is safe to delete at any time; drafts are not, so leave `.unfold/local/drafts/` alone.
