/**
 * The code panel beside the board (docs/ui-shell.md#the-code-panel): its path, its banner, its lines with
 * their numbers, and what it marks; and the outlines it draws on the canvas. Also the playgrounds'
 * compiled graph (their goldens), so a journey can name real keys and lines without guessing them.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { goldensDir } from "./paths";
import type { PlaygroundName } from "./sandbox";

export function codePanel(page: Page): Locator {
  return page.getByRole("complementary", { name: "Code panel" });
}

/** A step of the panel's path, by its label. */
export function pathStep(page: Page, label: string): Locator {
  return codePanel(page).locator(".code-path").getByRole("button", { name: label, exact: true });
}

/** The banner over the lines when they can't be trusted. */
export function codeBanner(page: Page): Locator {
  return codePanel(page).getByRole("status");
}

/** A shown line, by its real line number. */
export function codeLine(page: Page, line: number): Locator {
  return codePanel(page)
    .locator(".code-line")
    .filter({ has: page.locator(".code-number", { hasText: new RegExp(`^${line}$`) }) });
}

/** The numbers `first` to `last`, as the panel writes them. */
function numbers(first: number, last: number): string[] {
  return Array.from({ length: last - first + 1 }, (_, index) => String(first + index));
}

/** Waits until the panel shows lines `first` to `last`, and only those. */
export async function expectShownLines(page: Page, first: number, last: number): Promise<void> {
  await expect(codePanel(page).locator(".code-line .code-number")).toHaveText(numbers(first, last));
}

/** Waits until the panel tints lines `first` to `last` (the selection's), or none. */
export async function expectTintedLines(page: Page, lines: readonly [number, number] | null): Promise<void> {
  const tinted = codePanel(page).locator(".code-line.marked .code-number");
  if (lines === null) {
    await expect(tinted).toHaveCount(0);
  } else {
    await expect(tinted).toHaveText(numbers(lines[0], lines[1]));
  }
}

/** The text the panel fills as a call's own expression ("" when nothing is). */
export async function markedExpression(page: Page): Promise<string> {
  return codePanel(page)
    .locator(".code-mark")
    .evaluateAll((runs) => runs.map((run) => run.textContent).join(""));
}

/** Waits until the path reads `labels`, with the step the panel shows (bold) being `showing`. */
export async function expectPath(page: Page, labels: readonly string[], showing: string): Promise<void> {
  const path = codePanel(page).locator(".code-path");
  await expect(path.locator("button.code-crumb")).toHaveText([...labels]);
  await expect(path.locator("[aria-current='true']")).toHaveText(showing);
}

/**
 * Whether the canvas outlines `target` on its own shape: with the selection's outline, or (`lit`) the
 * lighter one a line hovered in the panel draws. An outline is drawn at the rect of what it goes around.
 */
async function isOutlined(target: Locator, lit: boolean): Promise<boolean> {
  return target.evaluate((element, wanted) => {
    const box = element.getBoundingClientRect();
    const outlines = element.closest(".flow")?.querySelectorAll(".flow-outline") ?? [];
    return [...outlines].some((outline) => {
      const at = outline.getBoundingClientRect();
      const same = Math.abs(at.x - box.x) < 1 && Math.abs(at.y - box.y) < 1 && Math.abs(at.width - box.width) < 1 && Math.abs(at.height - box.height) < 1;
      return same && outline.classList.contains("lit") === wanted;
    });
  }, lit);
}

/** Waits until the canvas outlines `target` (`lit`: the lighter outline of a hovered line). */
export async function expectOutlined(target: Locator, lit = false): Promise<void> {
  await expect.poll(() => isOutlined(target, lit), { message: lit ? "a hovered line lights it" : "the selection's outline is on it" }).toBe(true);
}

// --- The compiled graph -----------------------------------------------------------------------

/** A block of a playground's graph, as its golden records it. */
interface GraphNode {
  readonly key: string;
  readonly parent?: string;
  readonly kind: string;
  readonly name?: string;
  /** UTF-8 bytes into the file. */
  readonly range: readonly [number, number];
  readonly lines: readonly [number, number];
}

/** The blocks of a playground's compiled graph, from its golden (`testdata/goldens/<name>.jsonl`). */
export function graphNodes(playground: PlaygroundName): GraphNode[] {
  return readFileSync(join(goldensDir, `${playground}.jsonl`), "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as { readonly t: string })
    .filter((record): record is GraphNode & { readonly t: "node" } => record.t === "node");
}

/** The block `key` of `nodes`, failing loudly when the graph has no such block. */
export function graphNode(nodes: readonly GraphNode[], key: string): GraphNode {
  const node = nodes.find((candidate) => candidate.key === key);
  if (!node) {
    throw new Error(`the graph has no block ${key}`);
  }
  return node;
}

/** The one-based number of the one line of `text` that contains `needle`. */
export function lineOf(text: string, needle: string): number {
  const lines = text.split("\n");
  const found = lines.flatMap((line, index) => (line.includes(needle) ? [index + 1] : []));
  if (found.length !== 1 || found[0] === undefined) {
    throw new Error(`expected exactly one line with ${JSON.stringify(needle)}`);
  }
  return found[0];
}
