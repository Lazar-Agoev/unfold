/**
 * Driving one window: engine commands through `window.unfold` (exactly what the UI uses), the
 * events it receives, and locators for the parts of the window the journeys use. Waits are on
 * state (locators, `expect.poll`), never fixed sleeps.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldEvent } from "@unfold/desktop/unfold-api";
import type { UnfoldApp } from "./app";

/** The engine and shell events the recorder keeps, with the time each arrived. */
const RECORDED = [
  "graph_updated",
  "graph_status",
  "files_changed",
  "boards_changed",
  "board_conflict",
  "problems_changed",
  "folder_opened",
  "config_changed",
  "folder_changed",
  "board_state_transferred",
] as const;

export type RecordedType = (typeof RECORDED)[number];

/** Sizes the window and installs the event recorder once its page has loaded. */
export async function prepareWindow(app: UnfoldApp, page: Page): Promise<void> {
  await page.waitForLoadState("domcontentloaded");
  await app.resize(page, 1500, 950);
  await installRecorder(page);
}

/** Records every {@link RECORDED} event in `window.__e2e` (idempotent; again after a reload). */
export async function installRecorder(page: Page): Promise<void> {
  await page.waitForFunction(() => typeof window.unfold === "object");
  await page.evaluate((types) => {
    if (window.__e2e) {
      return;
    }
    const recorder: { events: { at: number; event: UnfoldEvent }[] } = { events: [] };
    window.__e2e = recorder;
    for (const type of types) {
      window.unfold.on(type, (event) => {
        recorder.events.push({ at: Date.now(), event });
      });
    }
  }, [...RECORDED]);
}

/** A command rejected by the engine or the main process. */
export class CommandError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(`${code}: ${message}`);
    this.code = code;
  }
}

type Settled = { readonly ok: true; readonly value: unknown } | { readonly ok: false; readonly code: string; readonly message: string };

function settle(result: Settled): unknown {
  if (result.ok) {
    return result.value;
  }
  throw new CommandError(result.code, result.message);
}

/**
 * Runs one engine command from the window. The result is the engine's JSON; journeys read the few
 * fields they check through a type they name (the generated types live in the UI package).
 */
export async function invoke<T>(page: Page, command: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await page.evaluate(
    ([name, input]) =>
      window.unfold.invoke(name, input).then(
        (value): Settled => ({ ok: true, value }),
        (error: unknown): Settled => {
          const { code, message } = error as { code?: unknown; message?: unknown };
          return { ok: false, code: String(code), message: String(message) };
        },
      ),
    [command, args] as const,
  );
  return settle(result) as T;
}

/** Runs one main-process window command from the window. */
export async function windowCommand<T>(page: Page, command: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await page.evaluate(
    ([name, input]) =>
      (window.unfold.window as (command: string, args: unknown) => Promise<unknown>)(name, input).then(
        (value): Settled => ({ ok: true, value }),
        (error: unknown): Settled => {
          const { code, message } = error as { code?: unknown; message?: unknown };
          return { ok: false, code: String(code), message: String(message) };
        },
      ),
    [command, args] as const,
  );
  return settle(result) as T;
}

export interface WindowInfo {
  readonly windowId: number;
  readonly key: string;
  readonly folder: string | null;
}

export function windowInfo(page: Page): Promise<WindowInfo> {
  return windowCommand<WindowInfo>(page, "window_info");
}

/** Every recorded event of `type`, oldest first, with its arrival time. */
export async function events(page: Page, type: RecordedType): Promise<{ readonly at: number; readonly event: UnfoldEvent }[]> {
  return page.evaluate((wanted) => (window.__e2e?.events ?? []).filter((entry) => entry.event.type === wanted), type);
}

/** How many events of `type` arrived so far: a mark to wait for later ones. */
export async function eventMark(page: Page, type: RecordedType): Promise<number> {
  return (await events(page, type)).length;
}

/**
 * Waits for an event of `type` after `mark` that `matches`, and returns it with its arrival time.
 */
export async function waitForEvent(
  page: Page,
  type: RecordedType,
  mark: number,
  matches: (event: UnfoldEvent) => boolean = () => true,
  timeout = 30_000,
): Promise<{ readonly at: number; readonly event: UnfoldEvent }> {
  let found: { at: number; event: UnfoldEvent } | undefined;
  await expect
    .poll(
      async () => {
        found = (await events(page, type)).slice(mark).find((entry) => matches(entry.event));
        return found !== undefined;
      },
      { timeout, message: `waiting for ${type}` },
    )
    .toBe(true);
  if (!found) {
    throw new Error(`no ${type}`);
  }
  return found;
}

/**
 * Starts watching, in the window, for an element matching `selector` to contain `text`, and
 * resolves with the wall-clock time (ms) of the first frame that shows it. Start it before the
 * action, then await it: unlike `expect` polling (100–1000 ms apart), this sees the exact frame.
 */
export function whenTextShows(page: Page, selector: string, text: string, timeout = 30_000): Promise<number> {
  return page.evaluate(
    ([wanted, needle, limit]) =>
      new Promise<number>((resolve, reject) => {
        const started = Date.now();
        const tick = () => {
          const shown = [...document.querySelectorAll(wanted)].some((element) => element.textContent.includes(needle));
          if (shown) {
            resolve(Date.now());
          } else if (Date.now() - started > limit) {
            reject(new Error(`${needle} never showed in ${wanted}`));
          } else {
            requestAnimationFrame(tick);
          }
        };
        tick();
      }),
    [selector, text, timeout] as const,
  );
}

/** The window's engine connection state. */
export async function connection(page: Page): Promise<string> {
  return page.evaluate(() => {
    let state = "";
    window.unfold.onConnection((current) => {
      state = current;
    })();
    return state;
  });
}

export async function expectConnection(page: Page, state: string, timeout = 30_000): Promise<void> {
  await expect.poll(() => connection(page), { timeout, message: `connection ${state}` }).toBe(state);
}

// --- Shell ------------------------------------------------------------------------------------

/** The graph's state in the status bar (§7.9). */
export function graphStatus(page: Page): Locator {
  return page.locator(".statusbar .statusbar-button [role='status']");
}

export async function expectGraphCurrent(page: Page, timeout = 60_000): Promise<void> {
  await expect(graphStatus(page)).toHaveText(/^Graph current/, { timeout });
}

/** Opens `folder` from the welcome screen's Open Folder… (with the picker stubbed). */
export async function openFromWelcome(app: UnfoldApp, page: Page, folder: string): Promise<void> {
  await app.stubFolderDialog(folder);
  await expectConnection(page, "connected");
  await page.locator(".welcome-open").click();
}

/** Presses Compile on the first-Compile screen and waits for a current graph in the workbench. */
export async function firstCompile(page: Page): Promise<void> {
  const button = page.locator(".first-compile-action").getByRole("button", { name: /^Compile/ });
  await button.click();
  await expect(page.locator(".workbench")).toBeVisible({ timeout: 60_000 });
  await expectGraphCurrent(page);
}

/** Shows a sidebar view by its activity bar button. */
export async function showView(page: Page, view: "Files" | "Entrances" | "Search" | "Boards"): Promise<void> {
  const button = page.locator(`.activitybar-button[aria-label='${view}']`);
  // The button toggles: pressing the view that is showing hides the sidebar.
  if ((await button.getAttribute("aria-pressed")) !== "true") {
    await button.click();
  }
  await expect(button).toHaveAttribute("aria-pressed", "true");
}

/** An entrance's row in the Entrances view, by its method column and trigger. */
export function entranceRow(page: Page, method: string, trigger: string): Locator {
  return page
    .getByRole("tree", { name: "Entrances" })
    .locator(".entrance-row")
    .filter({ has: page.locator(".entrance-method", { hasText: new RegExp(`^${escapeRegExp(method)}$`) }) })
    .filter({ has: page.locator(".entrance-trigger", { hasText: new RegExp(`^${escapeRegExp(trigger)}$`) }) });
}

/** Opens an entrance from the Entrances view, expanding applications until its row shows. */
export async function openEntrance(page: Page, method: string, trigger: string): Promise<void> {
  await showView(page, "Entrances");
  const tree = page.getByRole("tree", { name: "Entrances" });
  const row = entranceRow(page, method, trigger);
  await expect(tree.getByRole("treeitem").first()).toBeVisible();
  await expect
    .poll(
      async () => {
        if ((await row.count()) > 0) {
          return true;
        }
        const closed = tree.locator("[role='treeitem'][aria-expanded='false']").first();
        if ((await closed.count()) > 0) {
          await closed.locator(".tree-twisty").click();
        }
        return false;
      },
      { message: `entrance ${method} ${trigger}` },
    )
    .toBe(true);
  await row.first().click();
}

// --- Boards -----------------------------------------------------------------------------------

/** The board tab that is showing. */
export function activeBoard(page: Page): Locator {
  return page.locator(".editor-board:not([hidden])");
}

/** A card on the showing board, by the end of its key (`#function:createOrder`). */
export function card(page: Page, keySuffix: string): Locator {
  return activeBoard(page).locator(`.board-card[data-key$="${keySuffix}"]`);
}

/** Every card on the showing board. */
export function cards(page: Page): Locator {
  return activeBoard(page).locator(".board-card");
}

/** Waits until every card on the showing board has its model. */
export async function expectCardsLoaded(page: Page): Promise<void> {
  await expect(activeBoard(page).locator(".board-card[data-loading]")).toHaveCount(0);
}

/** A resolved call link inside a card or source tab, by its callee's name (`place` matches `orderService.place`). */
export function callLink(from: Locator, name: string): Locator {
  return from.locator(`.code-link.resolved[aria-label="Follow ${name}"], .code-link.resolved[aria-label^="Follow "][aria-label$=".${name}"]`).first();
}

/**
 * Waits until the showing board's camera has stopped moving (focusing a card pans with an
 * animation): its transform unchanged for ten consecutive frames.
 */
export async function cameraSettled(page: Page): Promise<void> {
  await activeBoard(page).evaluate(
    (board) =>
      new Promise<void>((resolve) => {
        const viewport = board.querySelector<HTMLElement>(".react-flow__viewport");
        let last = viewport?.style.transform ?? "";
        let still = 0;
        const tick = () => {
          const now = viewport?.style.transform ?? "";
          still = now === last ? still + 1 : 0;
          last = now;
          if (still >= 10) {
            resolve();
          } else {
            requestAnimationFrame(tick);
          }
        };
        requestAnimationFrame(tick);
      }),
  );
}

/** Whether the point at the middle of `target` hits it (nothing covers it, and it is on screen). */
export async function isHittable(target: Locator): Promise<boolean> {
  return target.evaluate((element) => {
    const box = element.getBoundingClientRect();
    const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
    return hit !== null && (hit === element || element.contains(hit));
  });
}

/**
 * Pans the showing board (a wheel scroll pans the canvas) until `target` can be pointed at: on
 * screen and not under the floating board bar.
 */
export async function panUntilHittable(page: Page, target: Locator): Promise<void> {
  const pane = activeBoard(page).locator(".react-flow__pane");
  await expect
    .poll(
      async () => {
        if (await isHittable(target)) {
          return true;
        }
        const box = await target.boundingBox();
        const area = await pane.boundingBox();
        if (box && area) {
          await page.mouse.move(area.x + area.width / 2, area.y + area.height / 3);
          const dy = box.y + box.height / 2 - (area.y + area.height / 2);
          const dx = box.x < area.x ? box.x - area.x - 40 : box.x + box.width > area.x + area.width ? box.x + box.width - (area.x + area.width) + 40 : 0;
          await page.mouse.wheel(dx, dy);
          await cameraSettled(page);
        }
        return false;
      },
      { message: "pan the canvas until the target is on screen" },
    )
    .toBe(true);
}

/** Follows a resolved call link inside a card. */
export async function follow(from: Locator, name: string): Promise<void> {
  await callLink(from, name).click();
}

/** The open board's ID from the window's tabs. */
export async function activeBoardId(page: Page): Promise<string> {
  const id = await activeBoard(page).locator(".board-tab").getAttribute("data-board-id");
  if (id === null) {
    throw new Error("no board is showing");
  }
  return id;
}

/** The editor tab strip's tab labels. */
export function editorTabs(page: Page): Locator {
  return page.locator(".editor").getByRole("tab");
}

/**
 * Sets a text field's value the way a keystroke does (one `input` event React sees), without the
 * operating system's text input. `fill` goes through it, and macOS's inline predictions then
 * sometimes commit a guessed completion into the field ("draft" + "ontext on things w").
 */
export async function enterText(field: Locator, text: string): Promise<void> {
  await field.focus();
  await field.evaluate((element, value) => {
    const input = element as HTMLInputElement | HTMLTextAreaElement;
    const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    // The prototype's setter, so React's value tracker sees a change (as with a keystroke).
    // eslint-disable-next-line @typescript-eslint/unbound-method -- called with the element as `this`
    const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, text);
  await expect(field).toHaveValue(text);
}

export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
