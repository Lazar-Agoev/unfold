/**
 * The code panel (docs/designs/viewing-code.md), end to end on a copy of `ts-orders`: a board written as
 * a file (with a combined step, which people can't make yet) opens with two expanded cards; selecting a
 * call, a part, a gap tick, and the combined step each shows its lines with the right thing marked; the
 * path widens the view; lines hovered and clicked in the panel light and select their blocks; a file
 * opened from Add adds a card with `⊕`; an entrance opens its lines in `unfold.toml`; a file changed on
 * disk shows whole until the background compile catches up; and Open in editor runs the editor command.
 */

import { chmodSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { addInput, openAdd, result, selectResult } from "../src/add";
import { openSettings } from "../src/agents";
import { boardView, card, expectCardsLoaded, showBoard } from "../src/board";
import {
  codeBanner,
  codeLine,
  codePanel,
  expectOutlined,
  expectPath,
  expectShownLines,
  expectTintedLines,
  graphNode,
  graphNodes,
  lineOf,
  markedExpression,
  pathStep,
} from "../src/code";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { sidebarBoard } from "../src/home";
import { clickOnCanvas } from "../src/reader";
import { editFile, readFile, writeFile } from "../src/sandbox";
import { enterText, invoke } from "../src/window";

const ORDERS = "src/routes/orders.ts";
const CLI = "src/cli.ts";
const CREATE_ORDER = `${ORDERS}#createOrder`;
const MAIN = `${CLI}#main`;
const REPORT_CASE = `${MAIN}/switch[0]/case[1]`;
const COMBINED_TITLE = "Place it and announce it";

test("the code panel shows each block's lines, follows the canvas, and keeps up with the code", async ({ launch, sandbox }) => {
  const { app, page, folders } = await launchCompiled(launch, sandbox);
  const folder = folders[0] ?? "";
  const nodes = graphNodes("ts-orders");
  const ordersText = readFile(join(folder, ORDERS));

  // The editor command runs a stub that records its arguments.
  const opened = join(sandbox.root, "opened.txt");
  const stub = join(sandbox.root, "editor.sh");
  writeFile(stub, `#!/bin/sh\nprintf '%s\\n' "$@" > '${opened}'\n`);
  chmodSync(stub, 0o755);
  const editorCommand = `${stub} {path} {line}`;
  const settings = await openSettings(page);
  await enterText(settings.getByRole("textbox", { name: "Editor command" }), editorCommand);
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await invoke<{ readonly editorCommand?: string }>(page, "get_settings")).editorCommand).toBe(editorCommand);
  await page.keyboard.press("Escape");
  await expect(settings).toBeHidden();

  // A board file with createOrder and main expanded, and a combined step over two consecutive calls
  // of createOrder's body (read from the compiled graph).
  const body = nodes.filter((node) => node.parent === CREATE_ORDER).sort((a, b) => a.range[0] - b.range[0]);
  const placeAt = body.findIndex((node) => node.name === "orderService.place");
  const place = body[placeAt];
  const post = body[placeAt + 1];
  if (!place || post?.name !== "slack.post") {
    throw new Error("createOrder's body no longer calls orderService.place, then slack.post");
  }
  const placeExpression = Buffer.from(ordersText).subarray(place.range[0], place.range[1]).toString();
  writeFile(
    join(folder, ".unfold", "boards", "code-panel.json"),
    JSON.stringify({
      format: "unfold-board",
      version: 1,
      title: "Code panel",
      pages: [
        {
          id: "page-1",
          title: "Reading code",
          defaultView: "expanded",
          cards: [
            { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] },
            { id: "main", ref: { block: MAIN }, at: [760, 0] },
          ],
          combined: [{ id: "place-and-announce", title: COMBINED_TITLE, members: [place.key, post.key], collapsed: false, author: "person:e2e" }],
        },
      ],
    }),
  );
  await expect(sidebarBoard(page, "Code panel")).toBeVisible();
  await showBoard(page, "Code panel");
  const createOrder = card(page, "createOrder");
  const main = card(page, "main");
  await expect(createOrder).toBeVisible();
  await expect(main).toBeVisible();
  await expectCardsLoaded(page);

  const panel = codePanel(page);
  const placeBox = createOrder.locator(".flow-call", { has: page.locator(".flow-call-name", { hasText: /^orderService\.place$/ }) });
  const postBox = createOrder.locator(".flow-call", { has: page.locator(".flow-call-name", { hasText: /^slack\.post$/ }) });
  const reportCase = main.locator(".flow-head.case", { hasText: '"report"' });
  const gapTick = createOrder.getByRole("button", { name: "⋯ 1 line" });
  const combined = createOrder.locator(".flow-combined", { hasText: COMBINED_TITLE });

  await test.step("a call, a part, a gap tick, and a combined step each show their lines", async () => {
    // A call shows its statement, with its own expression marked; `</>` opens the panel.
    await clickOnCanvas(page, placeBox);
    await expect(panel).toBeHidden();
    await placeBox.getByRole("button", { name: "View code" }).click();
    await expect(panel).toBeVisible();
    await expectShownLines(page, place.lines[0], place.lines[1]);
    await expect.poll(() => markedExpression(page)).toBe(placeExpression);
    await expectTintedLines(page, null);
    await expectPath(page, [ORDERS, "createOrder", "orderService.place"], "orderService.place");
    await expectOutlined(placeBox);

    // A part shows its whole control block, the part tinted; the panel follows the selection.
    const part = graphNode(nodes, REPORT_CASE);
    const control = graphNode(nodes, part.parent ?? "");
    await clickOnCanvas(page, reportCase);
    await expectShownLines(page, control.lines[0], control.lines[1]);
    await expectTintedLines(page, part.lines);
    await expect.poll(() => markedExpression(page)).toBe("");
    await expectPath(page, [CLI, "main", "switch (command)", 'case "report"'], "switch (command)");
    await expectOutlined(reportCase);

    // A gap tick shows the lines it counts, and is outlined.
    const gapLine = lineOf(ordersText, "const draft: OrderDraft = req.body;");
    await clickOnCanvas(page, gapTick);
    await expectShownLines(page, gapLine, gapLine);
    await expectTintedLines(page, null);
    await expectPath(page, [ORDERS, "createOrder", `line ${gapLine}`], `line ${gapLine}`);
    await expectOutlined(gapTick);

    // A combined step shows from its first block's statement to its last block's end.
    await clickOnCanvas(page, combined.locator(".flow-combined-title"));
    await expectShownLines(page, place.lines[0], post.lines[1]);
    await expectTintedLines(page, null);
    await expect.poll(() => markedExpression(page)).toBe("");
    await expectPath(page, [ORDERS, "createOrder", `⊞ ${COMBINED_TITLE}`], `⊞ ${COMBINED_TITLE}`);
    await expectOutlined(combined);
  });

  const definition = graphNode(nodes, CREATE_ORDER);
  await test.step("the path widens the view, and the selection stays marked", async () => {
    await clickOnCanvas(page, placeBox);
    await expectShownLines(page, place.lines[0], place.lines[1]);
    await pathStep(page, "createOrder").click();
    await expectShownLines(page, definition.lines[0], definition.lines[1]);
    await expectTintedLines(page, place.lines);
    await expect.poll(() => markedExpression(page)).toBe(placeExpression);
    await expect(pathStep(page, "createOrder")).toHaveAttribute("aria-current", "true");
    await expect(placeBox).toHaveClass(/\bselected\b/);

    await pathStep(page, ORDERS).click();
    await expectShownLines(page, 1, ordersText.split("\n").length - 1);
    await expectTintedLines(page, place.lines);
    await expect(placeBox).toHaveClass(/\bselected\b/);
    await pathStep(page, "createOrder").click();
    await expectShownLines(page, definition.lines[0], definition.lines[1]);
  });

  await test.step("hovering a line lights its block; clicking it selects the block and the panel stays", async () => {
    const postLine = codeLine(page, post.lines[0]);
    await postLine.hover();
    await expectOutlined(postBox, true);
    await postLine.click();
    await expect(postBox).toHaveClass(/\bselected\b/);
    await expectOutlined(postBox);
    await expectShownLines(page, definition.lines[0], definition.lines[1]);
    await expect(pathStep(page, "createOrder")).toHaveAttribute("aria-current", "true");
    await expectTintedLines(page, place.lines);
  });

  await test.step("a file opened from Add adds a definition's card with ⊕", async () => {
    const retry = "src/lib/retry.ts";
    const withRetry = lineOf(readFile(join(folder, retry)), "export async function withRetry");
    await openAdd(page);
    await enterText(addInput(page), "retry.ts");
    await selectResult(page, result(page, `file ${retry}`));
    await addInput(page).press("Enter");
    await expectShownLines(page, 1, readFile(join(folder, retry)).split("\n").length - 1);
    await expectPath(page, [retry], retry);
    const add = panel.getByRole("button", { name: `Add the definition on line ${withRetry} to the page` });
    await add.click();
    await expect(card(page, "withRetry")).toBeVisible();
    await expect(add).toBeHidden();
    await expectPath(page, [retry], retry);
  });

  await test.step("an entrance card opens its lines in unfold.toml", async () => {
    await openAdd(page);
    await enterText(addInput(page), "Place an order");
    await selectResult(page, result(page, /^entrance Place an order/));
    await addInput(page).press("Enter");
    const entrance = card(page, "Server · Place an order");
    await expect(entrance).toBeVisible();
    await entrance.hover();
    await entrance.getByRole("button", { name: "View code" }).click();
    const declared = lineOf(readFile(join(folder, "unfold.toml")), 'name = "Place an order"');
    await expectShownLines(page, declared, declared);
    await expectPath(page, ["unfold.toml", "Server", "Place an order"], "Place an order");
    // Reaching `⊕` scrolled the lines sideways; a new view starts at its left, its numbers showing.
    await expect(codeLine(page, declared).locator(".code-number")).toBeInViewport();
  });

  const part = graphNode(nodes, REPORT_CASE);
  const control = graphNode(nodes, part.parent ?? "");
  await test.step("a file changed on disk shows whole until the background compile catches up", async () => {
    await clickOnCanvas(page, reportCase);
    await expectShownLines(page, control.lines[0], control.lines[1]);
    const cliLines = readFile(join(folder, CLI)).split("\n").length - 1;
    editFile(join(folder, CLI), 'logger.info("Usage', 'logger.error("Usage');
    // The changed state lasts only until the compile catches up, so it is checked on every frame.
    await page.waitForFunction(
      ({ text, count }) => {
        const shown = document.querySelector("[aria-label='Code panel']");
        const banner = shown?.querySelector("[role='status']")?.textContent ?? "";
        const numbers = [...(shown?.querySelectorAll(".code-line .code-number") ?? [])].map((number) => number.textContent);
        const marked = shown?.querySelectorAll(".code-line.marked, .code-mark").length ?? 0;
        return banner.includes(text) && numbers.length === count && numbers[0] === "1" && marked === 0;
      },
      { text: "Changed since the last compile · showing the whole file", count: cliLines },
      { polling: "raf" },
    );
    await expect(codeBanner(page)).toBeHidden();
    await expectShownLines(page, control.lines[0], control.lines[1]);
    await expectTintedLines(page, part.lines);
    await expect(main.locator(".flow-call-name", { hasText: /^logger\.error$/ })).toBeVisible();
    await expect(main.locator(".flow-call-name", { hasText: /^logger\.info$/ })).toHaveCount(0);
  });

  await test.step("Open in editor runs the editor command with the file and line", async () => {
    await panel.getByRole("button", { name: "Open in editor" }).click();
    await expect.poll(() => readFileOrNull(opened)).toBe(`${join(folder, CLI)}\n${part.lines[0]}\n`);
  });

  await expect(boardView(page)).toBeVisible();
  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});

function readFileOrNull(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}
