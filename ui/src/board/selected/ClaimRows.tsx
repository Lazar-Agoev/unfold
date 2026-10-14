/**
 * Rows Selected shows for authored claims (resource cards and connections): what an item of the page
 * is called, what a claim cites (a block by its card's name and where it is, or an evidence range by
 * its lines), the connections in and out of an item, and a type it names.
 */

import { useQuery } from "@tanstack/react-query";
import { RESOURCE_WORDS } from "../../canvas/resources/kinds";
import { shortPath } from "../../code/text";
import { useEngineQuery } from "../../ipc";
import type { Anchor, Connection, Page, ProjectId } from "../../ipc/generated";
import { CONNECTION_WORDS, findCard, findFrame, findNote, findResource, select, type PageTarget } from "../../state/boards";
import { cardName, cardQuery } from "../../state/cards";
import { RelationRow } from "./Relations";

/**
 * An item of the page as Selected names it: a card's name (`cardNames`, from what is drawn), a resource
 * card's, a frame's title, a note's start.
 */
export function itemName(page: Page, id: string, cardNames: (cardId: string) => string): string {
  if (findCard(page, id)) {
    return cardNames(id);
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

/** One anchor: a block (its definition's name and where it is, once its card loads) or an evidence range. */
function AnchorRow({ project, anchor, label }: { readonly project: ProjectId; readonly anchor: Anchor; readonly label?: string }) {
  const key = "key" in anchor ? anchor.key : "";
  const query = useQuery({ ...cardQuery({ project, key }), enabled: "key" in anchor });
  const view = query.data;
  const flow = view?.type === "definition" ? view.flow : undefined;
  const name = "key" in anchor ? (view ? cardName(view) : anchor.key) : `${shortPath(anchor.file)}:${anchor.lines[0]}–${anchor.lines[1]}`;
  const where = flow ? ` · ${shortPath(flow.header.path)}:${flow.header.line}` : "";
  return (
    <RelationRow {...(label !== undefined ? { label } : {})} title={"key" in anchor ? anchor.key : anchor.file}>
      <span className="relation-code">{name}</span>
      <span className="relation-path">{where}</span>
    </RelationRow>
  );
}

/** What a claim cites; "nothing" when it cites nothing. */
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
function lineText(link: Connection): string {
  const kind = CONNECTION_WORDS[link.kind];
  return link.label ? `${kind} · ${link.label.text}` : kind;
}

/** The connections leaving (`→`) and entering (`←`) item `id`, each selecting the connection. */
export function LineRows({ target, page, id, cardNames }: { readonly target: PageTarget; readonly page: Page; readonly id: string; readonly cardNames: (cardId: string) => string }) {
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
            {itemName(page, out ? link.to : link.from, cardNames)}
          </span>
          <span className="relation-path">{` · ${lineText(link)}`}</span>
        </RelationRow>
      ))}
    </>
  );
}

/** A type a claim names (a table's type, a connection's carried type), by its name once it loads. */
export function TypeRow({ project, label, typeKey }: { readonly project: ProjectId; readonly label: string; readonly typeKey: string }) {
  const definition = useEngineQuery("type_definition", { project, key: typeKey });
  return (
    <RelationRow label={label} title={typeKey}>
      <span className="relation-code">{definition.data?.name ?? typeKey}</span>
    </RelationRow>
  );
}
