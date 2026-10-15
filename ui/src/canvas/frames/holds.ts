/**
 * What a frame holds, as its collapsed node counts it: "5 cards". Every card and resource card inside
 * it counts, nested frames' too; notes don't.
 */

import { frameItems } from "../../state/boards";
import { plural } from "../../lib/format";
import type { FramedPage } from "./geometry";

/** "5 cards", "1 card", or "nothing yet" for a frame of notes only. */
export function holdsText(page: FramedPage, frameId: string): string {
  const inside = new Set(frameItems(page, frameId));
  const count = page.cards.filter((card) => inside.has(card.id)).length + (page.resources ?? []).filter((resource) => inside.has(resource.id)).length;
  return count > 0 ? plural(count, "card") : "nothing yet";
}
