/**
 * Every row of the redesign's "Where everything goes" table (§6) and keyboard map (§8) has a home in
 * the journeys: `e2e/src/coverage.ts` maps each row to the test steps that exercise it, and this check
 * reads the redesign document and the journey sources so neither can drift (a new row without a home,
 * a mapped step that was renamed or deleted, or a mapping for a row that no longer exists all fail).
 * It runs without the app.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { KEYBOARD_HOMES, MAP_HOMES, type Home } from "../src/coverage";
import { repoRoot } from "../src/paths";

const REDESIGN = join(repoRoot, "docs", "redesign", "studio-redesign.md");

/** The first column of every body row of the table under the heading that starts with `heading`. */
function firstColumn(markdown: string, heading: string): string[] {
  const start = markdown.indexOf(`\n${heading}`);
  expect(start, `the redesign has a section "${heading}"`).toBeGreaterThanOrEqual(0);
  const section = markdown.slice(start + 1);
  const end = section.indexOf("\n## ", 1);
  const lines = (end < 0 ? section : section.slice(0, end)).split("\n").filter((line) => line.startsWith("| "));
  // Skip the header row and its rule.
  return lines.slice(2).map((line) => {
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

test("every row of the redesign's §6 table has a home in the journeys", () => {
  const rows = firstColumn(readFileSync(REDESIGN, "utf8"), "## 6. Where everything goes");
  expect(rows.length).toBeGreaterThan(30);
  checkHomes("§6", rows, MAP_HOMES);
});

test("every shortcut of the redesign's §8 keyboard map is pressed in the journeys", () => {
  const rows = firstColumn(readFileSync(REDESIGN, "utf8"), "## 8. Keyboard");
  expect(rows.length).toBeGreaterThan(15);
  checkHomes("§8", rows, KEYBOARD_HOMES);
});
