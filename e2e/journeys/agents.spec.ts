/**
 * Agents (docs/ipc.md) on a compiled `ts-orders`: Preferences ›
 * Agents turns the local MCP server on; the server refuses web pages and other hosts; a real MCP
 * client, connected as Claude Code, lists the tools and prompts and builds the page of design §1.3
 * the way an agent would, while the window shows each piece as it lands: cards, frames, step groups,
 * a table, connections with their chips, a note, and the agent marks. A table the code doesn't back
 * is refused and leaves no undo step, so one ⌘Z undoes the agent's last edit; a write with a stale
 * revision is refused; and finish_work ends the work with a toast.
 */

import type { Locator, Page } from "@playwright/test";
import { AgentClient, freePort, openPreferences, rawStatus, revisionOf } from "../src/agents";
import { boardView, card, clickCanvas, expectCardsLoaded, readBoard, type BoardFile } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { boardTab, homeTab, toast } from "../src/home";
import { enterText } from "../src/window";

const POST_ORDERS = 'entrance:["express.route","http","POST","/orders","requireUser"]';
const ORDERS_WORKER = 'entrance:["bullmq.worker","worker","","orders","processOrder"]';
const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const CHARGE = "src/services/payment.ts#class:StripeProvider/method:charge";
const PROCESS_ORDER = "src/jobs/process-order.ts#function:processOrder";
const REQUIRE_USER = "src/routes/middleware.ts#function:requireUser";
const VALIDATE_BODY = "src/routes/middleware.ts#function:validateBody";
const REPOSITORY = "src/db/repository.ts#class:OrderRepository";

/** The first page of a board, as its file holds it. */
function firstPage(folder: string, board: string): BoardFile["pages"][number] {
  const [first] = readBoard(folder, board).pages;
  if (!first) {
    throw new Error(`${board} has no page`);
  }
  return first;
}

/** The id of the card that shows `key` on a board's first page. */
function cardId(folder: string, board: string, key: string): string {
  const found = firstPage(folder, board).cards.find((item) => (item.ref.block ?? item.ref.entrance) === key);
  if (!found) {
    throw new Error(`no card shows ${key}`);
  }
  return found.id;
}

/** An expanded frame on the canvas, by its title. */
function frame(page: Page, title: string): Locator {
  return boardView(page).getByRole("region", { name: `Frame ${title}`, exact: true });
}

/** A connection's line on the canvas, by its id. */
function connectionLine(page: Page, id: string): Locator {
  return boardView(page).locator(`.board-connection[data-link='${id}'] .connection-line`);
}

/** A connection's chip on the canvas, by the start of its words. */
function chip(page: Page, words: string): Locator {
  return boardView(page).getByRole("button", { name: new RegExp(`^Connection ${words}`) });
}

/** The top bar's "Claude is working…". */
function working(page: Page): Locator {
  return boardView(page).locator(".board-topbar").getByRole("status");
}

/** A PNG's width and height, from its header. */
function pngSize(base64: string): { width: number; height: number } {
  const bytes = Buffer.from(base64, "base64");
  expect(bytes.subarray(1, 4).toString("ascii")).toBe("PNG");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

test.describe("agents", () => {
  test("an agent connects over MCP and builds a page the person watches, undoes, and reviews", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const port = await freePort();
    const address = `http://127.0.0.1:${port}/mcp`;
    const clients: AgentClient[] = [];
    const ids: Record<string, string> = {};
    let board = "";
    let pageId = "";
    /** The page's revision before the connections were drawn: stale once they are. */
    let before = "";

    try {
      await test.step("Preferences › Agents: Let agents connect and a port, saved, start the server at the address shown", async () => {
        const preferences = await openPreferences(page);
        await preferences.getByRole("switch", { name: "Let agents connect" }).click();
        await enterText(preferences.getByRole("textbox", { name: "Port" }), String(port));
        await expect(preferences.getByText("Agents can connect once you save.")).toBeVisible();
        await preferences.getByRole("button", { name: "Save", exact: true }).click();
        await expect(preferences).toBeHidden();
        const again = await openPreferences(page);
        await expect(again.getByText(/^Listening\./)).toBeVisible();
        await expect(again.getByRole("group", { name: "Connect an agent" }).getByText(address, { exact: true })).toBeVisible();
        await again.getByRole("button", { name: "Cancel", exact: true }).click();
        await expect(again).toBeHidden();
      });

      await test.step("the server refuses a request from a web page and one for another host", async () => {
        expect(await rawStatus(address, { origin: "https://example.com" })).toBe(403);
        expect(await rawStatus(address, { host: `example.com:${port}` })).toBe(403);
      });

      const claude = await AgentClient.connect(address);
      clients.push(claude);

      await test.step("an agent connects as Claude: the 34 tools, the six prompts, the start guide, and Preferences lists it", async () => {
        const tools = await claude.tools();
        expect(tools).toHaveLength(34);
        expect(tools.slice(0, 2)).toEqual(["get_guide", "get_current_view"]);
        expect(tools.at(-1)).toBe("finish_work");
        expect(await claude.prompts()).toEqual(["explain-entrance", "simplify-function", "service-map", "data-model", "tour", "fix-boards"]);
        expect(await claude.ok("get_guide", { topic: "start" })).toContain("# Working in Unfold");
        const preferences = await openPreferences(page);
        await expect(preferences.getByRole("group", { name: "Connected agents" }).getByText(/^Claude · /)).toBeVisible();
        await preferences.getByRole("button", { name: "Cancel", exact: true }).click();
        await expect(preferences).toBeHidden();
      });

      await test.step("create_board traces POST /orders onto a draft in a background tab; the person stays on Home", async () => {
        const created = await claude.ok("create_board", { project: "ts-orders", title: "Order placement", trace: POST_ORDERS });
        expect(created).toContain("Traced POST /orders on the first page");
        board = /\(board id ([^)]+)\)/.exec(created)?.[1] ?? "";
        pageId = /\(page id ([^)]+)\)/.exec(created)?.[1] ?? "";
        expect(board).toMatch(/^\.unfold\/local\/drafts\//);
        await expect(boardTab(page, "Order placement")).toBeVisible();
        await expect(homeTab(page)).toHaveAttribute("aria-selected", "true");
        for (const [name, key] of Object.entries({ post: POST_ORDERS, requireUser: REQUIRE_USER, validateBody: VALIDATE_BODY, createOrder: CREATE_ORDER })) {
          ids[name] = cardId(folder, board, key);
        }
      });

      await test.step("get_current_view reports the board and the card the person selects", async () => {
        await boardTab(page, "Order placement").click();
        await expectCardsLoaded(page);
        await card(page, "#function:createOrder").locator(".card-header").click();
        await expect.poll(() => claude.ok("get_current_view")).toContain(`Selected: card ${ids["createOrder"]}: ${CREATE_ORDER}.`);
        const view = await claude.ok("get_current_view");
        expect(view).toContain("Unfold · ts-orders › Order placement (draft) › Page 1");
        expect(view).toContain(`board ${board}, page ${pageId}`);
      });

      await test.step("add_cards: OrderService.place in Flow, the orders worker with its handler, StripeProvider.charge in Diagram", async () => {
        const added = await claude.ok("add_cards", {
          board,
          page: pageId,
          cards: [
            { key: PLACE, view: "flow" },
            { key: ORDERS_WORKER, follow: { callees: 1 } },
            { key: CHARGE, view: "diagram" },
          ],
        });
        expect(added).toContain("Cards (4 created):");
        for (const [name, key] of Object.entries({ place: PLACE, worker: ORDERS_WORKER, processOrder: PROCESS_ORDER, charge: CHARGE })) {
          ids[name] = cardId(folder, board, key);
        }
        await expect(card(page, "#class:OrderService/method:place").locator(".flow-node").first()).toBeVisible();
        await expect(card(page, "#function:processOrder")).toBeVisible();
        await expect(card(page, "#class:StripeProvider/method:charge")).toHaveClass(/diagram-card/);
        await expect(working(page)).toHaveText("Claude is working…");
        await expect(boardView(page).locator(".react-flow__node.agent-working").first()).toBeVisible();
      });

      await test.step("frame_cards: server, worker, and shared boxes, a collapsed Request checks, and the ts-orders zone", async () => {
        const framed = await claude.ok("frame_cards", {
          board,
          page: pageId,
          frames: [
            {
              id: "request-checks",
              title: "Request checks",
              description: "Turns the request away unless a user is signed in and the body is valid.",
              members: [ids["requireUser"], ids["validateBody"]],
              collapsed: true,
            },
            { id: "server", title: "server", icon: "api", members: [ids["post"], "request-checks", ids["createOrder"]] },
            { id: "worker", title: "worker", icon: "worker", members: [ids["worker"], ids["processOrder"]] },
            { id: "shared", title: "shared", icon: "library", members: [ids["place"], ids["charge"]] },
            { id: "ts-orders", title: "ts-orders", look: "zone", icon: "system", members: ["server", "worker", "shared"] },
          ],
        });
        expect(framed).toContain("Frames (5 created):");
        for (const title of ["server", "worker", "shared", "ts-orders"]) {
          await expect(frame(page, title).locator(".agent-mark")).toBeVisible();
        }
        await expect(frame(page, "ts-orders")).toHaveClass(/look-zone/);
        await expect(boardView(page).getByRole("article", { name: "Frame Request checks, collapsed" })).toContainText("2 functions");
        await expect(card(page, "#function:requireUser")).toHaveCount(0);
      });

      await test.step("group_steps and label_steps give createOrder two collapsed step groups and two labels", async () => {
        const grouped = await claude.ok("group_steps", {
          board,
          page: pageId,
          card: ids["createOrder"],
          groups: [
            { title: "Queue it for a worker, reply 202", lines: [28, 30], collapsed: true },
            { title: "Place it now and reply 201", lines: [32, 34], collapsed: true },
          ],
        });
        expect(grouped).toContain("lines 28–30: variable (line 28), expression_statement (line 29), return (line 30)");
        const labeled = await claude.ok("label_steps", {
          board,
          page: pageId,
          card: ids["createOrder"],
          labels: [
            { step: `${CREATE_ORDER}/variable:draft`, text: "Read the order draft" },
            { step: `${CREATE_ORDER}/if[0]`, text: "Did the client ask for an async reply?" },
          ],
        });
        expect(labeled).toContain("Flow labels (2 created):");
        const createOrder = card(page, "#function:createOrder");
        await expect(createOrder.getByText("Queue it for a worker, reply 202", { exact: true })).toBeVisible();
        await expect(createOrder.getByText("Place it now and reply 201", { exact: true })).toBeVisible();
      });

      await test.step("add_resources: the orders table backed by interface Order, anchored to OrderRepository", async () => {
        const added = await claude.ok("add_resources", {
          board,
          page: pageId,
          resources: [
            {
              id: "orders-table",
              kind: "table",
              name: "orders",
              type: "src/types.ts#interface:Order",
              columns: [{ name: "id", key: "primary" }],
              anchors: [{ key: REPOSITORY }],
              description: "Orders by id, in memory for each process.",
              frame: "shared",
            },
          ],
        });
        // The columns are the type's fields, inherited ones included; Claude marked only the key.
        expect(added).toContain("columns id (primary), status, createdAt, customerId, email, items");
        before = revisionOf(added);
        const table = boardView(page).getByRole("article", { name: "table orders" });
        await expect(table.locator(".rcard-mark")).toContainText(["PK"]);
        await expect(table.locator(".rcard-name")).toContainText(["id", "status", "createdAt", "customerId", "email", "items"]);
        await expect(table.locator(".card-header .agent-mark")).toBeVisible();
      });

      await test.step("connect: the queue to the orders worker carrying OrderJob, the runtime call, and the write", async () => {
        const connected = await claude.ok("connect", {
          board,
          page: pageId,
          connections: [
            {
              id: "enqueue-order",
              kind: "queue",
              from: ids["createOrder"],
              step: `${CREATE_ORDER}/if[0]/variable:job/call[0]`,
              to: ids["worker"],
              carries: "src/types.ts#interface:OrderJob",
              label: "place-order job",
              description: "createOrder adds a place-order job to the orders queue, which the orders worker takes.",
              anchors: [{ key: "src/queues/queues.ts#variable:ordersQueue" }],
            },
            {
              id: "charge-stripe",
              kind: "calls",
              from: ids["place"],
              step: `${PLACE}/expression_statement[0]/call[0]`,
              to: ids["charge"],
              description: "createProvider returns a StripeProvider, so this.payments.charge runs StripeProvider.charge.",
              anchors: [{ key: "src/services/payment.ts#function:createProvider" }],
            },
            {
              id: "insert-order",
              kind: "writes",
              from: ids["place"],
              step: `${PLACE}/expression_statement[1]`,
              to: "orders-table",
              description: "place inserts the new order into the repository.",
            },
          ],
        });
        expect(connected).toContain("Connections (3 created):");
        // The channel comes from the worker entrance, not from Claude.
        expect(connected).toContain("channel orders, carries OrderJob");
        for (const id of ["enqueue-order", "charge-stripe", "insert-order"]) {
          await expect(connectionLine(page, id)).toBeVisible();
        }
        expect(firstPage(folder, board).links?.map((link) => [link.id, link.kind, link.author])).toEqual([
          ["charge-stripe", "calls", "agent:claude"],
          ["enqueue-order", "queue", "agent:claude"],
          ["insert-order", "writes", "agent:claude"],
        ]);
      });

      await test.step("a table with a column no anchor mentions is refused with the engine's words, and the page stays as it was", async () => {
        const refused = await claude.refused("add_resources", {
          board,
          page: pageId,
          resources: [{ id: "customers", kind: "table", name: "orders", columns: [{ name: "customer_id", type: "string" }], anchors: [{ key: REPOSITORY }] }],
        });
        expect(refused).toContain('Can\'t add the table "orders": the column "customer_id" doesn\'t appear in its anchors (OrderRepository in src/db/repository.ts).');
        await expect(boardView(page).locator(".resource-card")).toHaveCount(1);
        expect(firstPage(folder, board).resources?.map((resource) => resource.id)).toEqual(["orders-table"]);
      });

      await test.step("one ⌘Z undoes Claude's last edit, the three connections, as one step; ⇧⌘Z brings them back", async () => {
        await clickCanvas(page);
        await page.keyboard.press("Meta+Z");
        for (const id of ["enqueue-order", "charge-stripe", "insert-order"]) {
          await expect(connectionLine(page, id)).toHaveCount(0);
        }
        await expect(boardView(page).getByRole("article", { name: "table orders" })).toBeVisible();
        await page.keyboard.press("Meta+Shift+Z");
        for (const id of ["enqueue-order", "charge-stripe", "insert-order"]) {
          await expect(connectionLine(page, id)).toBeVisible();
        }
      });

      await test.step("add_notes and arrange_page; check finds no overlaps; get_screenshot draws the page", async () => {
        const note = "Queued orders are placed by the worker. The client gets a job id, not the order.";
        const noted = await claude.ok("add_notes", { board, page: pageId, notes: [{ id: "async-note", text: note, attach: ids["createOrder"] }] });
        expect(noted).toContain("attached to create-order");
        await expect(boardView(page).getByRole("complementary", { name: `Note: ${note}` })).toContainText("Claude · agent");
        expect(await claude.ok("arrange_page", { board, page: pageId })).toContain("Moved (");
        const checked = await claude.ok("check", { board, page: pageId });
        expect(checked).toContain("Health: no problems.");
        expect(checked).not.toContain("- overlap:");
        const shot = await claude.call("get_screenshot", { board, page: pageId });
        expect(shot.isError).toBe(false);
        expect(shot.images).toHaveLength(1);
        const [image] = shot.images;
        expect(image?.mimeType).toBe("image/png");
        const size = pngSize(image?.data ?? "");
        expect(shot.text).toContain(`The page (page id ${pageId}), ${size.width}×${size.height} px.`);
        expect(size.width).toBeGreaterThan(1000);
        expect(size.width).toBeLessThanOrEqual(4096);
        expect(size.height).toBeGreaterThan(400);
        // The chips keep clear of the cards once the page is arranged.
        await expect(chip(page, "queue · orders · OrderJob · place-order job")).toBeVisible();
        await expect(chip(page, "runtime call")).toBeVisible();
        await expect(chip(page, "writes · orders")).toBeVisible();
      });

      await test.step("read_page and read_code name what is on the page; a write with a stale revision is refused", async () => {
        const read = await claude.ok("read_page", { board, page: pageId });
        expect(read).toContain("Frames (5), nested as they are:");
        expect(read).toContain("table “orders” (resource id orders-table)");
        expect(read).toContain("queue connection createOrder (create-order) → worker orders (orders) (link id enqueue-order)");
        expect(read).toContain("calls connection OrderService.place (order-service-place) → StripeProvider.charge (stripe-provider-charge)");
        expect(read).toContain("note async-note, attached to createOrder (create-order)");
        const code = await claude.ok("read_code", { key: CREATE_ORDER });
        expect(code).toContain("28 |     const job = await ordersQueue.add(\"place-order\", { order: draft });");
        expect(code).toContain("line 28: calls ordersQueue.add → external, bullmq");
        const now = revisionOf(read);
        expect(now).not.toBe(before);
        const stale = await claude.refused("describe", { board, page: pageId, items: [{ id: "worker", text: "Takes queued orders and places them." }], expectRevision: before });
        expect(stale).toContain(`The page changed since you read it (rev ${before}, now rev ${now}).`);
        await expect(frame(page, "worker").locator(".frame-description")).toHaveCount(0);
        await claude.ok("describe", { board, page: pageId, items: [{ id: "worker", text: "Takes queued orders and places them." }], expectRevision: now });
        await expect(frame(page, "worker").locator(".frame-description")).toHaveText("Takes queued orders and places them.");
      });

      await test.step("finish_work shows Claude's summary as a toast and clears Claude is working…", async () => {
        await expect(working(page)).toBeVisible();
        const finished = await claude.ok("finish_work", { summary: "Order placement: frames, a table, and three connections" });
        expect(finished).toContain("its working marks are cleared");
        await expect(toast(page, "Claude · Order placement: frames, a table, and three connections")).toBeVisible();
        await expect(working(page)).toHaveCount(0);
        await expect(boardView(page).locator(".react-flow__node.agent-working")).toHaveCount(0);
      });
    } finally {
      await Promise.all(clients.map((client) => client.close()));
    }
  });
});
