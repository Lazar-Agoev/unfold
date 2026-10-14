/** What Enter does with Add's selection: its card goes onto the page (or the card already there is focused). */

import { addRef, type PageTarget } from "../state/boards";
import type { ResultItem } from "./search";

export function addResult(item: ResultItem, page: PageTarget): void {
  addRef(page, item.ref, item.hit.name);
}
