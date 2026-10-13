/**
 * Rows Selected shows for authored claims (resource cards and connections): what an item of the page
 * is called, what a claim cites (each anchor opening its source in the peek sheet as View code
 * resolves it: a key at its block's first line, an evidence range highlighting its lines), the
 * connections in and out of an item, and a type it names.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { RESOURCE_WORDS } from "../../canvas/resources/kinds";
import { KIND_WORDS } from "../../canvas/connections/chip";
import { shortPath } from "../../code/text";
import type { Anchor, Link, Page, ProjectId } from "../../ipc/generated";
import { cardKey, findCard, findFrame, findNote, findResource, keyDisplayName, select, type PageTarget } from "../../state/boards";
import { openPeek } from "../../state/overlays";
import { anchorSource } from "../viewCode";
import { RelationRow } from "./Relations";

/** An item of the page as Selected names it: a card's name, a resource card's, a frame's title, a note's start. */
export function itemName(page: Page, id: string): string {
  const card = findCard(page, id);
  if (card) {
    return keyDisplayName(cardKey(card));
  }
  const resource = findResource(page, id);
  if (resource) {
    return `${RESOURCE_WORDS[resource.kind]} ${resource.name}`;
  }
  return findFrame(page, id)?.title ?? findNote(page, id)?.text ?? id;
}

/** Selects an item of the page by id. */
export function selectItem(target: PageTarget, page: Page, id: string): void {
  if (findCard(page, id)) {
    select(target, { kind: "card", cardId: id, key: null });
  } else if (findResource(page, id)) {
    select(target, { kind: "resource", resourceId: id });
  } else if (findFrame(page, id)) {
    select(target, { kind: "frame", frameId: id });
  } else if (page.links.some((link) => link.id === id)) {
    select(target, { kind: "link", linkId: id });
  }
}

/** One anchor: a key (its name, and where it is once its card model loads) or an evidence range, opening its source. */
function AnchorRow({ project, anchor, label }: { readonly project: ProjectId; readonly anchor: Anchor; readonly label?: string }) {
  const key = "key" in anchor ? anchor.key : null;
  const query = useQuery({ ...cardQuery(project, key ?? ""), enabled: key !== null });
  const open = anchorSource(project, anchor, query.data).target;
  const name = "key" in anchor ? keyDisplayName(anchor.key) : `${shortPath(anchor.file)}:${anchor.lines[0]}–${anchor.lines[1]}`;
  const where = "key" in anchor && open ? ` · ${shortPath(open.path)}${open.line !== undefined ? `:${open.line}` : ""}` : "";
  return (
    <RelationRow
      {...(label !== undefined ? { label } : {})}
      title={"key" in anchor ? anchor.key : anchor.file}
      {...(open
        ? {
            onActivate: () => {
              openPeek(open);
            },
          }
        : {})}
    >
      <span className="relation-code">{name}</span>
      <span className="relation-path">{where}</span>
    </RelationRow>
  );
}

/** What a claim cites, each opening its source; "nothing" when it cites nothing. */
export function AnchorRows({ target, anchors }: { readonly target: PageTarget; readonly anchors: readonly Anchor[] }) {
  if (anchors.length === 0) {
    return <RelationRow label="Cites">nothing</RelationRow>;
  }
  return (
    <>
      {anchors.map((anchor, position) => (
        <AnchorRow
          key={position}
          project={target.board.project}
          anchor={anchor}
          {...(position === 0 ? { label: "Cites" } : {})}
        />
      ))}
    </>
  );
}

/** A connection as a line in and out of an item: its kind's word, then its label. */
function lineText(link: Link): string {
  const kind = link.kind !== undefined ? KIND_WORDS[link.kind] : "link";
  return link.label ? `${kind} · ${link.label.text}` : kind;
}

/** The links and connections leaving (`→`) and entering (`←`) item `id`, each selecting the line. */
export function LineRows({ target, page, id }: { readonly target: PageTarget; readonly page: Page; readonly id: string }) {
  const lines = page.links.flatMap((link) => (link.from === id ? [{ link, out: true }] : link.to === id ? [{ link, out: false }] : []));
  return (
    <>
      {lines.map(({ link, out }, position) => (
        <RelationRow
          key={link.id}
          {...(position === 0 ? { label: "Lines" } : {})}
          onActivate={() => {
            select(target, { kind: "link", linkId: link.id });
          }}
        >
          <span className="relation-code">
            {out ? "→ " : "← "}
            {itemName(page, out ? link.to : link.from)}
          </span>
          <span className="relation-path">{` · ${lineText(link)}`}</span>
        </RelationRow>
      ))}
    </>
  );
}

/** A type a claim names (a table's type, a connection's carried type). */
export function TypeRow({ label, typeKey }: { readonly label: string; readonly typeKey: string }) {
  return (
    <RelationRow label={label} title={typeKey}>
      <span className="relation-code">{keyDisplayName(typeKey)}</span>
    </RelationRow>
  );
}
