/**
 * What the window journeys need from the main process: which windows it focused, finding a window by
 * its key, and the application menu's items, clicked as the person would.
 */

import type { Page } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { windowInfo } from "./window";

/** Records which windows the main process focuses from now on (`BrowserWindow#focus`). */
export async function spyOnFocus(app: UnfoldApp): Promise<void> {
  await app.electron.evaluate(({ BrowserWindow }) => {
    const store = globalThis as unknown as { __focused?: number[] };
    if (store.__focused) {
      return;
    }
    const focused: number[] = [];
    store.__focused = focused;
    const prototype = BrowserWindow.prototype as unknown as { focus: (this: Electron.BrowserWindow) => void };
    const original = prototype.focus;
    prototype.focus = function focus(this: Electron.BrowserWindow) {
      focused.push(this.id);
      original.call(this);
    };
  });
}

/** The ids of the windows the main process focused since {@link spyOnFocus}, oldest first. */
export async function focusedWindows(app: UnfoldApp): Promise<number[]> {
  return app.electron.evaluate(() => (globalThis as unknown as { __focused?: number[] }).__focused ?? []);
}

/** The open window whose key is `key`. */
export async function windowWithKey(app: UnfoldApp, key: string): Promise<Page> {
  for (const page of app.windows()) {
    if ((await windowInfo(page)).key === key) {
      return page;
    }
  }
  throw new Error(`no window has key ${key}`);
}

/**
 * Clicks an item of the application menu (`["File", "Add Project…"]`) for the window of `page`, as
 * choosing it with the mouse does: Electron passes that window to the item.
 */
export async function clickAppMenu(app: UnfoldApp, page: Page, path: readonly string[]): Promise<void> {
  const id = await app.windowId(page);
  await app.electron.evaluate(({ BrowserWindow, Menu }, { labels, windowId }) => {
    let items = Menu.getApplicationMenu()?.items ?? [];
    let item: Electron.MenuItem | undefined;
    for (const label of labels) {
      item = items.find((candidate) => candidate.label === label);
      if (!item) {
        throw new Error(`no menu item ${label}`);
      }
      items = item.submenu?.items ?? [];
    }
    const window = BrowserWindow.fromId(windowId);
    if (!item || !window) {
      throw new Error("nothing to click");
    }
    // Typed as a bare `Function`: Electron calls it with the event, the window, and its contents.
    const click = item.click as (event: unknown, window: Electron.BrowserWindow, contents: Electron.WebContents) => void;
    click.call(item, undefined, window, window.webContents);
  }, { labels: [...path], windowId: id });
}
