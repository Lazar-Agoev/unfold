# Unfold

Unfold is a desktop app for reading a codebase as a graph. You open a folder and press **Compile**. Unfold shows the ways the code can start (HTTP routes, queue workers, schedules, commands, and launch scripts) and lets you follow each one, call by call, as cards on a board. Each card shows a function either as its exact source (**Code**) or as a diagram of its structure (**Flow**). You can explain a path with descriptions and labels, then publish the board into the repository. After the first Compile, the graph keeps itself current in the background. Unfold reads TypeScript and Python, and it never runs the code it reads.

**Status:** design. This repository holds the plan, and there is no code yet. Work starts with Phase 0 of the [migration plan](docs/migration-plan.md).

## The docs

| Doc | Read it for | What it explains |
| --- | --- | --- |
| [Architecture](docs/architecture.md) | What we're building. Start here | The product and its guarantees, the graph model, and the Rust engine (oxc for TypeScript, Ruff's parser for Python). How Compile and background updates work, the `.unfold/` folder and the SQLite graph, and the board and settings formats. The reader, the window, the commands between the UI and the engine, and the repository layout. Testing, performance targets, and risks |
| [Decisions and trade-offs](docs/decisions-and-tradeoffs.md) | Why it's built this way | How the design came about, then every decision (U-1 to U-36) with the options we weighed and their pros and cons, such as Rust and Tauri versus TypeScript and Electron. It ends with what we rejected and what's still open |
| [Migration plan](docs/migration-plan.md) | The order we build it in | What moves over from the proof of concept and what stays behind. The phases: 0–7 lead to V1, and 8–11 to V2. What V1 and V2 contain, the gates each phase must pass, and what we do if a risky part fails |

## The proof of concept

Unfold rebuilds a proof of concept, [`graph-compiler`](../graph-compiler), which sits next to this repository at `~/Desktop/repos/graph-compiler`. Its product was called Codegraph. It was built to prove one idea: that TypeScript and Python can be compiled, without running them, into a graph of the code that people can trust and read. It proved it. It was never deployed, and it is now a frozen reference.

**What it built**

- **A compiler** that reads TypeScript and Python and records three things:
  - the code's structure: functions, classes, branches, loops, and calls;
  - what each call targets;
  - how the code can start, from package scripts and bins, Express, BullMQ, node-cron, Python console scripts, FastAPI, Celery, APScheduler, and manual declarations.

  A call is linked only when there is proof of exactly one target. Otherwise, the graph records why the call can't be followed.
- **A reader** that draws the graph as cards, checks each file's hash before showing its code, and saves boards of explanations.
- **Evidence that the approach holds.** 854 compiler tests, 57 reader tests, and 3 browser tests passed, and every linked call was checked against an independent oracle: the TypeScript type checker or Pyright.

**How it was built differently**

| Proof of concept | Unfold |
| --- | --- |
| A compiler CLI and a separate browser reader, joined by files and a local HTTP server | One desktop app (Tauri 2) with the engine inside it |
| Written in TypeScript. It used a fork of TypeScript 6, a second TypeScript analyzer backed by the type checker, and vendored Pyright | A Rust engine, using oxc for TypeScript and Ruff's parser for Python |
| The graph written as CSV files into `.codecodegraph/`, with drafts and boards in two more folders at the repository root | One `.unfold/` folder: shared settings and published boards, plus a local graph cache and drafts that Git ignores |
| Re-index by hand after changes | Compile once, then background updates as files change |
| One sidebar with a flat list of definitions | An IDE-style window: files, entrances, search, boards, tabs, and an inspector |

**What carries over**

- Its rules: link a call only with proof, record only what the code declares, show code only after verifying it, produce the same graph from the same input, and never run anything.
- Its reader design.
- Its playgrounds and fixtures.
- Its frozen outputs, which become Unfold's test reference.

Its code doesn't carry over. This repository rebuilds it from scratch in its new form: one desktop app, a Rust engine, and a graph that keeps itself current.
