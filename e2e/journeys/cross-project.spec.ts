/**
 * Boards that span projects (redesign §4.8, R-7): a page of a ts-orders board holds a card from
 * py-orders, found with Add across both; a link between them is drawn by hand and labeled; when
 * py-orders' shared name changes, its card becomes a placeholder until the name matches again.
 */

import type { Page } from "@playwright/test";
import { addBox, addInput, openAdd, result, selectResult } from "../src/add";
import {
  activeBoardRef,
  boardView,
  card,
  cardMenu,
  chooseMenuPath,
  layerRow,
  onThisPage,
  panUntilHittable,
  readBoard,
  selected,
} from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, traceEntrance } from "../src/flows";
import { boardTab, dialog, projectMenu, projectRow, selectProject, showHome } from "../src/home";
import { enterText } from "../src/window";

const PY_PLACE = "src/orders_playground/services/order_service.py#class:OrderService/method:place";

/** Renames a project's shared name in Project settings → General (empty: the folder's name). */
async function renameProject(page: Page, from: string, to: string): Promise<void> {
  await showHome(page);
  await projectMenu(page, from, "Settings…");
  const settings = dialog(page, `${from} settings`);
  await expect(settings).toBeVisible();
  await enterText(settings.getByRole("textbox", { name: "Name" }), to);
  await settings.getByRole("button", { name: "Save" }).click();
  await expect(settings).toBeHidden();
}

test.describe("boards across projects", () => {
  test("a card from another project, a drawn link, and a placeholder after a rename", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox, ["ts-orders", "py-orders"]);
    const [orders = ""] = folders;
    await selectProject(page, "ts-orders");
    await traceEntrance(page, "POST", "/orders");
    const board = await activeBoardRef(page);

    await test.step("Add puts a card from the other project on the page", async () => {
      await openAdd(page);
      await enterText(addInput(page), "place");
      const python = result(page, "Function OrderService.place, src/orders_playground/services/order_service.py");
      await expect(python).toBeVisible();
      await selectResult(page, python);
      await addInput(page).press("Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(card(page, PY_PLACE).locator(".code-view")).toBeVisible();
      await expect(card(page, PY_PLACE).locator(".card-header .monogram")).toHaveText("P");
    });

    await test.step("the board file names the other project and the card's project", async () => {
      await expect
        .poll(() => {
          const file = readBoard(orders, board.id);
          return { projects: file.projects, refs: file.pages[0]?.cards.filter((entry) => entry.ref.project !== undefined).map((entry) => entry.ref) };
        })
        .toEqual({ projects: ["py-orders"], refs: [{ block: PY_PLACE, project: "py-orders" }] });
    });

    await test.step("On this page shows each card's project", async () => {
      await expect(onThisPage(page).locator(".layer-monogram .monogram")).not.toHaveCount(0);
      await expect(layerRow(page, "OrderService.place").locator(".monogram")).toHaveText("P");
      await expect(layerRow(page, "createOrder").locator(".monogram")).toHaveText("T");
    });

    await test.step("a link drawn by hand joins cards from the two projects", async () => {
      await cardMenu(card(page, CREATE_ORDER), "Draw link…");
      await expect(boardView(page).getByRole("status").filter({ hasText: "Click the card to link to" })).toBeVisible();
      const target = card(page, PY_PLACE).locator(".card-header");
      await panUntilHittable(page, target);
      await target.click();
      await expect(boardView(page).getByRole("status").filter({ hasText: "Click the card to link to" })).toBeHidden();
      await expect(selected(page).getByRole("heading", { name: "Link", exact: true })).toBeVisible();
      await enterText(selected(page).getByRole("textbox", { name: "Label" }), "calls over HTTP");
      await expect(boardView(page).locator(".board-link-label")).toContainText("calls over HTTP");
      await expect
        .poll(() => readBoard(orders, board.id).pages[0]?.links?.map((link) => link.label?.text))
        .toEqual(["calls over HTTP"]);
    });

    await test.step("Share › Copy Mermaid draws the link as a dashed, labeled arrow", async () => {
      await boardView(page).getByRole("button", { name: "Share" }).click();
      await chooseMenuPath(page, "Share", ["Copy Mermaid"]);
      await expect.poll(() => app.clipboardText()).toContain(`-.->|"calls over HTTP"|`);
    });

    await test.step("renaming the other project's shared name leaves a placeholder", async () => {
      await renameProject(page, "py-orders", "py-renamed");
      await expect(projectRow(page, "py-renamed")).toBeVisible();
      await boardTab(page, "POST /orders").click();
      const placeholder = boardView(page).locator(`.placeholder-card[data-key='${PY_PLACE}']`);
      await expect(placeholder).toBeVisible();
      await expect(placeholder).toHaveAccessibleName(/, from py-orders, not connected$/);
      await expect(placeholder.getByText("From py-orders · not connected")).toBeVisible();
      await expect(placeholder.getByRole("button", { name: "Connect project" })).toBeVisible();
      await expect(onThisPage(page).getByText("not connected")).toBeVisible();
      // The drawn link still joins the placeholder.
      await expect(boardView(page).locator(".board-link-label")).toContainText("calls over HTTP");
    });

    await test.step("renaming it back reads the card again", async () => {
      await renameProject(page, "py-renamed", "py-orders");
      await expect(projectRow(page, "py-orders")).toBeVisible();
      await boardTab(page, "POST /orders").click();
      await expect(card(page, PY_PLACE).locator(".code-view")).toBeVisible();
      await expect(boardView(page).locator(".placeholder-card")).toHaveCount(0);
    });
  });
});
