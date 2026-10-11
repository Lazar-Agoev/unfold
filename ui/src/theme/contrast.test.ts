import { describe, expect, it } from "vitest";
import tokens from "./tokens.css?raw";

/** The `--color-*` values declared in the first block matching `selector`. */
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
  return new Map([...block.matchAll(/--color-([\w-]+):\s*([^;]+);/g)].map((match) => [match[1] ?? "", (match[2] ?? "").trim()]));
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

const HEX = /^#[0-9a-f]{6}$/i;

/**
 * Text on the grounds it sits on: WCAG AA, 4.5:1 (no text lighter than `--color-subtle` on white). Pairs with a translucent value in a theme (dark warn and
 * authored tints) are checked only where both are opaque.
 */
const TEXT_PAIRS: readonly (readonly [string, string])[] = [
  ...["canvas", "home", "sidebar", "chrome", "tabstrip", "surface", "control", "track", "pressed", "hover", "selection", "match", "step"].map(
    (ground) => ["text", ground] as const,
  ),
  ...[
    "canvas",
    "home",
    "sidebar",
    "chrome",
    "tabstrip",
    "surface",
    "control",
    "track",
    "track-soft",
    "pressed",
    "hover",
    "selection",
    "step",
    "placeholder",
  ].map((ground) => ["muted", ground] as const),
  ["subtle", "surface"],
  ...["home", "chrome", "sidebar", "surface", "selection", "hover"].map((ground) => ["accent-text", ground] as const),
  ["on-accent", "accent"],
  ["on-danger", "danger"],
  ["danger", "surface"],
  ["danger", "home"],
  ["warn", "warn-fill"],
  ["success-text", "success-fill"],
  ["success-body", "success-fill"],
  ["warn", "home"],
  ["authored", "surface"],
  ["authored", "authored-tint"],
  ["monogram-ink", "monogram"],
  ["monogram-ink", "selection-strong"],
  ["workspace-mark-ink", "workspace-mark"],
];

/**
 * Focus rings, state dots, and marks against their grounds: 3:1. `--color-faint` isn't held to it:
 * it is for rules, icons, and disabled states only (docs/ui-shell.md#theme), and the designs' value is below
 * 3:1 (2.56:1 light, 2.74:1 dark on the surface).
 */
const MARK_PAIRS: readonly (readonly [string, string])[] = [
  ...["surface", "sidebar", "canvas", "home", "chrome"].map((ground) => ["focus-ring", ground] as const),
  ["accent", "chrome"],
  ["accent", "canvas"],
  // Review amber is #D97706 everywhere; on the light sidebar it is 2.98:1, so that pair isn't held.
  ["review", "surface"],
  ["review", "home"],
  ["success", "home"],
  // The primary button's shortcut hint ("⇧⌘B" on Compile, "⌘O" on Add project) repeats the label's key, as drawn.
  ["on-accent-soft", "accent"],
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
  const opaque = (pairs: readonly (readonly [string, string])[]) =>
    pairs.filter(([foreground, background]) => HEX.test(color(foreground)) && HEX.test(color(background)));

  it.each(opaque(TEXT_PAIRS))("%s on %s reads at 4.5:1", (foreground, background) => {
    expect(contrast(color(foreground), color(background))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(opaque(MARK_PAIRS))("%s on %s shows at 3:1", (foreground, background) => {
    expect(contrast(color(foreground), color(background))).toBeGreaterThanOrEqual(3);
  });
});

describe("dark set", () => {
  it("is the same in the OS media query and the explicit override", () => {
    const fromMedia = colors(':root:not([data-theme="light"])');
    expect(fromMedia.size).toBeGreaterThan(60);
    expect(fromMedia).toEqual(colors(':root[data-theme="dark"]'));
  });

  it("gives every light color a dark value", () => {
    const light = [...colors(":root {").keys()];
    const dark = colors(':root[data-theme="dark"]');
    expect(light.filter((name) => !dark.has(name))).toEqual([]);
  });
});
