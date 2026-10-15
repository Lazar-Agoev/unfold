import { describe, expect, it } from "vitest";
import base from "./base.css?raw";

/** Every stylesheet of the UI, as text. */
const SHEETS = import.meta.glob<string>("../**/*.css", { query: "?raw", import: "default", eager: true });

/** The declarations of the rules whose selector list is exactly `selectors`. */
function declarations(css: string, selectors: string): string[] {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter((match) => (match[1] ?? "").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ").trim() === selectors)
    .map((match) => match[2] ?? "");
}

describe("ligatures", () => {
  it("are off on every element, so `===`, `=>` and `->` show the characters in the file", () => {
    // `!important`, because a rule's `font` shorthand resets them on its element.
    expect(declarations(base, "*, *::before, *::after").some((body) => /font-variant-ligatures:\s*none\s*!important/.test(body))).toBe(true);
  });

  it("are turned off in one place only", () => {
    const elsewhere = Object.entries(SHEETS)
      .filter(([path, css]) => path !== "./base.css" && /font-variant-ligatures|"liga"|"calt"/.test(css))
      .map(([path]) => path);
    expect(elsewhere).toEqual([]);
  });
});
