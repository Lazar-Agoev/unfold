/**
 * Text measurement with the bundled fonts (docs/ui-shell.md#theme-and-components: the UI and code fonts ship with the
 * app, so text measures the same everywhere). Card widths and drawn-flow geometry are computed from
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

/** The card fonts (cards.css): a field's or a code row's text, the header's name, and its location. */
export const CODE_FONT = '12px "JetBrains Mono Variable", ui-monospace, monospace';
export const TITLE_FONT = '600 13px "JetBrains Mono Variable", ui-monospace, monospace';
export const LOCATION_FONT = '11px "JetBrains Mono Variable", ui-monospace, monospace';
/** A title the page gave a definition, in words (cards.css `.card-title.authored`), and the code name after it (`.card-code-name`). */
export const AUTHORED_TITLE_FONT = '600 13px "Inter Variable", system-ui, sans-serif';
export const CODE_NAME_FONT = '12px "JetBrains Mono Variable", ui-monospace, monospace';
/** A header's tag after the name (`ASYNC`, cards.css `.card-tag`), in small caps. */
export const TAG_FONT = '700 9px "Inter Variable", system-ui, sans-serif';
/** What an expanded card says from its code (cards.css): the signature, the doc, and the imports and what runs when it is defined. */
export const SIGNATURE_FONT = '11.5px "JetBrains Mono Variable", ui-monospace, monospace';
export const DOC_FONT = '12.5px "Inter Variable", system-ui, sans-serif';
export const LINKS_FONT = '11px "JetBrains Mono Variable", ui-monospace, monospace';
/** An app's or entrance's card (cards.css): the app in small caps, its name, and what starts it. */
export const START_APP_FONT = '600 10px "Inter Variable", system-ui, sans-serif';
export const START_NAME_FONT = '600 14px "Inter Variable", system-ui, sans-serif';
export const START_ON_FONT = '12px "JetBrains Mono Variable", ui-monospace, monospace';
/** A type card's kind before its name (`interface`, cards.css `.tcard-kind`). */
export const TYPE_KIND_FONT = '13px "JetBrains Mono Variable", ui-monospace, monospace';
/** A description's text (cards.css). */
export const DESCRIPTION_FONT = '12px "Inter Variable", system-ui, sans-serif';
/** A collapsed frame's title, and the agent mark's name beside it (frames.css). */
export const FRAME_TITLE_FONT = '600 13px "Inter Variable", system-ui, sans-serif';
export const MARK_FONT = '11px "Inter Variable", system-ui, sans-serif';
/** A connection's chip (connections.css): its kind's word, its words, and the code it names. */
export const CHIP_WORD_FONT = '600 11px "Inter Variable", system-ui, sans-serif';
export const CHIP_TEXT_FONT = '11px "Inter Variable", system-ui, sans-serif';
export const CHIP_CODE_FONT = '11px "JetBrains Mono Variable", ui-monospace, monospace';
