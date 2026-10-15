/**
 * How the canvas's words answer the zoom. Everything is drawn in canvas units and the camera scales
 * it, so left alone every word shrinks as you zoom out. A few push back, as sizes on screen:
 *
 * - **Card and resource names** grow a little: 13px on screen at 100%, 15px at 50%; below 50% they
 *   stop growing and shrink with the card (`--name-grow`, a factor on the name's own size, which each
 *   card caps further so the name stays inside it).
 * - **Box titles** read 13 → 14 → 16px and **zone titles** 20 → 26 → 34px at 100, 50, and 25%, and
 *   keep growing further out (`--box-title-size`, `--zone-title-size`).
 * - **Chips and frame descriptions** fade out below 60% (`data-far`); hovering still shows them.
 * - **The writing chip** keeps its size on screen at any zoom (`--screen-scale`), and a connection's
 *   card does whenever it would be smaller (`--hover-scale`).
 *
 * The camera sets these on the canvas's root as CSS variables, so a zoom re-renders nothing.
 */

import { useStore } from "@xyflow/react";
import { useLayoutEffect } from "react";

/** Below this zoom, chips and frame descriptions fade out. */
const FAR_ZOOM = 0.6;
/** The zoom names stop growing at, and how much bigger they are on screen there (15px for a 13px name). */
const NAME_PEAK_ZOOM = 0.5;
const NAME_PEAK_GROWTH = 15 / 13;

/** What the zoom sets: canvas units (a factor, or px) for the words, the far switch, and the scales that undo the zoom. */
export interface ZoomScale {
  readonly zoom: number;
  /** A factor on a name's size: 1 at 100% and closer, 30/13 at 50% and further. */
  readonly nameGrow: number;
  /** A box's and a zone's title, in canvas px. */
  readonly boxTitle: number;
  readonly zoneTitle: number;
  readonly far: boolean;
  /** 1 / zoom: something at this scale keeps its size on screen. */
  readonly screen: number;
  /** max(1, 1 / zoom): it keeps its size on screen when it would be smaller. */
  readonly hover: number;
}

/** A name's growth: its on-screen size goes linearly from 1× at 100% to 15/13× at 50%, then it shrinks with the card. */
function nameGrowth(zoom: number): number {
  const z = Math.max(NAME_PEAK_ZOOM, Math.min(1, zoom));
  const onScreen = 1 + ((NAME_PEAK_GROWTH - 1) * (1 - z)) / (1 - NAME_PEAK_ZOOM);
  return onScreen / z;
}

/**
 * A title's size in canvas px: on screen it reads `base` at 100%, `half` at 50%, and `quarter` at 25%
 * (linear in the zoom between them, and on past 25%), and closer than 100% it scales with the canvas.
 */
function titleSize(zoom: number, base: number, half: number, quarter: number): number {
  if (zoom >= 1) {
    return base;
  }
  const near = base + ((half - base) * (1 - zoom)) / 0.5;
  const far = half + ((quarter - half) * (0.5 - zoom)) / 0.25;
  return Math.max(near, far) / zoom;
}

/** Every size the zoom sets. */
export function zoomScale(zoom: number): ZoomScale {
  return {
    zoom,
    nameGrow: nameGrowth(zoom),
    boxTitle: titleSize(zoom, 13, 14, 16),
    zoneTitle: titleSize(zoom, 20, 26, 34),
    far: zoom < FAR_ZOOM,
    screen: 1 / zoom,
    hover: Math.max(1, 1 / zoom),
  };
}

/** Writes a zoom's sizes on the canvas's root. */
function applyZoomScale(root: HTMLElement, zoom: number): void {
  const scale = zoomScale(zoom);
  const { style } = root;
  style.setProperty("--zoom", String(scale.zoom));
  style.setProperty("--name-grow", String(scale.nameGrow));
  style.setProperty("--box-title-size", `${scale.boxTitle}px`);
  style.setProperty("--zone-title-size", `${scale.zoneTitle}px`);
  style.setProperty("--screen-scale", String(scale.screen));
  style.setProperty("--hover-scale", String(scale.hover));
  if (scale.far) {
    root.dataset["far"] = "";
  } else {
    delete root.dataset["far"];
  }
}

/** Follows the camera's zoom onto the canvas's root (rendered inside React Flow; it draws nothing). */
export function ZoomScaleVariables() {
  const zoom = useStore((state) => state.transform[2]);
  const root = useStore((state) => state.domNode);
  useLayoutEffect(() => {
    if (root) {
      applyZoomScale(root, zoom);
    }
  }, [root, zoom]);
  return null;
}
