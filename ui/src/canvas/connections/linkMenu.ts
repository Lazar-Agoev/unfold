/**
 * The menu of a link, drawn or a connection (design §4.7): Kind ▸ (any kind; a drawn link becomes a
 * connection, and `relates` only joins two tables; `other` asks for a label), Edit label, Describe… (a
 * connection's evidence in words), Copy link, and Remove. Changing the kind makes you the one who drew
 * it. Each choice that changes the page is one board edit.
 */

import { copyLink } from "../../agent";
import type { MenuEntry } from "../../components";
import type { LinkKind } from "../../ipc/generated";
import { CONNECTION_KINDS, findPage, findResource, getOpenBoard, removeLink, select, setLinkKind, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";
import { LINK_LABEL_FIELD } from "../cards/select";

/** The focus target of a connection's description field in Selected. */
export const LINK_DESCRIPTION_FIELD = "selected.linkDescription";

/** Each kind as menus name it. */
export const MENU_WORDS: Readonly<Record<LinkKind, string>> = {
  http: "HTTP",
  rpc: "RPC",
  queue: "Queue",
  event: "Event",
  writes: "Writes",
  reads: "Reads",
  calls: "Runtime call",
  relates: "Relates",
  other: "Other",
};

/** Selects a link and focuses a field of it in Selected. */
function showField(target: PageTarget, linkId: string, field: string): void {
  select(target, { kind: "link", linkId });
  showInSelected(field);
}

/** The link `linkId` of the target's page, with its page. */
function linkOf(target: PageTarget, linkId: string) {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const link = page?.links.find((item) => item.id === linkId);
  return page && link ? { page, link } : undefined;
}

/** The kinds a link can take, as menu entries (Kind ▸, Selected's Kind); `author` (null until known) signs the change. */
export function kindEntries(target: PageTarget, linkId: string, author: string | null): MenuEntry[] {
  const found = linkOf(target, linkId);
  if (!found) {
    return [];
  }
  const { page, link } = found;
  const tables = [link.from, link.to].every((id) => findResource(page, id)?.kind === "table");
  return CONNECTION_KINDS.map((kind) => ({
    id: `kind-${kind}`,
    label: MENU_WORDS[kind],
    checked: link.kind === kind,
    disabled: author === null || (kind === "relates" && !tables),
    onSelect: () => {
      if (author !== null) {
        setLinkKind(target, linkId, kind, author);
      }
      // An `other` connection says what it is in its label.
      if (kind === "other" && !link.label) {
        showField(target, linkId, LINK_LABEL_FIELD);
      }
    },
  }));
}

/** The entries of link `linkId`'s menu; `author` (null until known) signs a new kind. */
export function linkMenuEntries(target: PageTarget, linkId: string, author: string | null): MenuEntry[] {
  const link = linkOf(target, linkId)?.link;
  if (!link) {
    return [];
  }
  return [
    { id: "kind", label: "Kind", submenu: kindEntries(target, linkId, author) },
    { id: "edit", label: "Edit label", icon: "edit", onSelect: () => { showField(target, linkId, LINK_LABEL_FIELD); } },
    ...(link.kind !== undefined ? [{ id: "describe", label: "Describe…", onSelect: () => { showField(target, linkId, LINK_DESCRIPTION_FIELD); } }] : []),
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, linkId) },
    { kind: "separator", id: "s1" },
    { id: "remove", label: link.kind !== undefined ? "Remove connection" : "Remove link", danger: true, onSelect: () => { removeLink(target, linkId); } },
  ];
}
