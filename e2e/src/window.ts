/**
 * Driving one window at the transport level: engine commands through `window.unfold` (exactly what the
 * UI uses), the events it receives, its engine connection, and a folder dropped on it the way the
 * operating system drops one. Waits are on state (locators, `expect.poll`), never fixed sleeps.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldEvent } from "@unfold/desktop/unfold-api";
import type { UnfoldApp } from "./app";

/** The engine and shell events the recorder keeps, with the time each arrived. */
const RECORDED = [
  "workspace_changed",
  "project_state",
  "project_reset",
  "graph_updated",
  "files_changed",
  "boards_changed",
  "board_conflict",
  "problems_changed",
  "config_changed",
  "preferences_changed",
  "board_state_transferred",
  "focus_board",
  "project_added",
] as const;

type RecordedType = (typeof RECORDED)[number];

/** Sizes the window and installs the event recorder once its page has loaded. */
export async function prepareWindow(app: UnfoldApp, page: Page): Promise<void> {
  await page.waitForLoadState("domcontentloaded");
  await app.resize(page, 1500, 950);
  await installRecorder(page);
}

/** Records every {@link RECORDED} event in `window.__e2e` (idempotent). */
async function installRecorder(page: Page): Promise<void> {
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
class CommandError extends Error {
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

interface WindowInfo {
  readonly windowId: number;
  readonly key: string;
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

/** Waits for an event of `type` after `mark` that `matches`, and returns it with its arrival time. */
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
async function connection(page: Page): Promise<string> {
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

/**
 * Drops files or folders on the window at the middle of `target` (default: the content area), as
 * the operating system does: Chromium builds the drag data from the paths, so the preload sees real
 * dropped files.
 */
export async function dropPaths(page: Page, paths: readonly string[], target?: Locator): Promise<void> {
  const box = await (target ?? page.locator("#content-area")).boundingBox();
  if (!box) {
    throw new Error("nothing to drop on");
  }
  const session = await page.context().newCDPSession(page);
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const data = { items: [], files: [...paths], dragOperationsMask: 1 };
  for (const type of ["dragEnter", "dragOver", "drop"] as const) {
    await session.send("Input.dispatchDragEvent", { type, ...point, data });
  }
  await session.detach();
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
