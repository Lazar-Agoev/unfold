/**
 * Explaining a card by hand (docs/designs/mcp-capabilities.md, "What people do by hand"), end to end on
 * a copy of `py-orders`, whose `OrderService.place` charges, saves, announces, and returns: from the
 * block menu a step is named, the announcement and the return are combined, the card is described,
 * and a step is proposed; the combined step collapses and keeps its way out to End and its arrow; a
 * table added from the canvas's menu takes a drawn link that becomes Writes; two tables relate with
 * columns and a cardinality; Edit › Undo and Redo step through it; an edit to the code under the
 * description sends the card to review until Accept; and a board file changed on disk reloads.
 */

import { join } from "node:path";
import { boardView, card, expectCardsLoaded, selected, showBoard } from "../src/board";
import { addTable, chooseOnBlock, connectionChip, finishLink, savedPage, setLinkKind } from "../src/explain";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { sidebarBoard } from "../src/home";
import { chooseOnCard, clickOnCanvas } from "../src/reader";
import { editFile, readFile, writeFile } from "../src/sandbox";
import { enterText } from "../src/window";

const SERVICE = "src/orders_playground/services/order_service.py";
const PLACE = `${SERVICE}#OrderService.place`;
const VALIDATE = `${PLACE}/call:validate_order`;
const INSERT = `${PLACE}/call:self.repo.insert`;
const EMIT = `${PLACE}/call:events.emit`;
const RETURN = `${PLACE}/return[0]`;
const EVENTS_EMIT = "src/orders_playground/services/events.py#Events.emit";

const NAME = "Save the order";
const COMBINED = "Announce it and hand it back";
const DESCRIPTION = "Charges the draft, saves it as an order, and announces it.";
const PROPOSED = "Reserve the stock";

test("a person names, combines, describes, proposes, and composes a page that keeps up with the code", async ({ launch, sandbox }) => {
  const { app, page, folders } = await launchCompiled(launch, sandbox, ["py-orders"]);
  const folder = folders[0] ?? "";
  const boardFile = join(folder, ".unfold", "boards", "explain.json");
  writeFile(
    boardFile,
    JSON.stringify({
      format: "unfold-board",
      version: 1,
      title: "Explain",
      pages: [
        {
          id: "page-1",
          title: "Placing an order",
          defaultView: "compact",
          cards: [
            { id: "place", ref: { block: PLACE }, at: [0, 0], view: "expanded" },
            { id: "emit", ref: { block: EVENTS_EMIT }, at: [760, 0] },
          ],
        },
      ],
    }),
  );
  await expect(sidebarBoard(page, "Explain")).toBeVisible();
  await showBoard(page, "Explain");
  const place = card(page, "place");
  const emit = card(page, "emit");
  await expect(place).toBeVisible();
  await expect(emit).toBeVisible();
  await expectCardsLoaded(page);

  const block = (key: string) => place.locator(`.flow-call[data-key='${key}']`);
  const combined = place.locator(".flow-combined", { hasText: COMBINED });
  const emitPort = place.locator(`.card-port[data-handleid='call:${EMIT}']`);
  const emitArrow = boardView(page).locator(`.board-arrow-hit[data-arrow='place|call:${EMIT}|emit']`);
  const sidebarField = (name: string) => selected(page).getByRole("textbox", { name, exact: true });

  await test.step("the block menu names a block, combines consecutive blocks, describes the card, and proposes a step", async () => {
    await expect(emitArrow).toHaveCount(1);

    await chooseOnBlock(page, block(INSERT), "Name…");
    await expect(sidebarField("Name")).toBeFocused();
    await enterText(sidebarField("Name"), NAME);
    await expect(block(INSERT).locator(".flow-call-name.named")).toHaveText(NAME);
    await expect(block(INSERT).locator(".flow-code-name")).toHaveText("self.repo.insert");
    await expect.poll(() => savedPage(boardFile).names?.map((entry) => [entry.key, entry.text])).toEqual([[INSERT, NAME]]);

    await chooseOnBlock(page, block(EMIT), "Combine…");
    const chooser = page.getByRole("dialog", { name: "Combine steps" });
    const steps = chooser.getByRole("listbox", { name: "Steps" });
    await expect(steps.getByRole("option", { name: "events.emit" })).toHaveAttribute("aria-selected", "true");
    await expect(steps.getByRole("option", { name: NAME })).toHaveAttribute("aria-selected", "false");
    await steps.getByRole("option", { name: "return order" }).click();
    await expect(chooser.getByText("Combine 2 steps")).toBeVisible();
    await enterText(chooser.getByRole("textbox", { name: "Title" }), COMBINED);
    await chooser.getByRole("button", { name: "Combine", exact: true }).click();
    await expect(chooser).toBeHidden();
    await expect(combined).toHaveClass(/\bcollapsed\b/);
    await expect(block(EMIT)).toHaveCount(0);
    await expect.poll(() => savedPage(boardFile).combined?.map(({ title, members, collapsed }) => ({ title, members, collapsed }))).toEqual([
      { title: COMBINED, members: [EMIT, RETURN], collapsed: true },
    ]);

    await chooseOnBlock(page, block(VALIDATE), "Describe…");
    await expect(sidebarField("Description")).toBeFocused();
    await enterText(sidebarField("Description"), DESCRIPTION);
    await expect(place.locator(".card-description")).toContainText(DESCRIPTION);
    await expect.poll(() => savedPage(boardFile).cards.find((item) => item.id === "place")?.description?.text).toBe(DESCRIPTION);

    await chooseOnBlock(page, block(VALIDATE), "Add proposed step after…");
    const proposing = page.getByRole("dialog", { name: "Add proposed step" });
    await expect(proposing.locator(".explain-where")).toHaveText("validate_order");
    await enterText(proposing.getByRole("textbox", { name: "Title" }), PROPOSED);
    await enterText(proposing.getByRole("textbox", { name: "Why" }), "Two buyers can take the last one.");
    await proposing.getByRole("button", { name: "Propose", exact: true }).click();
    await expect(proposing).toBeHidden();
    await expect(place.locator(".flow-proposed", { hasText: PROPOSED })).toBeVisible();
    await expect.poll(() => savedPage(boardFile).proposals?.map(({ title, reason, after }) => ({ title, reason, after }))).toEqual([
      { title: PROPOSED, reason: "Two buyers can take the last one.", after: VALIDATE },
    ]);
    const saved = savedPage(boardFile);
    for (const author of [...(saved.names ?? []), ...(saved.proposals ?? [])].map((item) => item.author)) {
      expect(author).toMatch(/^person:/);
    }
  });

  await test.step("the combined step opens and collapses, and keeps its way out to End and its arrow", async () => {
    await chooseOnBlock(page, combined.locator(".flow-combined-title"), "Open");
    await expect(combined).toHaveClass(/\bopen\b/);
    await expect(block(EMIT)).toBeVisible();
    await expect(place.locator(`.flow-exit[data-key='${RETURN}']`)).toBeVisible();

    await chooseOnBlock(page, combined.locator(".flow-combined-title"), "Collapse");
    await expect(combined).toHaveClass(/\bcollapsed\b/);
    await expect(place.locator(`.flow-exit[data-key='${RETURN}']`)).toHaveCount(0);
    await expect(combined.locator(".flow-combined-summary")).toContainText("events.emit");
    // The return inside still routes to End, and the call inside still points at its card, from the step.
    await expect(combined.locator(".flow-combined-exit.r-return")).toHaveText(/returns\s*→ End/);
    await expect(emitArrow).toHaveCount(1);
    const box = await combined.boundingBox();
    const port = await emitPort.boundingBox();
    if (!box || !port) {
      throw new Error("the combined step or its port isn't drawn");
    }
    expect(port.y + port.height / 2).toBeGreaterThan(box.y);
    expect(port.y + port.height / 2).toBeLessThan(box.y + box.height);
    await expect.poll(() => savedPage(boardFile).combined?.[0]?.collapsed).toBe(true);
  });

  let ordersId = "";
  await test.step("a table added from the canvas's menu takes a drawn link, and the link's kind becomes Writes", async () => {
    const orders = await addTable(page, "orders", [
      { name: "id", type: "uuid", key: "primary" },
      { name: "customer_id", type: "uuid", key: "foreign" },
      { name: "total", type: "numeric" },
    ]);
    await expect(orders).toContainText("customer_id");
    await expect
      .poll(() => savedPage(boardFile).resources?.map(({ kind, name, columns }) => ({ kind, name, columns })))
      .toEqual([
        {
          kind: "table",
          name: "orders",
          columns: [
            { name: "id", type: "uuid", key: "primary" },
            { name: "customer_id", type: "uuid", key: "foreign" },
            { name: "total", type: "numeric" },
          ],
        },
      ]);
    ordersId = savedPage(boardFile).resources?.[0]?.id ?? "";

    await chooseOnCard(page, place, "Draw link…");
    await finishLink(page, orders, "keeps the order");
    await expect(connectionChip(page, "keeps the order")).toBeVisible();
    await expect.poll(() => savedPage(boardFile).links?.map(({ from, to, kind, label }) => ({ from, to, kind, label: label?.text }))).toEqual([
      { from: "place", to: ordersId, kind: "other", label: "keeps the order" },
    ]);

    await setLinkKind(page, connectionChip(page, "keeps the order"), "Writes");
    await expect(connectionChip(page, "writes · orders · keeps the order")).toBeVisible();
    await expect(boardView(page).locator(".board-connection.kind-writes")).toHaveCount(1);
    await expect.poll(() => savedPage(boardFile).links?.map(({ kind, label }) => ({ kind, label: label?.text }))).toEqual([{ kind: "writes", label: "keeps the order" }]);
  });

  const relationship = () => savedPage(boardFile).links?.find((link) => link.kind === "relates");
  await test.step("two tables relate, with their columns and a cardinality set in Selected", async () => {
    const orders = boardView(page).getByRole("article", { name: "table orders", exact: true });
    const customers = await addTable(page, "customers", [
      { name: "id", type: "uuid", key: "primary" },
      { name: "email", type: "text", key: "unique" },
    ]);
    await clickOnCanvas(page, orders.locator(".card-header"), { button: "right" });
    const menu = page.getByRole("menu", { name: "Resource card actions" });
    await menu.getByRole("menuitem", { name: "Draw link…", exact: true }).click();
    await finishLink(page, customers, "belongs to");
    await setLinkKind(page, connectionChip(page, "belongs to"), "Relates");
    await expect(connectionChip(page, "relates · belongs to")).toBeVisible();

    const columns = selected(page);
    await columns.getByRole("button", { name: "Column", exact: true }).first().click();
    await page.getByRole("menu", { name: "Column of orders" }).getByRole("menuitemcheckbox", { name: "customer_id", exact: true }).click();
    await expect(columns.getByRole("button", { name: "customer_id", exact: true })).toBeVisible();
    await columns.getByRole("button", { name: "Column", exact: true }).click();
    const theirs = page.getByRole("menu", { name: "Column of customers" });
    await expect(theirs.getByRole("menuitemcheckbox")).toHaveText(["id", "email"]);
    await theirs.getByRole("menuitemcheckbox", { name: "id", exact: true }).click();
    await columns.getByRole("button", { name: "Choose", exact: true }).click();
    await page.getByRole("menu", { name: "Cardinality" }).getByRole("menuitemcheckbox", { name: "Many to one", exact: true }).click();
    await expect(columns.getByRole("button", { name: "Many to one", exact: true })).toBeVisible();
    await expect(connectionChip(page, "N : 1 · belongs to")).toBeVisible();
    await expect.poll(() => relationship()).toMatchObject({ from: ordersId, fromColumn: "customer_id", toColumn: "id", cardinality: "many-to-one" });
  });

  await test.step("Edit › Undo and Redo step back and forth through the edits", async () => {
    await app.clickMenu(page, ["Edit", "Undo"]);
    await expect(connectionChip(page, "relates · belongs to")).toBeVisible();
    await expect.poll(() => relationship()?.cardinality ?? null).toBeNull();
    await app.clickMenu(page, ["Edit", "Undo"]);
    await expect.poll(() => relationship()?.toColumn ?? null).toBeNull();
    expect(relationship()?.fromColumn).toBe("customer_id");

    await app.clickMenu(page, ["Edit", "Redo"]);
    await app.clickMenu(page, ["Edit", "Redo"]);
    await expect(connectionChip(page, "N : 1 · belongs to")).toBeVisible();
    await expect.poll(() => relationship()).toMatchObject({ fromColumn: "customer_id", toColumn: "id", cardinality: "many-to-one" });
  });

  await test.step("an edit to the code under the description sends the card to review, and Accept clears it", async () => {
    const banner = place.locator(".card-review");
    const signature = savedPage(boardFile).cards.find((item) => item.id === "place")?.description?.signature;
    expect(signature).toBeTruthy();
    await expect(banner).toHaveCount(0);
    editFile(join(folder, SERVICE), `"status": ORDER_STATUSES[0]`, `"status": ORDER_STATUSES[1]`);
    await expect(banner).toContainText("Changed since it was described");
    // Only the description is under review: the name, the combined step, and the proposal keep their code.
    await expect(boardView(page).locator(".card-review")).toHaveCount(1);
    await clickOnCanvas(page, banner.getByRole("button", { name: "Review" }));
    const review = page.getByRole("menu", { name: "Review" });
    await review.getByRole("menuitem", { name: "Accept", exact: true }).click();
    await expect(banner).toHaveCount(0);
    await expect(place.locator(".card-description")).toContainText(DESCRIPTION);
    await expect.poll(() => savedPage(boardFile).cards.find((item) => item.id === "place")?.description?.signature).not.toBe(signature);
  });

  await test.step("a board file changed on disk reloads the board", async () => {
    const board = JSON.parse(readFile(boardFile)) as { pages: { resources?: { name: string }[] }[] };
    const customers = board.pages[0]?.resources?.find((resource) => resource.name === "customers");
    if (!customers) {
      throw new Error("the board file has no customers table");
    }
    customers.name = "accounts";
    writeFile(boardFile, JSON.stringify(board, null, 2));
    await expect(boardView(page).getByRole("article", { name: "table accounts", exact: true })).toBeVisible();
    await expect(boardView(page).getByRole("article", { name: "table customers", exact: true })).toHaveCount(0);
    await expect(block(INSERT).locator(".flow-call-name.named")).toHaveText(NAME);
    await expect(combined).toBeVisible();
    await expect(connectionChip(page, "N : 1 · belongs to")).toBeVisible();
  });

  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});
