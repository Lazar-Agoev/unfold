/**
 * The page's closed labels and the cards they must not cover: each label reports its curve and the
 * widths of its forms, the canvas reports the cards' rectangles, and each label reads back where
 * `placeLabels` puts it (or that it isn't drawn at rest). One per canvas (`LabelLayoutContext`).
 * Reports in one commit are placed together: listeners hear once, after the commit's reports are in.
 */

import { createContext, useContext, useSyncExternalStore } from "react";
import { placeLabels, type LabelSpec, type Placement, type Rect } from "./placeLabels";

function sameSpec(a: LabelSpec, b: LabelSpec): boolean {
  const { from, c1, c2, to } = a.curve;
  const other = b.curve;
  return (
    a.height === b.height &&
    a.widths.length === b.widths.length &&
    a.widths.every((width, index) => width === b.widths[index]) &&
    [from, c1, c2, to].every((point, index) => {
      const match = [other.from, other.c1, other.c2, other.to][index];
      return match !== undefined && point.x === match.x && point.y === match.y;
    })
  );
}

function sameRects(a: readonly Rect[], b: readonly Rect[]): boolean {
  return a.length === b.length && a.every((rect, index) => {
    const other = b[index];
    return other !== undefined && rect.x === other.x && rect.y === other.y && rect.width === other.width && rect.height === other.height;
  });
}

export class LabelLayout {
  private readonly labels = new Map<string, LabelSpec>();
  private cards: readonly Rect[] = [];
  private placements = new Map<string, Placement | null>();
  private dirty = false;
  private scheduled = false;
  private readonly listeners = new Set<() => void>();

  /** Reports a label (or that it is gone, with null). */
  report(id: string, spec: Omit<LabelSpec, "id"> | null): void {
    const old = this.labels.get(id);
    if (spec === null) {
      if (!old) {
        return;
      }
      this.labels.delete(id);
    } else {
      const next = { id, ...spec };
      if (old && sameSpec(old, next)) {
        return;
      }
      this.labels.set(id, next);
    }
    this.changed();
  }

  /** Reports the page's cards, as drawn. */
  setCards(cards: readonly Rect[]): void {
    if (sameRects(this.cards, cards)) {
      return;
    }
    this.cards = cards;
    this.changed();
  }

  /** Where a label is drawn: its placement, null when it isn't drawn at rest, undefined until placed. */
  placement(id: string): Placement | null | undefined {
    if (this.dirty) {
      this.dirty = false;
      const next = placeLabels([...this.labels.values()], this.cards);
      // Unchanged placements keep their identity, so their labels don't render again.
      for (const [key, value] of next) {
        const old = this.placements.get(key);
        if (old && value && old.x === value.x && old.y === value.y && old.form === value.form) {
          next.set(key, old);
        }
      }
      this.placements = next;
    }
    return this.placements.get(id);
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private changed(): void {
    this.dirty = true;
    if (!this.scheduled) {
      this.scheduled = true;
      queueMicrotask(() => {
        this.scheduled = false;
        for (const listener of this.listeners) {
          listener();
        }
      });
    }
  }
}

export const LabelLayoutContext = createContext<LabelLayout | null>(null);

const NO_LAYOUT = new LabelLayout();

/** The layout of the canvas around (a stand-alone one outside a canvas). */
export function useLabelLayout(): LabelLayout {
  return useContext(LabelLayoutContext) ?? NO_LAYOUT;
}

/** A label's placement, kept current. */
export function useLabelPlacement(layout: LabelLayout, id: string): Placement | null | undefined {
  return useSyncExternalStore(layout.subscribe, () => layout.placement(id));
}
