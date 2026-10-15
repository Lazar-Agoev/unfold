/**
 * Where expanded frames' titles go as you zoom out. A title grows on screen from its corner (zoom.ts:
 * a box's 13 → 14 → 16px, a zone's 20 → 26 → 34px at 100, 50, and 25%): its line keeps its bottom-left
 * corner where it is at 100%, so it grows up and to the right, over the frame's edge, never over what
 * the frame holds. A box title fades out before it would run into the title of a frame around it: this
 * finds the zoom it is gone by. Pure.
 */

import type { Frame } from "../../ipc/generated";
import type { Measure } from "../../flow/layout";
import type { Rect } from "../layout";
import { zoomScale } from "../zoom";

/** A title's corner in its frame (frames.css): its left inset and where its line's bottom sits, and its line as a share of its size. */
const TITLE_PLACE = {
  box: { left: 16, bottom: 12 + 18, line: 18 / 13 },
  zone: { left: 20, bottom: 18 + 26, line: 26 / 20 },
} as const;

/** The titles' fonts at their 100% sizes (frames.css `.frame-title-text`), and a zone title's tracking. */
const BOX_TITLE_FONT = '600 13px "Inter Variable", system-ui, sans-serif';
const ZONE_TITLE_FONT = '700 20px "Inter Variable", system-ui, sans-serif';
const ZONE_TRACKING = -0.01;

/** The zooms tried, closest first, and the least one (the canvas's). */
const STEP = 0.01;
const LEAST_ZOOM = 0.05;

/** An expanded frame as drawn: its frame and its box. */
export interface DrawnBox {
  readonly frame: Frame;
  readonly rect: Rect;
}

/** A title's rectangle at a zoom, in canvas units. */
function titleRect(box: DrawnBox, zoom: number, measure: Measure): Rect {
  const scale = zoomScale(zoom);
  const zone = box.frame.look === "zone";
  const place = TITLE_PLACE[box.frame.look];
  const size = zone ? scale.zoneTitle : scale.boxTitle;
  const base = zone
    ? (measure(box.frame.title, ZONE_TITLE_FONT) + box.frame.title.length * ZONE_TRACKING * 20) / 20
    : measure(box.frame.title, BOX_TITLE_FONT) / 13;
  const height = size * place.line;
  return { x: box.rect.x + place.left, y: box.rect.y + place.bottom - height, width: base * size, height };
}

function overlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * For each box drawn inside another expanded frame, the zoom its title is gone by: the closest zoom at
 * which it would run into an enclosing frame's title. Boxes whose titles never meet one are left out.
 */
export function titleFades(boxes: readonly DrawnBox[], measure: Measure): Map<string, number> {
  const byId = new Map(boxes.map((box) => [box.frame.id, box]));
  const parent = new Map<string, DrawnBox>();
  for (const box of boxes) {
    for (const member of box.frame.members) {
      if (byId.has(member)) {
        parent.set(member, box);
      }
    }
  }
  const fades = new Map<string, number>();
  for (const box of boxes) {
    if (box.frame.look !== "box") {
      continue;
    }
    const around: DrawnBox[] = [];
    for (let up = parent.get(box.frame.id); up; up = parent.get(up.frame.id)) {
      around.push(up);
    }
    if (around.length === 0) {
      continue;
    }
    for (let zoom = 1; zoom >= LEAST_ZOOM; zoom = Math.round((zoom - STEP) * 100) / 100) {
      const own = titleRect(box, zoom, measure);
      if (around.some((outer) => overlap(own, titleRect(outer, zoom, measure)))) {
        fades.set(box.frame.id, zoom);
        break;
      }
    }
  }
  return fades;
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
