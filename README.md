# Unfold

**Understand your code through connected diagrams.**

Unfold is a desktop app that turns TypeScript and Python source into diagrams you can
explore. Start at an HTTP route, worker, scheduled job, or script, follow its calls,
expand functions into steps and decisions, and inspect the actual source behind each
block.

Build explanations yourself, or connect Claude Code, Codex, or another MCP client to
create them with you. Boards can include architectural connections, tables and ERDs,
descriptions, and proposed changes. Save them alongside your code so your team can
read and maintain them.

Unfold analyzes source locally without executing the projects it reads. Its graph
updates as files change, and calls whose targets cannot be proven stay visibly
unresolved.

[Run the app](#run-from-source) · [Open a project](#open-your-first-project) ·
[Set up MCP](#connect-an-agent-with-mcp) · [Save and share](#save-and-share)

## Run from source

Install these prerequisites:

| Tool | Requirement |
| --- | --- |
| Node.js | 24 or newer, as specified in [package.json](package.json) |
| pnpm | The version pinned in [package.json](package.json), currently `11.9.0` |
| Rust | Install through rustup; [rust-toolchain.toml](rust-toolchain.toml) selects the toolchain |
| just | The command runner used by the [justfile](justfile) |
| Native build tools | Xcode Command Line Tools on macOS; Visual Studio C++ Build Tools on Windows; a C/C++ compiler and `make` on Linux |

You can install the pinned pnpm version with:

```sh
npm install --global pnpm@11.9.0
```

From this repository's root, install dependencies, build the engine and app, and launch:

```sh
pnpm install
just addon
just js-build
pnpm --filter @unfold/desktop start
```

To open a project at launch, pass its folder:

```sh
pnpm --filter @unfold/desktop start -- /absolute/path/to/your/project
```

For development with hot reload, use `just dev` after `pnpm install`. This recipe uses
Bash and Unix process utilities; the build-and-start commands above avoid that recipe
on Windows.

## Open your first project

1. Launch Unfold and click **Add project…**, choose **File → Add Project…**, or press
   **Cmd/Ctrl+O**. Select the folder containing your code. You can also drop a folder
   onto the window.
2. Review the files Unfold will read. It respects ignore files and skips folders such
   as `node_modules` and `.venv`. Exclude additional folders, such as build output,
   if needed.
3. Click **Compile**. This analyzes the code and creates `.unfold/` inside the project
   for its cache and boards. Adding the folder alone writes nothing to it.
4. On Home, find an entry point under **How it starts** and click **Trace** to open a
   board. Unfold detects launch scripts, Express and FastAPI routes, BullMQ and Celery
   workers, and node-cron and APScheduler schedules.
5. Follow calls and expand cards to explore the path. Background updates keep the
   graph current when you edit your code.

A **project** is a code folder. A **board** explains part of a system, and its
**pages** hold individual diagrams. Add more projects with the **+** beside Projects
in the sidebar; a page can contain code from several connected projects.

For a quick example, open [testdata/playgrounds/ts-orders](testdata/playgrounds/ts-orders)
or [testdata/playgrounds/py-orders](testdata/playgrounds/py-orders) and compile it.
These sample projects can be analyzed without installing their dependencies or
starting their services.

## Read and explain code

| What you want to do | How |
| --- | --- |
| Find a function, type, file, or entry point | Use **Add** on a board, or press **Cmd/Ctrl+K** |
| Follow a call | Click its link to open the called function beside it |
| Read a function's flow | Use the card's chevron to expand its steps, branches, and loops |
| Inspect the source behind a block | Select it and press **C**, or use **View code** / the `</>` button |
| Read the page as source | Switch the page lens from **Diagram** to **Code** |
| Simplify a long flow | Select consecutive steps with click and Shift-click, then right-click → **Combine…** |
| Explain a card or step | Add its description or label in the **Selected** sidebar |
| Clean up the layout | Click **Arrange**, then use **Fit** (**Shift+1**) |

Combined blocks retain their original steps and source. Hover a dashed call to see
why its target is unresolved. Types show their fields and written type information.

An MCP agent can also add frames around services, tables with columns and key markers,
ERD relationships, queues, notes, and connections between services. Authored
explanations and connections are visually distinct from compiled facts. When the code
behind an explanation changes, Unfold marks it for review.

If an entry point is missing, use **Add entrance…** in the project menu. Project
**Settings** controls analysis scope, detectors, Python import roots, and evidence
files such as SQL or Prisma schemas. The [user guide](docs/user-guide.md) covers the
full workflow.

## Connect an agent with MCP

Unfold includes a local [Model Context Protocol](https://modelcontextprotocol.io/)
server inside the desktop app. It lets an agent read compiled code, search projects,
and create or edit diagrams while you watch them update.

### 1. Enable the server

Open **Preferences** using the gear at the bottom of Home's sidebar. Under **Agents**,
turn on **Let agents connect** and click **Save**. Wait for **Listening**.

The default address is:

```text
http://127.0.0.1:29180/mcp
```

The transport is **Streamable HTTP**. Keep Unfold running while using MCP. The server
is off by default and accepts connections from programs on the same computer.

### 2. Configure your client

With your chosen agent's CLI installed, run its command once.

**Claude Code** — available across your repositories:

```sh
claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp
```

**Codex:**

```sh
codex mcp add unfold --url http://127.0.0.1:29180/mcp
```

In Codex, `codex mcp list` lists configured servers, and `/mcp` shows active servers
inside a session. See the [official Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp#configure-with-the-cli)
for client configuration details.

For another MCP client, add a server named `unfold` using **Streamable HTTP** and the
address above. If you change the port in Preferences, use the updated address and
connection command shown there.

Start a new agent session after configuring it. If a Claude Code session was already
running, use `/mcp` to reconnect. **Preferences → Agents → Connected agents** shows
clients that have connected.

### 3. Ask for a diagram

Open and compile your project, then ask the agent in your own words. Replace the
project, route, and function names in these examples with yours:

```text
Use Unfold to create an onboarding board for my payments project.
Show how it starts and add a page for each main request path.

Use Unfold to explain POST /orders end to end, including the worker
that processes the order and the tables it writes to.

On the current Unfold page, combine createOrder's steps into named
blocks and label them so a new developer can follow the flow.
```

To target a specific page, open its **⋯** menu in the sidebar, choose **Copy link**,
and paste the resulting `unfold://…` link into the agent's chat with your request.
These links identify items for the MCP tools.

The agent can read source, trace calls, build boards, arrange cards, add explanations,
and check diagrams. Its edits save as it works and can be undone in Unfold. An agent
adding a new project folder asks for confirmation in the app. Review the result and
press **Publish** yourself; agents cannot publish boards or edit your source through
Unfold's MCP tools.

Claude Code also exposes these built-in recipes:

| Command | Result |
| --- | --- |
| `/unfold:explain-entrance POST /orders` | Explain an entry point end to end |
| `/unfold:simplify-function createOrder` | Combine and label a function's flow |
| `/unfold:service-map payments` | Map services and their connections |
| `/unfold:data-model` | Show tables or types, relationships, and readers/writers |
| `/unfold:tour` | Create an onboarding board |
| `/unfold:fix-boards` | Review and repair diagrams after a refactor |

## Save and share

New boards are **drafts** and save automatically on your machine. Click **Publish**
to write a board to `.unfold/boards/<name>.json` in the project. Include that file in
your repository so teammates can open the same board in Unfold. Further edits remain
local until you publish again.

Use **Share → Export Mermaid…** or **Copy Mermaid** for a diagram you can embed in
GitHub Markdown, or **Share → Export PNG…** for an image.

| Path in the project | Purpose | Share in the repository? |
| --- | --- | --- |
| `unfold.toml` | Shared project settings; created when settings change | Yes |
| `.unfold/boards/` | Published boards | Yes |
| `.unfold/local/` | Graph cache, drafts, unpublished changes, and view state | No; automatically ignored |

## Troubleshooting

| Problem | What to check |
| --- | --- |
| The agent cannot connect | Keep Unfold open, enable **Let agents connect**, save, and check that Preferences says **Listening**. Start a new agent session or reconnect. |
| The MCP port is occupied | Choose another port in Preferences, save, and update the client's server URL to match. |
| An entry point is missing | Check project scope and detectors in **Settings**, or declare it with **Add entrance…**. |
| Code has changed since analysis | Let background updates finish, or click **Compile**. Check **Health and activity** for parse errors or update failures. |

## Further reading

- [User guide](docs/user-guide.md): navigation, shortcuts, editing, settings, and agent workflows.
- [Documentation map](docs/README.md): focused references for contributing to Unfold.
- [Architecture](docs/architecture.md): the Rust engine, Electron shell, and React UI.
- [Testing](docs/testing.md#choose-checks-for-a-local-change): choose checks for a change.
- [Desktop packaging](desktop/README.md#packaging): build the unpacked macOS app with `just bundle-dir`.
