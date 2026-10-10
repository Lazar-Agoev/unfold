/**
 * Text measurement with the bundled fonts (architecture §11.8: the UI and code fonts ship with the
 * app, so text measures the same everywhere). Card widths and Flow geometry are computed from
 * these widths before anything renders. Measurements wait for the fonts: until they load, widths
 * come from a character-width estimate and the epoch changes once they are ready.
 */

import { useSyncExternalStore } from "react";
import type { Measure } from "../flow/layout";

let context: CanvasRenderingContext2D | null | undefined;
const cache = new Map<string, number>();
let epoch = 0;
const listeners = new Set<() => void>();
let watching = false;

function sizeOf(font: string): number {
  const match = /(\d+(?:\.\d+)?)px/.exec(font);
  return match ? Number(match[1]) : 13;
}

/** An estimate for environments without a canvas (tests) or before the fonts load. */
export const estimateText: Measure = (text, font) => {
  const size = sizeOf(font);
  const perChar = font.includes("Mono") || font.includes("monospace") ? 0.6 : 0.56;
  return text.length * size * perChar;
};

function canvasContext(): CanvasRenderingContext2D | null {
  if (context !== undefined) {
    return context;
  }
  // jsdom has no canvas; measuring there would only log "not implemented".
  if (typeof document === "undefined" || navigator.userAgent.includes("jsdom")) {
    context = null;
    return context;
  }
  context = document.createElement("canvas").getContext("2d");
  return context;
}

function fontsReady(): boolean {
  return typeof document !== "undefined" && "fonts" in document && document.fonts.status === "loaded";
}

function watchFonts(): void {
  if (watching || typeof document === "undefined" || !("fonts" in document)) {
    return;
  }
  watching = true;
  const bump = () => {
    cache.clear();
    epoch += 1;
    for (const listener of listeners) {
      listener();
    }
  };
  void document.fonts.ready.then(bump);
  document.fonts.addEventListener("loadingdone", bump);
}

/** Measures with the bundled fonts once they are loaded, estimating until then. */
export const measureText: Measure = (text, font) => {
  watchFonts();
  const canvas = canvasContext();
  if (!canvas || !fontsReady()) {
    return estimateText(text, font);
  }
  const key = `${font}\u0000${text}`;
  const cached = cache.get(key);
  if (cached !== undefined) {
    return cached;
  }
  canvas.font = font;
  const width = canvas.measureText(text).width;
  cache.set(key, width);
  return width;
};

const measures = new Map<number, Measure>();

/**
 * The measurer for the current font epoch: a new function once the bundled fonts load, so layouts
 * memoized on it recompute with the real widths.
 */
export function useMeasure(): Measure {
  const current = useFontEpoch();
  let measure = measures.get(current);
  if (!measure) {
    measure = (text, font) => measureText(text, font);
    measures.clear();
    measures.set(current, measure);
  }
  return measure;
}

/** Changes when the bundled fonts finish loading, so measured layouts recompute. */
function useFontEpoch(): number {
  return useSyncExternalStore(
    (listener) => {
      watchFonts();
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => epoch,
  );
}

/** The card fonts (cards.css, code.css): code rows, the header's name, location, and an entrance's method. */
export const CODE_FONT = '12px "JetBrains Mono Variable", ui-monospace, monospace';
export const TITLE_FONT = '600 13px "JetBrains Mono Variable", ui-monospace, monospace';
export const LOCATION_FONT = '11px "JetBrains Mono Variable", ui-monospace, monospace';
export const METHOD_FONT = '500 11px "JetBrains Mono Variable", ui-monospace, monospace';
