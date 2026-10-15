/**
 * How the canvas's words answer the zoom. Everything is drawn in canvas units and the camera scales
 * it, so left alone every word shrinks as you zoom out. A few push back, as sizes on screen:
 *
 * - **Card and resource names** grow a little: 13px on screen at 100%, 15px at 50%; below 50% they
 *   stop growing and shrink with the card (`--name-grow`, a factor on the name's own size, which each
 *   card caps further so the name stays inside it).
 * - **Frames** grow with their titles. A box's title reads 13 → 14 → 16px and a zone's 20 → 26 → 34px at
 *   100, 50, and 25%, and keeps growing further out. Its drawn edge moves out so the title stays inside,
 *   and thickens: a box's 1 → 1.5 → 2px, a zone's from none to 1.5 → 2px (`frameZoom`; frames.css).
 * - **Chips and frame descriptions** fade out below 60% (`data-far`); hovering still shows them.
 * - **The writing chip** keeps its size on screen at any zoom (`--screen-scale`), and a connection's
 *   card does whenever it would be smaller (`--hover-scale`).
 *
 * The camera sets these on the canvas's root as CSS variables, so a zoom re-renders nothing.
 */

import { useStore } from "@xyflow/react";
import { useLayoutEffect } from "react";
import { framePadding } from "./frames/geometry";

/** Below this zoom, chips and frame descriptions fade out. */
const FAR_ZOOM = 0.6;
/** The zoom names stop growing at, and how much bigger they are on screen there (15px for a 13px name). */
const NAME_PEAK_ZOOM = 0.5;
const NAME_PEAK_GROWTH = 15 / 13;

/** A frame's drawn edge and title at a zoom, in canvas px. */
interface FrameZoom {
  /** The title's size, and the same as a factor on its size at 100%. */
  readonly title: number;
  readonly titleScale: number;
  /** The edge's width. */
  readonly edge: number;
  /** Where the title sits inside the edge: from its top and its left. */
  readonly titleTop: number;
  readonly titleLeft: number;
  /** How far the edge moves out past the room the frame leaves around what it holds: up, and to each side and below. */
  readonly growTop: number;
  readonly growSide: number;
}

/** What the zoom sets: canvas units (a factor, or px) for the words, the frames, the far switch, and the scales that undo the zoom. */
export interface ZoomScale {
  readonly zoom: number;
  /** A factor on a name's size: 1 at 100% and closer, 30/13 at 50% and further. */
  readonly nameGrow: number;
  readonly box: FrameZoom;
  readonly zone: FrameZoom;
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
 * A size in canvas px (a title's, an edge's): on screen it reads `base` at 100%, `half` at 50%, and
 * `quarter` at 25% (linear in the zoom between them, and on past 25%); closer than 100% it scales with the canvas.
 */
function titleSize(zoom: number, base: number, half: number, quarter: number): number {
  if (zoom >= 1) {
    return base;
  }
  const onScreen = zoom >= 0.5 ? base + ((half - base) * (1 - zoom)) / 0.5 : half + ((quarter - half) * (0.5 - zoom)) / 0.25;
  return onScreen / zoom;
}

/**
 * A look's frame on zoom. Its title's size on screen at 100, 50, and 25%, and its line as a share of it;
 * its edge's width on screen at the same zooms; then, in canvas px at 100%, with the least each keeps on
 * screen further out: the title's inset from the edge's top and left, the gap under the title, and the room
 * each side.
 */
interface FrameLook {
  readonly title: readonly [number, number, number];
  readonly line: number;
  readonly edge: readonly [number, number, number];
  readonly top: readonly [number, number];
  readonly left: readonly [number, number];
  readonly gap: readonly [number, number];
  readonly side: number;
}

const BOX: FrameLook = { title: [13, 14, 16], line: 18 / 13, edge: [1, 1.5, 2], top: [12, 6], left: [16, 8], gap: [14, 5], side: 6 };
const ZONE: FrameLook = { title: [20, 26, 34], line: 1.3, edge: [0, 1.5, 2], top: [18, 8], left: [20, 10], gap: [18, 6], side: 8 };

/**
 * A frame's edge and title at `zoom`: the title grows (`titleSize`) and stays inside, so the edge moves up
 * by what the title, its inset, and the gap under it need beyond the room the frame leaves above what it
 * holds (geometry.ts `framePadding`; a description's line is in both), and out each side and below to keep
 * a few pixels of room on screen. Closer than 100% nothing moves.
 */
function frameZoom(zoom: number, look: FrameLook, room: { readonly top: number; readonly left: number }): FrameZoom {
  const atLeast = ([canvas, onScreen]: readonly [number, number]) => Math.max(canvas, onScreen / zoom);
  const title = titleSize(zoom, ...look.title);
  const titleTop = atLeast(look.top);
  return {
    title,
    titleScale: title / look.title[0],
    edge: titleSize(zoom, ...look.edge),
    titleTop,
    titleLeft: atLeast(look.left),
    growTop: Math.max(0, titleTop + title * look.line + atLeast(look.gap) - room.top),
    growSide: Math.max(0, atLeast([room.left, look.side]) - room.left),
  };
}

/** Every size the zoom sets. */
export function zoomScale(zoom: number): ZoomScale {
  return {
    zoom,
    nameGrow: nameGrowth(zoom),
    box: frameZoom(zoom, BOX, framePadding({ look: "box" })),
    zone: frameZoom(zoom, ZONE, framePadding({ look: "zone" })),
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
  for (const [look, frame] of [["box", scale.box], ["zone", scale.zone]] as const) {
    style.setProperty(`--${look}-title-size`, `${frame.title}px`);
    style.setProperty(`--${look}-title-scale`, String(frame.titleScale));
    style.setProperty(`--${look}-edge`, `${frame.edge}px`);
    style.setProperty(`--${look}-title-top`, `${frame.titleTop}px`);
    style.setProperty(`--${look}-title-left`, `${frame.titleLeft}px`);
    style.setProperty(`--${look}-grow-top`, `${frame.growTop}px`);
    style.setProperty(`--${look}-grow-side`, `${frame.growSide}px`);
  }
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
