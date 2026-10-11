/**
 * What a frame holds, in words, as its collapsed node says it (design §4.4): "2 entrances · 3
 * functions · 1 table". Every card and resource card inside it counts, nested frames' too.
 */

import type { ResourceKind } from "../../ipc/generated";
import { cardKey, frameItems } from "../../state/boards";
import { isTypeKind, keyBlockKind } from "../../code/typeKinds";
import { plural } from "../../lib/format";
import type { FramedPage } from "./geometry";

const FUNCTION_KINDS: ReadonlySet<string> = new Set(["function", "method", "constructor", "getter", "setter"]);

/** Each kind's word, in the order the text lists them. */
const WORDS = ["entrance", "function", "class", "type", "card", "table", "queue", "cache", "storage", "outside service", "actor"] as const;

type Word = (typeof WORDS)[number];

const RESOURCE_WORDS: Readonly<Record<ResourceKind, Word>> = {
  table: "table",
  queue: "queue",
  cache: "cache",
  storage: "storage",
  external: "outside service",
  actor: "actor",
};

/** The word for a card of `key`. */
function cardWord(key: string): Word {
  const kind = keyBlockKind(key);
  if (kind === undefined) {
    return key.startsWith("entrance:") ? "entrance" : "card";
  }
  if (FUNCTION_KINDS.has(kind)) {
    return "function";
  }
  if (kind === "class") {
    return "class";
  }
  return isTypeKind(kind) ? "type" : "card";
}

/** "3 functions · 1 table"; "nothing yet" for a frame of notes only. */
export function holdsText(page: FramedPage, frameId: string): string {
  const counts = new Map<Word, number>();
  const count = (word: Word) => counts.set(word, (counts.get(word) ?? 0) + 1);
  const inside = new Set(frameItems(page, frameId));
  for (const card of page.cards) {
    if (inside.has(card.id)) {
      count(cardWord(cardKey(card)));
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
