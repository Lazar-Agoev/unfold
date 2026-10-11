/**
 * Rows Selected shows for authored claims (resource cards and connections): what an item of the page
 * is called, what a claim cites (each anchor opening its source in the peek sheet: a key at its block,
 * an evidence range at its lines), the connections in and out of an item, and a type that opens its
 * definition in place.
 */

import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { cardQuery } from "../../canvas";
import { RESOURCE_WORDS } from "../../canvas/resources/kinds";
import { KIND_WORDS } from "../../canvas/connections/chip";
import { TypeDefinition } from "../../code/TypeDefinition";
import { shortPath } from "../../code/text";
import type { Anchor, Link, Page, ProjectId } from "../../ipc/generated";
import { cardKey, findCard, findFrame, findNote, findResource, keyDisplayName, select, toggledOpenTypes, type PageTarget } from "../../state/boards";
import { openPeek } from "../../state/overlays";
import { resolveRefProject, useProjectIndex } from "../../state/workspace";
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
function AnchorRow({ project, anchor, label }: { readonly project: ProjectId | null; readonly anchor: Anchor; readonly label?: string }) {
  const key = "key" in anchor ? anchor.key : null;
  const query = useQuery({ ...cardQuery(project ?? "", key ?? ""), enabled: project !== null && key !== null });
  const model = query.data;
  let place: { readonly path: string; readonly line?: number } | null = null;
  if ("file" in anchor) {
    place = { path: anchor.file, line: anchor.lines[0] };
  } else if (model?.type === "block") {
    place = { path: model.header.path, line: model.header.startLine };
  } else if (model?.type === "entrance") {
    place = { path: model.evidence.path, ...(model.evidence.startLine !== null ? { line: model.evidence.startLine } : {}) };
  }
  const name = "key" in anchor ? keyDisplayName(anchor.key) : `${shortPath(anchor.file)}:${anchor.lines[0]}–${anchor.lines[1]}`;
  const where = "key" in anchor && place ? ` · ${shortPath(place.path)}${place.line !== undefined ? `:${place.line}` : ""}` : "";
  const open = project !== null && place !== null ? place : null;
  return (
    <RelationRow
      {...(label !== undefined ? { label } : {})}
      title={"key" in anchor ? anchor.key : anchor.file}
      {...(open && project !== null
        ? {
            onActivate: () => {
              openPeek({ project, path: open.path, ...(open.line !== undefined ? { line: open.line } : {}) });
            },
          }
        : {})}
    >
      <span className="relation-code">{name}</span>
      <span className="relation-path">{project === null ? ` · in ${anchor.project ?? "another project"}, not connected` : where}</span>
    </RelationRow>
  );
}

/** What a claim cites, each opening its source; "nothing" when it cites nothing. */
export function AnchorRows({ target, anchors }: { readonly target: PageTarget; readonly anchors: readonly Anchor[] }) {
  const index = useProjectIndex();
  if (anchors.length === 0) {
    return <RelationRow label="Cites">nothing</RelationRow>;
  }
  return (
    <>
      {anchors.map((anchor, position) => (
        <AnchorRow
          key={position}
          project={resolveRefProject(index, target.board.project, anchor.project)}
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

/** A type a claim names (a table's type, a connection's carried type): its name, opening its definition in place. */
export function TypeRow({ label, project, typeKey }: { readonly label: string; readonly project: ProjectId | null; readonly typeKey: string }) {
  const [open, setOpen] = useState(false);
  const [openTypes, setOpenTypes] = useState<Readonly<Record<string, number>>>({});
  return (
    <>
      <RelationRow
        label={label}
        title={typeKey}
        {...(project !== null
          ? {
              onActivate: () => {
                setOpen((value) => !value);
              },
            }
          : {})}
      >
        <span className="relation-code">{keyDisplayName(typeKey)}</span>
        {project === null ? <span className="relation-path"> · its project isn't connected</span> : null}
      </RelationRow>
      {open && project !== null ? (
        <div className="selected-definition">
          <TypeDefinition
            project={project}
            typeKey={typeKey}
            row={label}
            depth={1}
            openTypes={openTypes}
            onToggle={(row, span) => {
              setOpenTypes((current) => toggledOpenTypes(current, row, span));
            }}
          />
        </div>
      ) : null}
    </>
  );
}
