/** What each kind of resource card is called and drawn with (design §4.5): a quiet glyph and a word. */

import type { IconName } from "../../icons";
import type { ResourceKind } from "../../ipc/generated";

/** Each kind's word, as a card's header and Selected say it. */
export const RESOURCE_WORDS: Readonly<Record<ResourceKind, string>> = {
  table: "table",
  queue: "queue",
  cache: "cache",
  storage: "storage",
  external: "external",
  actor: "actor",
};

/** Each kind's glyph. */
export const RESOURCE_ICONS: Readonly<Record<ResourceKind, IconName>> = {
  table: "resource-table",
  queue: "resource-queue",
  cache: "resource-cache",
  storage: "resource-storage",
  external: "resource-external",
  actor: "resource-actor",
};
