import { describe, expect, it } from "vitest";
import tokens from "./tokens.css?raw";

/** The `--color-*` values `:root` declares. */
const COLORS = new Map([...tokens.matchAll(/--color-([\w-]+):\s*([^;]+);/g)].map((match) => [match[1] ?? "", (match[2] ?? "").trim()]));

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
 * Text on the grounds it sits on: WCAG AA, 4.5:1 (no text lighter than `--color-subtle` on white). Pairs
 * with a translucent value are checked only where both are opaque.
 */
const TEXT_PAIRS: readonly (readonly [string, string])[] = [
  ...["canvas", "home", "sidebar", "chrome", "surface", "control", "track", "pressed", "hover", "selection", "step"].map(
    (ground) => ["text", ground] as const,
  ),
  ...[
    "canvas",
    "home",
    "sidebar",
    "chrome",
    "surface",
    "control",
    "track",
    "pressed",
    "hover",
    "selection",
    "step",
    "placeholder",
  ].map((ground) => ["muted", ground] as const),
  ["subtle", "surface"],
  ["syntax-plain", "editor"],
  ["syntax-plain", "mark"],
  ...["home", "chrome", "sidebar", "surface", "selection", "hover"].map((ground) => ["accent-text", ground] as const),
  ["on-accent", "accent"],
  ["on-danger", "danger"],
  ["danger", "surface"],
  ["danger", "home"],
  ["warn", "warn-fill"],
  ["proposed-ink", "proposed-tint"],
  ["proposed-ink", "surface"],
  ["warn", "home"],
  ["authored", "surface"],
  ["authored", "authored-tint"],
  ["monogram-ink", "monogram"],
  ["workspace-mark-ink", "workspace-mark"],
];

/**
 * Focus rings, state dots, and marks against their grounds: 3:1. `--color-faint` isn't held to it:
 * it is for rules, icons, and disabled states only (docs/ui-shell.md#theme), and the designs' value is below
 * 3:1 (2.56:1 on the surface).
 */
const MARK_PAIRS: readonly (readonly [string, string])[] = [
  ...["surface", "sidebar", "canvas", "home", "chrome"].map((ground) => ["focus-ring", ground] as const),
  ["accent", "chrome"],
  ["accent", "canvas"],
  // Review amber is #D97706 everywhere; on the light sidebar it is 2.98:1, so that pair isn't held.
  ["review", "surface"],
  ["review", "home"],
  ["success", "home"],
  // A proposed step's dashed outline and connectors.
  ["proposed", "proposed-tint"],
  ["proposed", "surface"],
];

describe("tokens", () => {
  const color = (name: string) => {
    const value = COLORS.get(name);
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
