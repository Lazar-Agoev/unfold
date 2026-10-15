# Unfold

**See how your code works, as connected diagrams of the code itself.**

Unfold is a macOS app that compiles TypeScript and Python into diagrams. Start where a system starts (an HTTP
route, a worker, a schedule, a command), follow its calls, expand functions into their steps, branches, and loops,
and open the source behind any block.

Explain a page yourself, or have Claude Code or Codex do it with you over MCP: name and combine steps, describe
cards, frame services, add tables and queues, connect them, and propose changes. Boards are files in your
repository, so your team reads the same diagrams.

Unfold reads code without running it. A call is drawn as resolved only when Unfold can prove its one target; the
rest stay visibly unknown. The graph updates as you edit.

## Run from source

| Tool | Version |
| --- | --- |
| macOS | With the Xcode Command Line Tools |
| Node.js | 24 or newer |
| pnpm | `11.9.0`, pinned in [package.json](package.json) (`npm install --global pnpm@11.9.0`) |
| Rust | Through rustup; [rust-toolchain.toml](rust-toolchain.toml) picks the toolchain |
| just | The command runner for the [justfile](justfile) |

```sh
pnpm install
just addon        # the Rust engine, as a Node addon
just js-build     # the UI and the Electron shell
pnpm --filter @unfold/desktop start
```

`just dev` runs the app with hot reload instead. It keeps its own app data, in
`~/Library/Application Support/Unfold Dev`, so it runs beside an installed Unfold with its own projects and settings.

## Open a project

1. Click **Add project** (or **File › Add Project…**) and pick a code folder. Unfold compiles it at once and keeps it
   current. It writes only inside the folder's `.unfold/`, and to `unfold.toml` when you or an agent change its
   settings.
2. **Home** shows the project: its state, **How it starts**, its **Boards**, and its **Health**.
3. Click an entrance under How it starts. Unfold makes a board with the entrance and its code, expanded in run order.

To try it, add [testdata/playgrounds/ts-orders](testdata/playgrounds/ts-orders) or
[testdata/playgrounds/py-orders](testdata/playgrounds/py-orders). Nothing needs installing.

### Say where your system starts

How it starts comes from `[[app]]` tables in `unfold.toml` at the folder's root. An app is a process you run, and
its entrances are what it handles. `code` names a function or module; `run` and `on` are shown as written.

```toml
[[app]]
name = "Server"
run = "npm run start"
code = "src/server.ts#"
entrances = [
  { name = "List orders", on = "GET /orders", code = "src/routes/orders.ts#listOrders" },
  { name = "Place an order", on = "POST /orders", code = ["src/routes/middleware.ts#requireUser", "src/routes/orders.ts#createOrder"] },
]
```

With no apps yet, Home offers **Add entrance** and **Copy agent prompt**, which asks an agent to write the tables.
A card's menu also has **Add as entrance…**. When code moves, the entrance shows an amber dot and, where it can,
a one-click fix.

## Read and explain

| To | Do |
| --- | --- |
| Put code on a page | **Add** at the bottom of the board, then type a name |
| Read a function | Expand its card; a call with a known target draws an arrow to its card |
| Follow a call in place | **Open in place** in the call's menu |
| See who calls a function | **Show callers** in the card menu |
| See builtin calls too | **Show all calls** in the top bar |
| Read the source | Select a block and click `</>`: the code panel shows its lines beside the canvas |
| Open it in your editor | **Open in editor** in the code panel; set the command in **Settings** |
| Name, describe, or combine steps | The block's menu: **Name…**, **Describe…**, **Combine…** |
| Propose a change | **Add proposed step after…** in the block's menu |
| Add a table, queue, or service | **Add resource…** in the canvas menu, then **Draw link…** |

When code under an explanation changes, the card asks for review. The [user guide](docs/user-guide.md) covers
everything in detail.

## Connect an agent

The app runs a local [Model Context Protocol](https://modelcontextprotocol.io/) server while it's open, at
`http://127.0.0.1:29180/mcp` (Streamable HTTP). It answers only programs on this machine. Run one command, once:

```sh
claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp   # Claude Code
codex mcp add unfold --url http://127.0.0.1:29180/mcp                            # Codex
```

**Settings › Agents** shows these commands and whether an agent is connected. Then ask in your own words:

```text
Use Unfold to explain POST /orders in my orders project, end to end.
```

To point an agent at a page, choose **Copy link** in the page's menu and paste the `unfold://…` link into the chat.
You watch the page change as the agent works, and each of its edits is one Edit › Undo. Agents never edit your
source; they change `unfold.toml` only when asked.

## What to commit

| Path | Holds | Commit it? |
| --- | --- | --- |
| `unfold.toml` | Settings, apps, and entrances | Yes |
| `.unfold/boards/` | Boards, saved as you edit | Yes |
| `.unfold/local/` | The graph cache, the lock, and empty board folders | No; it ignores itself |

**Export PNG…** in a board's menu saves an image of the page.

## Troubleshooting

| Problem | Check |
| --- | --- |
| An agent can't connect | Unfold is open, and Settings › Agents shows no error. Another program on port 29180 stops the server |
| An entrance is missing or marked | Hover its amber dot on Home for what happened and its fix |
| The diagram looks out of date | Home's state line; **Compile again** in the project menu; files that don't parse are listed under Health |

## More

- [User guide](docs/user-guide.md): the whole app.
- [Documentation map](docs/README.md): references for working on Unfold, starting with [AGENTS.md](AGENTS.md).
- [Testing](docs/testing.md#choose-checks-for-a-local-change): which checks to run.
- [Desktop](desktop/README.md#packaging): building the unpacked app with `just bundle-dir`.
