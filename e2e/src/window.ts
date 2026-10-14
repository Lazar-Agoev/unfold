/**
 * Driving the window at the transport level: engine commands through `window.unfold` (exactly what the
 * UI uses) and its engine connection. Waits are on state (locators, `expect.poll`), never fixed sleeps.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";

/** Sizes the window once its page has loaded. */
export async function prepareWindow(app: UnfoldApp, page: Page): Promise<void> {
  await page.waitForLoadState("domcontentloaded");
  await app.resize(page, 1500, 950);
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
