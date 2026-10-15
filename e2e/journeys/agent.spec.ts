/**
 * An agent's session (docs/ipc.md#agents), end to end on a copy of
 * `ts-orders`: an MCP client connects to the app's server as Claude Code, reads the guide, the
 * entrances, and the compiled tree; traces "Place an order" onto a new board, which the person opens
 * while "Claude is working…" and what it changed is outlined; shapes createOrder in one call (a name, a
 * combined step, a call opened in place), which Edit › Undo takes off in one step and Redo puts back;
 * frames the service, adds the customer, connects it over HTTP citing the entrance, and adds a note;
 * checks and finishes, which clears the marks and shows its summary; and, because the person asks,
 * declares an entrance in `unfold.toml`, one line.
 */

import { join } from "node:path";
import type { Locator } from "@playwright/test";
import { AGENT_ADDRESS, AgentClient, revisionOf } from "../src/agents";
import { boardView, card, cards, expectCardsLoaded, showBoard } from "../src/board";
import { savedPage } from "../src/explain";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { homeMain, showHome, sidebarBoard, startRow, toast } from "../src/home";
import { readFile } from "../src/sandbox";

const BOARD = "Placing an order";
const BRANCH = "Asked to answer later?";
const COMBINED = "Announce it and reply 201";
const DESCRIPTION = "Places the order now, or queues it when the client asks for a later answer.";
const FRAME = "Orders API";
const FRAME_ABOUT = "Checks the user and the order, then places it.";
const NOTE = "Queued orders are placed by the worker. The client gets a job id, not the order.";
const SUMMARY = "Traced placing an order and shaped createOrder.";

/** The first group `pattern` finds in a tool's answer, failing loudly when it isn't there. */
function found(text: string, pattern: RegExp): string {
  const match = pattern.exec(text)?.[1];
  if (match === undefined) {
    throw new Error(`no ${String(pattern)} in:\n${text}`);
  }
  return match;
}

/** The drawn flow at the end of a `shape_card` answer, each line without its keys. */
function shapedLines(text: string): string[] {
  const [, flow = ""] = text.split("The card as the person sees it now:\n");
  return flow
    .split("\n")
    .filter((line) => line.startsWith("(") || line.startsWith(" "))
    .map((line) => line.replace(/\s+key=.*$/, ""));
}

/** Whether the canvas outlines a card, resource card, frame, or note as what a working agent changed. */
async function expectWorkingOutline(item: Locator, outlined = true): Promise<void> {
  const node = item.locator("xpath=ancestor::div[contains(@class, 'react-flow__node')][1]");
  await (outlined ? expect(node).toHaveClass(/\bagent-working\b/) : expect(node).not.toHaveClass(/\bagent-working\b/));
}

test("an agent traces, shapes, composes, and finishes a page while the person watches", async ({ launch, sandbox }) => {
  const { app, page, folders } = await launchCompiled(launch, sandbox);
  const folder = folders[0] ?? "";
  const agent = await AgentClient.connect(AGENT_ADDRESS);
  const working = boardView(page).locator(".board-topbar .board-working");
  const entranceCard = card(page, "Server · Place an order");
  const createOrder = card(page, "createOrder");

  let entrance = "";
  let createOrderKey = "";
  await test.step("the agent reads the guide, the entrances, and createOrder's compiled tree", async () => {
    expect(await agent.ok("get_guide", { topic: "start" })).toMatch(/^Unfold · as Claude\n\S/);

    const entrances = await agent.ok("list_entrances", { project: "ts-orders", app: "Server" });
    entrance = found(entrances, /^- Place an order: key (\S+); on POST \/orders; code /m);

    const start = await agent.ok("read_code", { project: "ts-orders", key: entrance });
    expect(start).toContain("Entrance of Server Place an order");
    createOrderKey = found(start, /^3\. createOrder \(key (\S+)\): found at src\/routes\/orders\.ts:25$/m);

    const tree = await agent.ok("read_code", { project: "ts-orders", key: createOrderKey });
    expect(tree).toContain("function createOrder at src/routes/orders.ts:25");
    // Each block on the line it starts at, by short label, with how it is drawn.
    expect(tree).toMatch(/^L26 {2}⋯ const draft: OrderDraft = req\.body;\s+code$/m);
    expect(tree).toMatch(/^L27 {2}if\[0\]\/then\s+test req\.header\("prefer"\) === "respond-async"\s+key=\S+$/m);
    expect(tree).toMatch(/^L32 {2}call:orderService\.place\s+▸ src\/services\/order-service\.ts#OrderService\.place · binds order\s+key=\S+$/m);
    expect(tree).toMatch(/^L33 {2}call:slack\.post\s+▸ src\/lib\/slack\.ts#slack\.post\s+key=\S+$/m);
    expect(tree).toMatch(/^L34 {2}call:res\.status\(\)\.json\s+\? unknown_receiver\s+key=\S+$/m);
  });

  let boardId = "";
  let pageId = "";
  let boardFile = "";
  let revision = "";
  let createOrderId = "";
  let entranceId = "";
  let traced: string[] = [];
  await test.step("create_board traces the entrance onto a new board; the person's view doesn't move until they open it", async () => {
    const created = await agent.ok("create_board", { project: "ts-orders", title: BOARD, trace: entrance });
    expect(created).toContain("the person's view didn't move");
    boardId = found(created, /\(board id (\S+)\)/);
    pageId = found(created, /\(page id (\S+)\)/);
    entranceId = found(created, /^- Place an order \(card id (\S+), created\)$/m);
    createOrderId = found(created, /^- createOrder \(card id (\S+), created\)/m);
    traced = [...created.matchAll(/^- \S.* \(card id (\S+), created\)/gm)].map(([, id = ""]) => id);
    expect(traced).toHaveLength(4);
    revision = revisionOf(created);
    boardFile = join(folder, boardId);
    expect(savedPage(boardFile).cards.map((item) => item.id).toSorted()).toEqual(traced.toSorted());

    await expect(sidebarBoard(page, BOARD)).toBeVisible();
    await expect(homeMain(page)).toBeVisible();
    await showBoard(page, BOARD);
    await expect(cards(page)).toHaveCount(4);
    await expectCardsLoaded(page);
    await expect(working).toHaveText("Claude is working…");
    for (const name of ["Server · Place an order", "requireUser", "validateBody", "createOrder"]) {
      await expectWorkingOutline(card(page, name));
    }
  });

  await test.step("shape_card names a branch, combines two steps, and opens a call in place, as one undo step", async () => {
    const expanded = await agent.ok("set_view", { board: boardId, page: pageId, items: [{ id: createOrderId, view: "expanded" }], expectRevision: revision });
    const shaped = await agent.ok("shape_card", {
      board: boardId,
      page: pageId,
      card: createOrderId,
      description: DESCRIPTION,
      names: { "if[0]/then": BRANCH },
      combine: [{ steps: ["call:slack.post", "call:res.status().json"], title: COMBINED }],
      open: [["call:orderService.place"]],
      expectRevision: revisionOf(expanded),
    });
    revision = revisionOf(shaped);
    expect(shaped).toContain("One undo step in Unfold, saved.");
    expect(shapedLines(shaped)).toEqual([
      "( createOrder )",
      "  ⋯ 1 line",
      `  IF ${BRANCH} · req.header("prefer") === "respond-async"`,
      "      · ordersQueue.add → job, res.status().json ?   2 calls, 1 unknown",
      "      RETURN → End",
      "  ▸ orderService.place → order",
      "      ( place )",
      "        ▸ validateOrder → draft",
      "        · this.payments.charge ?, randomUUID   2 calls, 1 unknown",
      "        ▸ this.repo.insert",
      "        ▸ events.emit",
      "        RETURN order → returns",
      "      ( returns )",
      `  ⊞ ${COMBINED}   slack.post · 1 library · 1 unknown ?`,
      "( End )",
    ]);

    // The window draws the same shaping: the name, the combined step, and the call opened in place.
    await expect(createOrder.locator(".flow-head .flow-name")).toHaveText(BRANCH);
    const combined = createOrder.locator(".flow-combined", { hasText: COMBINED });
    await expect(combined).toHaveClass(/\bcollapsed\b/);
    await expect(createOrder.locator(".flow-call-name", { hasText: /^slack\.post$/ })).toHaveCount(0);
    await expect(createOrder.getByRole("button", { name: "Close orderService.place", exact: true })).toHaveAttribute("aria-expanded", "true");
    await expect(createOrder.locator(".flow-inset-name")).toHaveText("place");
    await expect(createOrder.locator(".flow-call-name", { hasText: /^this\.repo\.insert$/ })).toBeVisible();
    await expect(createOrder.locator(".card-description")).toContainText(DESCRIPTION);
    await expectWorkingOutline(createOrder);

    const saved = savedPage(boardFile);
    expect(saved.names?.map(({ text, author }) => ({ text, author }))).toEqual([{ text: BRANCH, author: "agent:claude" }]);
    expect(saved.combined?.map(({ title, collapsed }) => ({ title, collapsed }))).toEqual([{ title: COMBINED, collapsed: true }]);
    expect(saved.cards.find((item) => item.id === createOrderId)?.description).toMatchObject({ text: DESCRIPTION, author: "agent:claude" });
  });

  await test.step("Edit › Undo takes the whole shaping off in one step, and Redo puts it back", async () => {
    /** The shaping as the board file holds it. */
    const savedShaping = () => {
      const saved = savedPage(boardFile);
      return {
        names: saved.names?.map(({ text }) => text) ?? [],
        combined: saved.combined?.map(({ title }) => title) ?? [],
        description: saved.cards.find((item) => item.id === createOrderId)?.description?.text ?? null,
      };
    };
    const inset = createOrder.locator(".flow-inset-name");
    const combined = createOrder.locator(".flow-combined", { hasText: COMBINED });

    await app.clickMenu(page, ["Edit", "Undo"]);
    await expect(createOrder.getByText(BRANCH, { exact: true })).toHaveCount(0);
    await expect(combined).toHaveCount(0);
    await expect(createOrder.locator(".flow-call-name", { hasText: /^slack\.post$/ })).toBeVisible();
    await expect(inset).toHaveCount(0);
    await expect(createOrder.getByText(DESCRIPTION)).toHaveCount(0);
    // set_view, the step before, stays: the card is still expanded.
    await expect(createOrder.locator(".flow-call-name", { hasText: /^orderService\.place$/ })).toBeVisible();
    await expect.poll(savedShaping).toEqual({ names: [], combined: [], description: null });

    await app.clickMenu(page, ["Edit", "Redo"]);
    await expect(createOrder.locator(".flow-head .flow-name")).toHaveText(BRANCH);
    await expect(combined).toHaveClass(/\bcollapsed\b/);
    await expect(inset).toHaveText("place");
    await expect(createOrder.locator(".card-description")).toContainText(DESCRIPTION);
    await expect.poll(savedShaping).toEqual({ names: [BRANCH], combined: [COMBINED], description: DESCRIPTION });
    revision = revisionOf(await agent.ok("read_page", { board: boardId, page: pageId }));
  });

  const customer = boardView(page).getByRole("article", { name: "actor Customer", exact: true });
  const frame = boardView(page).getByRole("region", { name: `Frame ${FRAME}`, exact: true });
  const note = boardView(page).getByRole("complementary", { name: `Note: ${NOTE}`, exact: true });
  const http = boardView(page).locator(".board-connection.kind-http");
  await test.step("the agent frames the service, adds the customer, connects it over HTTP citing the entrance, and adds a note", async () => {
    const framed = await agent.ok("frame_cards", {
      board: boardId,
      page: pageId,
      frames: [{ title: FRAME, description: FRAME_ABOUT, members: traced, icon: "api" }],
      expectRevision: revision,
    });
    expect(framed).toMatch(/^- Orders API \(frame id \S+; box, api; created\) holds /m);

    const added = await agent.ok("add_resources", {
      board: boardId,
      page: pageId,
      resources: [{ kind: "actor", name: "Customer", description: "Whoever places the order through the API." }],
      expectRevision: revisionOf(framed),
    });
    const customerId = found(added, /^- actor “Customer” \(resource id (\S+); created\)$/m);

    // An agent's HTTP line cites its receiving side; the channel fills in from the entrance.
    const uncited = await agent.refused("connect", {
      board: boardId,
      page: pageId,
      connections: [{ kind: "http", from: customerId, to: entranceId, description: "The customer posts the order to the server." }],
    });
    expect(uncited).toContain("it has no anchors");
    const connected = await agent.ok("connect", {
      board: boardId,
      page: pageId,
      connections: [{ kind: "http", from: customerId, to: entranceId, description: "The customer posts the order to the server.", anchors: [{ key: entrance }] }],
      expectRevision: revisionOf(added),
    });
    expect(connected).toMatch(new RegExp(`^- http connection ${customerId} → ${entranceId} \\(link id \\S+; created\\): channel POST /orders$`, "m"));

    const noted = await agent.ok("add_notes", { board: boardId, page: pageId, notes: [{ text: NOTE, attach: createOrderId }], expectRevision: revisionOf(connected) });
    revision = revisionOf(noted);

    await expect(frame).toBeVisible();
    await expect(frame.locator(".frame-description")).toHaveText(FRAME_ABOUT);
    await expect(customer).toBeVisible();
    await expect(note).toBeVisible();
    await expect(boardView(page).getByRole("button", { name: "Connection HTTP · POST /orders", exact: true })).toBeVisible();
    for (const item of [customer, frame, note, entranceCard]) {
      await expectWorkingOutline(item);
    }
    await expect(http).toHaveClass(/\bworking\b/);
    await expect(working).toHaveText("Claude is working…");

    const saved = savedPage(boardFile);
    expect(saved.resources?.map(({ id, kind, name }) => ({ id, kind, name }))).toEqual([{ id: customerId, kind: "actor", name: "Customer" }]);
    expect(saved.links?.map(({ from, to, kind }) => ({ from, to, kind }))).toEqual([{ from: customerId, to: entranceId, kind: "http" }]);
  });

  await test.step("check finds the page healthy; finish_work clears the marks and shows the summary", async () => {
    const checked = await agent.ok("check", { board: boardId, page: pageId });
    expect(checked).toContain("Health: no problems.");
    expect(revisionOf(checked)).toBe(revision);

    const finished = await agent.ok("finish_work", { summary: SUMMARY });
    expect(finished).toContain(`Finished on “${BOARD}” (board id ${boardId})`);
    const finishedToast = toast(page, "Claude finished");
    await expect(finishedToast.locator(".toast-title")).toHaveText("Claude finished");
    await expect(finishedToast.locator(".toast-body")).toHaveText(SUMMARY);
    await expect(working).toHaveCount(0);
    await expect(boardView(page).locator(".react-flow__node.agent-working")).toHaveCount(0);
    await expect(http).not.toHaveClass(/\bworking\b/);
    await expectWorkingOutline(createOrder, false);
  });

  await test.step("asked to, the agent declares an entrance, and unfold.toml gains exactly that line", async () => {
    const config = join(folder, "unfold.toml");
    const before = readFile(config).split("\n");
    const main = found(await agent.ok("search", { projects: ["ts-orders"], query: "main" }), /^- main \(function\) at src\/cli\.ts:6: key (\S+)$/m);
    const declared = await agent.ok("set_entrance", { project: "ts-orders", app: "Orders CLI", name: "Show help", on: "orders --help", code: main });
    expect(declared).toContain("Added the entrance Show help to Orders CLI. Changed unfold.toml of ts-orders:");

    const after = readFile(config).split("\n");
    expect(after).toHaveLength(before.length + 1);
    const at = after.findIndex((line, index) => line !== before[index]);
    const line = after[at] ?? "";
    expect(line).toBe(`  { name = "Show help", on = "orders --help", code = "${main}" },`);
    expect(after.toSpliced(at, 1)).toEqual(before);
    expect(declared).toContain(`\n+${line}\n`);

    await showHome(page);
    await expect(startRow(page, "Show help")).toBeVisible();
  });

  await agent.close();
  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});
