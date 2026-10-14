/**
 * The menu of a connection (design §4.7): Kind ▸ (the seven, as `kindEntries` offers them; `relates`
 * only joins two tables), Edit label, Describe… (its evidence in words), Copy link, and Remove
 * connection. Changing the kind makes you the one who drew it. Each choice that changes the page is
 * one undo step.
 */

import { copyLink } from "../../agent";
import { showToast, type MenuEntry } from "../../components";
import { CONNECTION_KINDS, CONNECTION_WORDS, findPage, findResource, getOpenBoard, removeLink, select, setLinkKind, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";

/** The focus targets of a connection's label and description fields in Selected. */
export const LINK_LABEL_FIELD = "selected.linkLabel";
export const LINK_DESCRIPTION_FIELD = "selected.linkDescription";

/** Selects a connection and focuses a field of it in Selected. */
function showField(target: PageTarget, linkId: string, field: string): void {
  select(target, { kind: "link", linkId });
  showInSelected(field);
}

/** The connection `linkId` of the target's page, with its page. */
function linkOf(target: PageTarget, linkId: string) {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const link = page?.links.find((item) => item.id === linkId);
  return page && link ? { page, link } : undefined;
}

/**
 * The kinds a connection can take, as menu entries (Kind ▸, Selected's Kind); `author` (null until known)
 * signs the change. `calls` is offered only on a connection that leaves from a call (its step), and
 * `other` only once it has a label, since it says what it is there. A change is saved at once as one
 * undo step; what the board refuses comes back as a message and leaves the connection as it was.
 */
export function kindEntries(target: PageTarget, linkId: string, author: string | null): MenuEntry[] {
  const found = linkOf(target, linkId);
  if (!found) {
    return [];
  }
  const { page, link } = found;
  const tables = [link.from, link.to].every((id) => findResource(page, id)?.kind === "table");
  return CONNECTION_KINDS.filter((kind) => kind !== "calls" || link.step !== undefined).map((kind) => ({
    id: `kind-${kind}`,
    label: CONNECTION_WORDS[kind],
    checked: link.kind === kind,
    disabled: author === null || (kind === "relates" && !tables) || (kind === "other" && !link.label),
    onSelect: () => {
      if (author === null) {
        return;
      }
      void setLinkKind(target, linkId, kind, author).then((outcome) => {
        if (!outcome.ok) {
          showToast({ title: "Couldn't change the kind", body: outcome.message, tone: "danger" });
        }
      });
    },
  }));
}

/** The entries of connection `linkId`'s menu; `author` (null until known) signs a new kind. */
export function linkMenuEntries(target: PageTarget, linkId: string, author: string | null): MenuEntry[] {
  const link = linkOf(target, linkId)?.link;
  if (!link) {
    return [];
  }
  return [
    { id: "kind", label: "Kind", submenu: kindEntries(target, linkId, author) },
    { id: "edit", label: "Edit label", icon: "edit", onSelect: () => { showField(target, linkId, LINK_LABEL_FIELD); } },
    { id: "describe", label: "Describe…", onSelect: () => { showField(target, linkId, LINK_DESCRIPTION_FIELD); } },
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, linkId) },
    { kind: "separator", id: "s1" },
    { id: "remove", label: "Remove connection", danger: true, onSelect: () => { removeLink(target, linkId); } },
  ];
}
