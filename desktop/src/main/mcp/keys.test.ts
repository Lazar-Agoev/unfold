import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/** The desktop shell's sources. */
const SOURCES = join(__dirname, "../..");

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? sources(path) : /\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

/**
 * How code reads a key: naming one from its text, taking its kind from its text, or telling an
 * entrance's key by its prefix. Built from parts, so this file doesn't name them itself.
 */
const READS_KEYS = new RegExp([["key", "Name\\("], ["key", "Kind\\("], ["startsWith\\(\"", "entrance:\"\\)"]].map((parts) => parts.join("")).join("|"));

describe("keys (docs/architecture.md#ownership)", () => {
  it("are never read in the desktop shell: names and kinds come from the engine's answers", () => {
    const readers = sources(SOURCES).filter((path) => READS_KEYS.test(readFileSync(path, "utf8")));
    expect(readers).toEqual([]);
  });
});
