import { describe, expect, it } from "vitest";
import tokens from "./tokens.css?raw";

/** The `--color-*` hex values declared in the first block matching `selector`. */
function colors(selector: string): Map<string, string> {
  const start = tokens.indexOf(selector);
  const open = tokens.indexOf("{", start);
  let depth = 0;
  let end = open;
  for (; end < tokens.length; end += 1) {
    if (tokens[end] === "{") {
      depth += 1;
    } else if (tokens[end] === "}") {
      depth -= 1;
      if (depth === 0) {
        break;
      }
    }
  }
  const block = tokens.slice(open, end);
  return new Map([...block.matchAll(/--color-([\w-]+):\s*(#[0-9a-f]{6})\b/gi)].map((match) => [match[1] ?? "", match[2] ?? ""]));
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16) / 255);
  const [r = 0, g = 0, b = 0] = channels.map((value) => (value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

/** Text on its surfaces: WCAG AA, 4.5:1. */
const TEXT_PAIRS: readonly (readonly [string, string])[] = [
  ["text", "canvas"],
  ["text", "surface"],
  ["text", "sidebar"],
  ["text", "selection"],
  ["text", "popover"],
  ["muted", "popover"],
  ["muted", "canvas"],
  ["muted", "surface"],
  ["muted", "sidebar"],
  ["muted", "step"],
  ["muted", "selection"],
  ["accent-text", "surface"],
  ["accent-text", "sidebar"],
  ["accent-text", "selection"],
  ["on-accent", "accent"],
  ["on-accent", "accent-strong"],
  ["on-danger", "danger"],
  ["on-danger", "danger-strong"],
  ["authored", "surface"],
  ["authored", "authored-tint"],
  ["warn", "surface"],
  ["warn", "warn-fill"],
  ["failure", "surface"],
  ["failure", "chrome"],
  ["failure", "failure-fill"],
  ["on-tooltip", "tooltip"],
  ["muted-on-tooltip", "tooltip"],
];

/** Focus rings, icons, and marks against their surfaces: 3:1. */
const UI_PAIRS: readonly (readonly [string, string])[] = [
  ["focus-ring", "surface"],
  ["focus-ring", "sidebar"],
  ["focus-ring", "canvas"],
  ["accent", "chrome"],
  ["pending", "sidebar"],
  ["pending", "selection"],
  ["success", "chrome"],
];

const THEMES = {
  light: colors(":root {"),
  dark: new Map([...colors(":root {"), ...colors(':root[data-theme="dark"]')]),
};

describe.each(Object.entries(THEMES))("%s tokens", (_name, theme) => {
  const color = (name: string) => {
    const value = theme.get(name);
    if (value === undefined) {
      throw new Error(`no --color-${name}`);
    }
    return value;
  };

  it.each(TEXT_PAIRS)("%s on %s reads at 4.5:1", (foreground, background) => {
    expect(contrast(color(foreground), color(background))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(UI_PAIRS)("%s on %s shows at 3:1", (foreground, background) => {
    expect(contrast(color(foreground), color(background))).toBeGreaterThanOrEqual(3);
  });
});

describe("dark set", () => {
  it("is the same in the OS media query and the explicit override", () => {
    const fromMedia = colors(':root:not([data-theme="light"])');
    expect(fromMedia.size).toBeGreaterThan(30);
    expect(fromMedia).toEqual(colors(':root[data-theme="dark"]'));
  });
});
