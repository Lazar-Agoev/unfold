/**
 * Every row of the UI shell's "Where V1's parts went" table and its keyboard table has a home in the
 * journeys: `e2e/src/coverage.ts` maps each row to the test steps that exercise it, and this check
 * reads `docs/ui-shell.md` and the journey sources so neither can drift (a new row without a home, a
 * mapped step that was renamed or deleted, or a mapping for a row that no longer exists all fail).
 * It runs without the app.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { KEYBOARD_HOMES, MAP_HOMES, type Home } from "../src/coverage";
import { repoRoot } from "../src/paths";

const UI_SHELL = join(repoRoot, "docs", "ui-shell.md");

/** The first column of every body row of the first table under the heading `heading`. */
function firstColumn(markdown: string, heading: string): string[] {
  const start = markdown.indexOf(`\n${heading}\n`);
  expect(start, `docs/ui-shell.md has a section "${heading}"`).toBeGreaterThanOrEqual(0);
  const lines = markdown.slice(start + 1).split("\n");
  const first = lines.findIndex((line) => line.startsWith("| "));
  const end = lines.findIndex((line, index) => index > first && !line.startsWith("| "));
  // Skip the header row and its rule.
  return lines.slice(first + 2, end < 0 ? undefined : end).map((line) => {
    const cell = line.slice(2).split(/(?<!\\)\|/)[0] ?? "";
    return cell.replaceAll("\\|", "|").replaceAll("**", "").replaceAll("`", "").trim();
  });
}

/** The source of a journey spec. */
const sources = new Map<string, string>();
function source(spec: string): string {
  let text = sources.get(spec);
  if (text === undefined) {
    text = readFileSync(join(repoRoot, "e2e", "journeys", spec), "utf8");
    sources.set(spec, text);
  }
  return text;
}

/** A string literal as it is written in a spec (`"…"`), so titles and step names match exactly. */
function literal(text: string): string {
  return JSON.stringify(text);
}

function checkHomes(table: string, rows: readonly string[], homes: Readonly<Record<string, readonly Home[]>>): void {
  for (const row of rows) {
    const mapped = homes[row];
    expect(mapped, `${table} row "${row}" has a home in e2e/src/coverage.ts`).toBeDefined();
    expect(mapped?.length ?? 0, `${table} row "${row}" names at least one step`).toBeGreaterThan(0);
    for (const home of mapped ?? []) {
      const text = source(home.spec);
      expect(text.includes(literal(home.test)), `${home.spec} has the test ${literal(home.test)} (for "${row}")`).toBe(true);
      expect(text.includes(`test.step(${literal(home.step)}`), `${home.spec} has the step ${literal(home.step)} (for "${row}")`).toBe(true);
    }
  }
  for (const row of Object.keys(homes)) {
    expect(rows, `e2e/src/coverage.ts maps "${row}", a row ${table} no longer has`).toContain(row);
  }
}

test("every row of the UI shell's \"Where V1's parts went\" table has a home in the journeys", () => {
  const rows = firstColumn(readFileSync(UI_SHELL, "utf8"), "## Where V1's parts went");
  expect(rows.length).toBeGreaterThanOrEqual(18);
  checkHomes("the \"Where V1's parts went\" table", rows, MAP_HOMES);
});

test("every action of the UI shell's keyboard table runs in the journeys", () => {
  const rows = firstColumn(readFileSync(UI_SHELL, "utf8"), "## Keyboard");
  expect(rows.length).toBeGreaterThanOrEqual(23);
  checkHomes("the keyboard table", rows, KEYBOARD_HOMES);
});
