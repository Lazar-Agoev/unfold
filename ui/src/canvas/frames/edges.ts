/**
 * Where expanded frames' edges go as you zoom out (zoom.ts `frameZoom`): each frame's edge moves out past
 * its box so its growing title stays inside, and a frame whose top holds another frame moves out further,
 * past that frame's edge, so no title runs into another. Frames know how deep that nesting goes; the zoom
 * does the rest in CSS (frames.css). Boxes inside a zone get a faint fill. Pure.
 */

import type { Measure } from "../../flow/layout";
import type { Frame } from "../../ipc/generated";
import type { Rect } from "../layout";
import { framePadding } from "./geometry";

/** An expanded frame as drawn: its frame and its box. */
export interface DrawnBox {
  readonly frame: Frame;
  readonly rect: Rect;
}

/**
 * How deep the frames inside a frame nest: along its top (`top`, the frames that sit where what it holds
 * starts, and theirs), and anywhere (`any`). Its edge moves out by each of their growth too.
 */
export interface Nesting {
  readonly top: number;
  readonly any: number;
}

/** A frame with no frames inside it. */
export const FLAT: Nesting = { top: 0, any: 0 };

/** Each expanded frame's nesting; frames with nothing inside them are left out. */
export function frameNesting(boxes: readonly DrawnBox[]): Map<string, Nesting> {
  const byId = new Map(boxes.map((box) => [box.frame.id, box]));
  const nesting = new Map<string, Nesting>();
  const of = (box: DrawnBox): Nesting => {
    const known = nesting.get(box.frame.id);
    if (known) {
      return known;
    }
    const start = box.rect.y + framePadding(box.frame).top;
    const inner = box.frame.members.flatMap((member) => {
      const child = byId.get(member);
      return child ? [{ child, nested: of(child) }] : [];
    });
    const found: Nesting = {
      top: Math.max(0, ...inner.filter(({ child }) => Math.abs(child.rect.y - start) < 1).map(({ nested }) => nested.top + 1)),
      any: Math.max(0, ...inner.map(({ nested }) => nested.any + 1)),
    };
    nesting.set(box.frame.id, found);
    return found;
  };
  for (const box of boxes) {
    of(box);
  }
  return new Map([...nesting].filter(([, found]) => found.any > 0));
}

/** A frame title's fonts at 100% (frames.css `.frame-title-text`), and a zone title's tracking. */
const BOX_TITLE_FONT = '600 13px "Inter Variable", system-ui, sans-serif';
const ZONE_TITLE_FONT = '700 20px "Inter Variable", system-ui, sans-serif';
const ZONE_TRACKING = -0.01 * 20;

/** A frame's title's width at 100%, in canvas px. */
export function titleWidth(frame: Pick<Frame, "look" | "title">, measure: Measure): number {
  return frame.look === "zone" ? measure(frame.title, ZONE_TITLE_FONT) + ZONE_TRACKING * frame.title.length : measure(frame.title, BOX_TITLE_FONT);
}

/** How far a frame's drawn edge moves out past its box on each side, as CSS lengths the zoom's variables fill in. */
export interface EdgeInsets {
  readonly top: string;
  readonly right: string;
  readonly bottom: string;
  readonly left: string;
}

/**
 * Where an expanded frame draws its edge at the zoom (zoom.ts `frameZoom`): up by what its title needs,
 * and its nested frames' growth along its top (`nesting`); out each side and below by its own room and its
 * nested frames'; and to the right, further when its growing title (`title` wide at 100%) would otherwise
 * run past a frame `width` wide. As `inset` values (negative: outward).
 */
export function edgeInsets(look: Frame["look"], nesting: Nesting, title: number, width: number): EdgeInsets {
  const own = look === "zone" ? "zone" : "box";
  const top = `(var(--${own}-grow-top, 0px) + var(--box-grow-top, 0px) * ${nesting.top})`;
  const side = `(var(--${own}-grow-side, 0px) + var(--box-grow-side, 0px) * ${nesting.any})`;
  const fit = `(${title}px * var(--${own}-title-scale, 1) + 2 * var(--${own}-title-left, 0px) - ${side} - ${width}px)`;
  return { top: `calc(-1 * ${top})`, right: `calc(-1 * max(${side}, ${fit}))`, bottom: `calc(-1 * ${side})`, left: `calc(-1 * ${side})` };
}

/** The frames inside a zone, at any depth: a box there gets a faint white fill. */
export function insideZones(frames: readonly Frame[]): Set<string> {
  const inside = new Set<string>();
  const visit = (frame: Frame, zoned: boolean) => {
    for (const member of frame.members) {
      const child = frames.find((item) => item.id === member);
      if (child) {
        if (zoned) {
          inside.add(child.id);
        }
        visit(child, zoned || frame.look === "zone");
      }
    }
  };
  for (const frame of frames) {
    if (frame.look === "zone") {
      visit(frame, true);
    }
  }
  return inside;
}
