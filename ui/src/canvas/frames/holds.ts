/**
 * What a frame holds, in words, as its collapsed node says it: "2 entrances · 3
 * functions · 1 table". Every card and resource card inside it counts, nested frames' too; a card's
 * word comes from what the reader drew for it ("card" while it isn't loaded).
 */

import type { CardView, ResourceKind } from "../../ipc/generated";
import { frameItems } from "../../state/boards";
import { plural } from "../../lib/format";
import type { FramedPage } from "./geometry";

/** Each kind's word, in the order the text lists them. */
const WORDS = ["app", "entrance", "module", "function", "class", "type", "card", "table", "queue", "cache", "storage", "outside service", "actor"] as const;

type Word = (typeof WORDS)[number];

const RESOURCE_WORDS: Readonly<Record<ResourceKind, Word>> = {
  table: "table",
  queue: "queue",
  cache: "cache",
  storage: "storage",
  external: "outside service",
  actor: "actor",
};

/** The word for a card that drew `view`. */
function wordOf(view: CardView | undefined): Word {
  if (view?.type === "start") {
    return view.kind;
  }
  switch (view?.flow.header.kind) {
    case "module":
    case "function":
    case "class":
    case "type":
      return view.flow.header.kind;
    default:
      return "card";
  }
}

/** "3 functions · 1 table"; "nothing yet" for a frame of notes only. `views` is what each loaded card drew, by card id. */
export function holdsText(page: FramedPage, frameId: string, views: ReadonlyMap<string, CardView>): string {
  const counts = new Map<Word, number>();
  const count = (word: Word) => counts.set(word, (counts.get(word) ?? 0) + 1);
  const inside = new Set(frameItems(page, frameId));
  for (const card of page.cards) {
    if (inside.has(card.id)) {
      count(wordOf(views.get(card.id)));
    }
  }
  for (const resource of page.resources ?? []) {
    if (inside.has(resource.id)) {
      count(RESOURCE_WORDS[resource.kind]);
    }
  }
  const parts = WORDS.flatMap((word) => {
    const n = counts.get(word) ?? 0;
    return n > 0 ? [plural(n, word, word === "class" ? "classes" : undefined)] : [];
  });
  return parts.length > 0 ? parts.join(" · ") : "nothing yet";
}
