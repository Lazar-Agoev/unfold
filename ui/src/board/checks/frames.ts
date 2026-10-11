/**
 * A page's frames as a tree: which frame holds each item, everything a frame holds at any depth, what
 * a collapsed frame hides, and where a frame sits on the canvas. Pure; the document's rules (an item in
 * at most one frame, no cycles) are trusted, but a cycle never loops.
 */

import type { Frame, Page } from "../../ipc/generated";
import { boundsOf, type Box } from "./geometry";
import { THRESHOLDS } from "./thresholds";

export interface FrameTree {
  /** The page's frames by id. */
  readonly frames: ReadonlyMap<string, Frame>;
  /** The frame that holds `id` directly, if any. */
  parentOf(id: string): string | undefined;
  /** Every item frame `id` holds, nested frames' items included. */
  inside(id: string): ReadonlySet<string>;
  /** The outermost collapsed frame that hides `id` (holding it at any depth), if any. */
  hiddenBy(id: string): string | undefined;
}

export function frameTree(page: Page): FrameTree {
  const frames = new Map((page.frames ?? []).map((frame) => [frame.id, frame]));
  const parents = new Map<string, string>();
  for (const frame of frames.values()) {
    for (const member of frame.members) {
      parents.set(member, frame.id);
    }
  }
  const held = new Map<string, Set<string>>();
  const inside = (id: string): ReadonlySet<string> => {
    const known = held.get(id);
    if (known) {
      return known;
    }
    const items = new Set<string>();
    held.set(id, items);
    const pending = [...(frames.get(id)?.members ?? [])];
    for (let member = pending.pop(); member !== undefined; member = pending.pop()) {
      if (member !== id && !items.has(member)) {
        items.add(member);
        pending.push(...(frames.get(member)?.members ?? []));
      }
    }
    return items;
  };
  const ancestors = (id: string): string[] => {
    const chain: string[] = [];
    for (let parent = parents.get(id); parent !== undefined && !chain.includes(parent) && parent !== id; parent = parents.get(parent)) {
      chain.push(parent);
    }
    return chain;
  };
  return {
    frames,
    parentOf: (id) => parents.get(id),
    inside,
    hiddenBy: (id) => ancestors(id).filter((frame) => frames.get(frame)?.collapsed === true).at(-1),
  };
}

/**
 * Where frame `id` sits: its box as drawn, else the box around its members as drawn (nested frames
 * the same way) with the frame's padding; undefined when nothing of it is drawn.
 */
export function frameBox(tree: FrameTree, boxes: ReadonlyMap<string, Box>, id: string, seen: ReadonlySet<string> = new Set()): Box | undefined {
  const drawn = boxes.get(id);
  if (drawn || seen.has(id)) {
    return drawn;
  }
  const within = new Set([...seen, id]);
  const members = (tree.frames.get(id)?.members ?? []).flatMap((member) => {
    const box = tree.frames.has(member) ? frameBox(tree, boxes, member, within) : boxes.get(member);
    return box ? [box] : [];
  });
  return boundsOf(members, THRESHOLDS.framePadding);
}
