/**
 * What Enter does with Add's selection: a name's card goes onto the page (or the card already there is
 * focused); a file opens in the code panel, whole.
 */

import { viewFile } from "../code/open";
import { addRef, type PageTarget } from "../state/boards";
import type { ResultItem } from "./search";

export function addResult(item: ResultItem, page: PageTarget): void {
  if (item.kind === "file") {
    viewFile(page.board.project, item.hit.path);
  } else {
    addRef(page, item.ref, item.hit.name);
  }
}
