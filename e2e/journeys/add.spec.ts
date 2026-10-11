/**
 * Add (docs/ui-shell.md#screens, cross-project search): one box that finds entrances, functions, files, and text in every
 * connected project and puts what you pick on a page, and runs commands after `>`. It replaces V1's
 * Search and Entrances views, the command palette, Go to File, and Go to Symbol.
 */

import { addBox, addInput, addPreview, addResults, addTab, openAdd, result, selectResult } from "../src/add";
import { activeBoardRef, boardView, card, cards, pagesList } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, PLACE, traceEntrance } from "../src/flows";
import { boardTab, projectId, sidebar, toast } from "../src/home";
import { enterText } from "../src/window";

const PY_PLACE = "src/orders_playground/services/order_service.py#class:OrderService/method:place";

test.describe("Add", () => {
  test("on a board: A, ⌘K and / open it, the map, Enter and ⌘Enter add, Escape closes", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");
    const before = await cards(page).count();

    await test.step("A, ⌘K and / open Add; Escape closes it", async () => {
      for (const key of ["a", "Meta+k", "/"]) {
        await page.keyboard.press(key);
        await expect(addInput(page)).toBeFocused();
        await page.keyboard.press("Escape");
        await expect(addBox(page)).toBeHidden();
      }
    });

    await test.step("the map shows each application's entrances and marks the one on the page", async () => {
      await openAdd(page);
      await expect(addTab(page, "Entrances")).toHaveAttribute("aria-selected", "true");
      await expect(addBox(page).getByText("Adds to “Page 1”")).toBeVisible();
      const map = addBox(page).getByRole("region", { name: "ts-orders" });
      for (const application of ["cli", "scheduler", "server", "worker"]) {
        await expect(map.getByRole("group", { name: application, exact: true })).toBeVisible();
      }
      const server = map.getByRole("group", { name: "server", exact: true });
      await expect(server.getByRole("button", { name: "POST /orders on page" })).toBeVisible();
      await expect(server.getByRole("button", { name: "GET /health", exact: true })).toBeVisible();
    });

    await test.step("choosing an entrance on the map adds it to the page", async () => {
      await addBox(page).getByRole("group", { name: "server", exact: true }).getByRole("button", { name: "GET /health", exact: true }).click();
      await expect(addBox(page)).toBeHidden();
      await expect(boardView(page).getByRole("article", { name: "GET /health", exact: true })).toBeVisible();
    });

    await test.step("Enter adds the selected function to the current page", async () => {
      await openAdd(page);
      await enterText(addInput(page), "validateOrder");
      const hit = result(page, /^Function validateOrder, src\/lib\/validate\.ts$/);
      await expect(hit).toBeVisible();
      await selectResult(page, hit);
      await addInput(page).press("Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(card(page, "src/lib/validate.ts#function:validateOrder")).toBeVisible();
      expect(await cards(page).count()).toBeGreaterThan(before);
      await expect(pagesList(page).getByRole("option")).toHaveCount(1);
    });

    await test.step("⌘Enter adds it on a new page", async () => {
      await openAdd(page);
      await enterText(addInput(page), "OrderService.place");
      const hit = result(page, /^Function OrderService\.place, /);
      await expect(hit).toBeVisible();
      await selectResult(page, hit);
      await addInput(page).press("Meta+Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(pagesList(page).getByRole("option")).toHaveCount(2);
      await expect(pagesList(page).getByRole("option", { name: /^Page 2/ })).toHaveAttribute("aria-selected", "true");
      await expect(card(page, PLACE)).toBeVisible();
      await expect(cards(page)).toHaveCount(1);
    });
  });

  test("search across two projects: kind tabs and counts, the preview, Look in, and Enter from Home", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox, ["ts-orders", "py-orders"]);
    const python = await projectId(page, "py-orders");

    await test.step("Look in limits the map to one project", async () => {
      await openAdd(page);
      await expect(addBox(page).getByText("Adds to a new draft")).toBeVisible();
      await expect(addBox(page).getByRole("region", { name: "ts-orders" })).toBeVisible();
      await expect(addBox(page).getByRole("region", { name: "py-orders" })).toBeVisible();
      const lookIn = addBox(page).getByRole("radiogroup", { name: "Look in" });
      await expect(lookIn.getByRole("radio", { name: /^All projects 36$/ })).toBeChecked();
      await lookIn.getByRole("radio", { name: /^py-orders/ }).click();
      await expect(addBox(page).getByRole("region", { name: "ts-orders" })).toBeHidden();
      await expect(addBox(page).getByRole("region", { name: "py-orders" })).toBeVisible();
      await lookIn.getByRole("radio", { name: /^All projects/ }).click();
      await expect(addBox(page).getByRole("region", { name: "ts-orders" })).toBeVisible();
    });

    await test.step("typing searches every project, with counts per kind and each row's project", async () => {
      await enterText(addInput(page), "place");
      const ts = result(page, "Function OrderService.place, src/services/order-service.ts");
      const py = result(page, "Function OrderService.place, src/orders_playground/services/order_service.py");
      await expect(ts).toBeVisible();
      await expect(py).toBeVisible();
      await expect(ts.locator(".monogram")).toHaveText("T");
      await expect(py.locator(".monogram")).toHaveText("P");
      await expect(addBox(page).getByText(/^2 projects · \d+ results$/)).toBeVisible();
      await expect(addTab(page, "All")).toHaveAttribute("aria-selected", "true");
      await expect(addTab(page, "Functions")).toHaveText(/^Functions\s*3$/);
      await expect(addTab(page, "Entrances")).toHaveText(/^Entrances\s*0$/);
    });

    await test.step("Tab and ⇧Tab switch the kind", async () => {
      await addInput(page).press("Tab");
      await expect(addTab(page, "Entrances")).toHaveAttribute("aria-selected", "true");
      await addInput(page).press("Tab");
      await expect(addTab(page, "Functions")).toHaveAttribute("aria-selected", "true");
      await expect(addResults(page).getByRole("option")).toHaveCount(3);
      await addInput(page).press("Shift+Tab");
      await addInput(page).press("Shift+Tab");
      await expect(addTab(page, "All")).toHaveAttribute("aria-selected", "true");
    });

    await test.step("the preview shows the selection's location, code, and relations", async () => {
      const preview = addPreview(page);
      await selectResult(page, result(page, "Function OrderService.place, src/services/order-service.ts"));
      await expect(preview.locator(".add-preview-location")).toHaveText(/ts-orders\s*·\s*src\/services\/order-service\.ts:13$/);
      await expect(preview.getByRole("heading", { name: "place" })).toBeVisible();
      await expect(preview).toContainText("place(input: unknown): Order {");
      await expect(preview).toContainText("Reached from");
      await expect(preview).toContainText("Calls");
      await expect(preview).toContainText("Called by");
      await expect(preview.getByRole("button", { name: /^Add to a new draft/ })).toBeVisible();
      await selectResult(page, result(page, "Function OrderService.place, src/orders_playground/services/order_service.py"));
      await expect(preview.locator(".add-preview-location")).toHaveText(/py-orders\s*·\s*src\/orders_playground\/services\/order_service\.py:20$/);
    });

    await test.step("from Home, Enter starts a new draft in the result's project", async () => {
      await addInput(page).press("Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(boardTab(page, "place")).toHaveAttribute("aria-selected", "true");
      await expect(card(page, PY_PLACE)).toBeVisible();
      expect((await activeBoardRef(page)).project).toBe(python);
    });
  });

  test("text search: match case, whole word, and regular expressions", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    const line = (path: string, number: number) => result(page, new RegExp(`^${path.replace(/[.]/g, "\\.")} line ${number}: `));

    await test.step("the Text tab lists every matching line, case-insensitively", async () => {
      await openAdd(page);
      await enterText(addInput(page), "Retry");
      await addTab(page, "Text").click();
      await expect(addTab(page, "Text")).toHaveAttribute("aria-selected", "true");
      await expect(line("src/cli.ts", 18)).toBeVisible();
      await expect(line("src/lib/retry.ts", 16)).toBeVisible();
      await expect(line("src/cli.ts", 19)).toBeVisible();
      await expect(addBox(page).getByRole("group", { name: "Text search options" })).toBeVisible();
    });

    await test.step("Match case keeps only the lines with Retry as typed", async () => {
      const option = addBox(page).getByRole("button", { name: "Match case" });
      await option.click();
      await expect(option).toHaveAttribute("aria-pressed", "true");
      await expect(line("src/cli.ts", 18)).toBeHidden();
      await expect(line("src/lib/retry.ts", 16)).toBeVisible();
      await option.click();
      await expect(line("src/cli.ts", 18)).toBeVisible();
    });

    await test.step("Match whole word drops retryFailed", async () => {
      const option = addBox(page).getByRole("button", { name: "Match whole word" });
      await option.click();
      await expect(option).toHaveAttribute("aria-pressed", "true");
      await expect(line("src/cli.ts", 19)).toBeHidden();
      await expect(line("src/cli.ts", 18)).toBeVisible();
      await option.click();
      await expect(line("src/cli.ts", 19)).toBeVisible();
    });

    await test.step("a regular expression matches by pattern", async () => {
      await enterText(addInput(page), "retry\\w+");
      await expect(addBox(page).getByText("No results.")).toBeVisible();
      const option = addBox(page).getByRole("button", { name: "Use regular expression" });
      await option.click();
      await expect(option).toHaveAttribute("aria-pressed", "true");
      await expect(line("src/cli.ts", 19)).toBeVisible();
      await expect(line("src/lib/retry.ts", 11)).toBeVisible();
      await expect(line("src/cli.ts", 18)).toBeHidden();
      await expect(addTab(page, "Text")).toHaveText(/^Text\s*6$/);
    });
  });

  test("commands: `>`, ⇧⌘P, ⌘P on Files, ⌘T on Functions, and Copy Diagnostics", async ({ sandbox, launch }) => {
    const { app, page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");

    await test.step("typing > switches Add to commands, grouped", async () => {
      await openAdd(page);
      await enterText(addInput(page), ">");
      const commands = addBox(page).getByRole("listbox", { name: "Commands" });
      await expect(commands.getByRole("option", { name: "Fit", exact: true })).toBeVisible();
      await expect(commands.getByRole("option", { name: "Copy Mermaid", exact: true })).toBeVisible();
      await expect(addBox(page).getByRole("tablist", { name: "Kinds" })).toBeHidden();
    });

    await test.step("a filtered command runs on Enter", async () => {
      await enterText(addInput(page), ">toggle sidebar");
      await expect(addBox(page).getByRole("listbox", { name: "Commands" }).getByRole("option").first()).toHaveAccessibleName("Toggle Sidebar");
      await addInput(page).press("Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(sidebar(page)).toBeHidden();
    });

    await test.step("⇧⌘P opens Add in commands mode", async () => {
      await page.keyboard.press("Meta+Shift+P");
      await expect(addInput(page)).toHaveValue(/^>\s*$/);
      await enterText(addInput(page), ">toggle sidebar");
      await expect(addBox(page).getByRole("listbox", { name: "Commands" }).getByRole("option").first()).toHaveAccessibleName("Toggle Sidebar");
      await addInput(page).press("Enter");
      await expect(sidebar(page)).toBeVisible();
    });

    await test.step("Copy Diagnostics is one of Add's commands", async () => {
      await page.keyboard.press("Meta+Shift+P");
      await enterText(addInput(page), ">copy diagnostics");
      await expect(addBox(page).getByRole("listbox", { name: "Commands" }).getByRole("option").first()).toHaveAccessibleName("Copy Diagnostics");
      await addInput(page).press("Enter");
      await expect(toast(page, "Copied diagnostics")).toBeVisible();
      await expect.poll(() => app.clipboardText()).toContain("Electron:");
    });

    await test.step("⌘P opens Add on Files", async () => {
      await page.keyboard.press("Meta+P");
      await expect(addTab(page, "Files")).toHaveAttribute("aria-selected", "true");
      await enterText(addInput(page), "order-service");
      await expect(result(page, "File src/services/order-service.ts")).toBeVisible();
      await expect(addTab(page, "Files")).toHaveAttribute("aria-selected", "true");
      await addInput(page).press("Escape");
      await expect(addBox(page)).toBeHidden();
    });

    await test.step("⌘T opens Add on Functions", async () => {
      await page.keyboard.press("Meta+T");
      await expect(addTab(page, "Functions")).toHaveAttribute("aria-selected", "true");
      await enterText(addInput(page), "createOrder");
      await expect(result(page, /^Function createOrder, src\/routes\/orders\.ts$/)).toBeVisible();
      await addInput(page).press("Escape");
      await expect(addBox(page)).toBeHidden();
    });
  });
});
