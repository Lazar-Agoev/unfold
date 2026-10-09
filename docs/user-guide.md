# Using Unfold

Unfold helps you read a codebase by following how it starts and where it goes. It reads TypeScript and Python. It
never runs your code, installs anything, or uses Git. Shortcuts below are for macOS; on Windows and Linux use Ctrl
where you see ⌘. The full list is in [the keyboard map](ui-shell.md#keyboard).

## Open a folder

Press **Open Folder…** (⌘O) on the welcome screen, pick a recent folder, or drop a folder onto the window. Opening
and browsing a folder writes nothing to it.

## The first Compile

A folder you haven't compiled before shows a short summary first: how many TypeScript and Python files Unfold will
read, and which folders it skips (for example `node_modules`, `dist`, or anything your `.gitignore` ignores). For a
large repository, **Choose folders…** lets you pick just the parts you care about.

Press **Compile** (⇧⌘B). Unfold reads the files, works out what calls what, and finds the ways the code starts. It
creates a `.unfold/` folder in your repository to keep the graph and your boards. From then on the graph keeps
itself current while the folder is open: you don't need to compile again after editing.

## Entrances

The **Entrances** view (⌘2) lists the ways your code can start:

- launch scripts and commands from `package.json` and `pyproject.toml`;
- HTTP routes (Express, FastAPI);
- queue workers and tasks (BullMQ, Celery);
- schedules (node-cron, APScheduler);
- entrances you declare yourself ([below](#settings)).

They are grouped by application, named after how each one starts (`server`, `worker`). Click an entrance to open
it as a card on a board, with the functions that handle it.

## Following calls

Each function on a board is a card. In a card, calls you can follow are links:

- **hover** a call to see what it calls and where;
- **click** it to open that function's card next to this one;
- **⌥-click** it to read the called function inline, without opening a card.

A call Unfold can't follow has a dashed underline; hover it to see why (for example, the target depends on a value
only known when the program runs). A call into a library shows the library's name. Unfold only links a call when it
can prove where it goes, so a link is always right.

Use **Back** (⌘[) and **Forward** (⌘]) to retrace your steps, **Fit Board** (⇧1) to see everything, and ⌘= / ⌘- to
zoom. You can also open any file as a read-only **source tab** (double-click it in Files, or ⌘P), click its calls, and
use the `⊕` beside a function to add it to the board. The **References** tab in the bottom panel shows who calls the
selected function and which entrances reach it.

## Code and Flow

Every card shows a function one of two ways:

- **Code**, its exact source, with folds and links;
- **Flow**, a diagram of its steps, branches, and loops.

The **Code | Flow** switch in the title bar sets the default for the window. Each card also has its own switch,
which is saved with the board.

## Describing, labeling, and grouping

Boards are for explaining a path to someone else (or to yourself later). Open the inspector (⌥⌘B, or `]`) and select:

- a **card**, to write a description of what the function does;
- a **Flow node**, to give it a plain-language label in place of its code ("Queue the order" instead of
  `ordersQueue.add(...)`). The Boards view counts the nodes that still show code and steps you through them.

To **group** lines, drag over the line numbers of a Code card and give the group a title; it shows as a colored rail
beside those lines. Groups can be described, collapsed, and ungrouped from the card's menu.

Everything you write is signed with your name. ⌘Z and ⇧⌘Z undo and redo board edits.

**When the code changes.** Descriptions and labels remember the code they were written for. If that code changes,
they are marked **outdated** (never silently deleted), and the Boards view and Problems list them with ways to
repair them.

## Drafts and Publish

A new board is a **draft**: it lives only on your machine, under `.unfold/local/drafts/`, and saves as you work.

When a board is ready to share, press **Publish**. Unfold writes it to `.unfold/boards/<name>.json` inside the
repository. Commit that file like code; teammates who pull it see the board in their Boards view, and later edits
show up in `git diff`. **Move Back to Drafts** in the board menu (`⋯`) takes a board out of the repository again.

## Export Mermaid

From the board menu (`⋯`), **Export Mermaid…** saves the board as a Mermaid diagram (`.mmd`), which renders in
GitHub, GitLab, and many wikis; **Copy Mermaid** puts it on the clipboard instead.

## Several windows

You can open as many windows as you like:

- **New Window** (⇧⌘N) opens another window on the same folder, so you can put two boards side by side or on two
  screens. Windows on the same folder share one graph, so an edit shows up in all of them.
- **Move to New Window** (right-click a board's tab) moves a board, with its undo history, into a window of its own.
- **Open Folder in New Window…** opens a different repository beside the current one.

A board can be open in only one window at a time; opening it elsewhere brings its window forward. When you reopen a
folder, its windows come back where you left them, with their tabs.

## Background updates and the status bar

While a folder is open, Unfold notices when files change (when you edit, switch branches, or pull) and updates the
graph in the background. Open cards update by themselves. The left end of the status bar tells you where things
stand:

| You see | It means |
| --- | --- |
| **● Graph current · 84 files** | Everything is up to date |
| **◌ Compiling…** or **◌ Updating · 3 files changed** | Unfold is catching up |
| **● 1 file doesn't parse** (amber) | A file has a syntax error; see [Broken files](#broken-files) |
| **● Background updates paused** | Updates are off for this folder; it counts the files changed since |
| **▲ Compile failed · see Problems** | Something went wrong; the previous graph stays, and Problems says why |
| **Reconnecting…** | Unfold's engine is restarting; your boards are kept |

Click the status to **Compile**, **Rebuild from scratch**, **Pause** or **Resume background updates**, show the
**Index log** (what the last update did), or **Copy Diagnostics** for a bug report (versions and counts only: no code
and no paths). "Using fallback location" means the folder can't be written to (it's read-only or on a network
drive), so Unfold keeps the graph and drafts in its own app folder instead; publishing needs a writable folder.

## Broken files

A file that doesn't parse (say, half-way through an edit) doesn't break the graph. Unfold keeps showing the last
version of that file that did parse, dimmed, marks it in Files, and lists it in **Problems** (⌘J). When you fix the
file, it comes back to normal on the next update. A file Unfold can't read at all is listed in Problems too.

Problems lists only things that need attention: broken or unreadable files, settings errors, and boards that need
review. Calls Unfold can't follow are facts about the code, not problems.

## Settings

Settings for a folder live in `.unfold/config.toml`, which is shared with your team through the repository. You
rarely need to edit it by hand:

- **What gets graphed.** Right-click a folder in Files for **Exclude from graph**, **Include in graph**, or **Graph
  only this folder**. Unfold already skips what your ignore files skip, plus folders such as `node_modules`.
- **Manual entrances.** If your code starts in a way Unfold doesn't detect, add it yourself: **Add Entrance…** in the
  Entrances view's `…` menu, or right-click a function and choose **Add as Entrance…**. You pick the kind (command, HTTP
  route, worker, schedule, event, or process), its trigger, and the functions that handle it.
- **Folder Settings…** (in the folder menu at the top left) sets up **Python projects** (where a project's imports
  start, its source roots, and the Python version), turns individual **detectors** on or off, and lists the scope
  rules.

App-wide **Preferences** (⌘,) set the theme, the command that opens a file in your own editor, whether background
updates start on for new folders, how many threads a Compile uses, and whether Unfold checks for new versions (the
only time it uses the network; nothing about you or your code is sent).

## What to commit

| Path | Commit it? |
| --- | --- |
| `.unfold/config.toml` | Yes: your team's shared settings. It appears only when you change a setting |
| `.unfold/boards/` | Yes: your published boards |
| `.unfold/local/` | No. It holds the graph cache, your drafts, and window positions, and it contains its own `.gitignore`, so Git ignores it automatically |

After a first Compile with default settings, Git sees nothing new. If your team would rather not share anything,
add `.unfold/` to the repository's `.gitignore`; published boards then simply stay on each machine. The graph cache
(`.unfold/local/cache/`) is safe to delete at any time; drafts are not, so leave `.unfold/local/drafts/` alone.
