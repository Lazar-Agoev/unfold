/**
 * Selected: what is selected on the page, pinned to the bottom of the board sidebar under a rule. A
 * card, a Flow node inside one, a group, a drawn link or a connection, a frame, a resource card, a
 * note, a line between cards, or the calls an arrow from a Diagram node stands for; a hint when nothing
 * is. It takes at most half
 * the sidebar's height and scrolls on its own; its collapse is remembered per window.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { IconButton, SectionLabel } from "../../components";
import { cardKey, findCard, findFrame, findNote, findPage, findResource, useOpenBoard, usePageView, useResolvedCardProject, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed, useLayout } from "../../state/layout";
import { SelectedCalls } from "./SelectedCalls";
import { SelectedCard } from "./SelectedCard";
import { SelectedConnection } from "./SelectedConnection";
import { SelectedFrame } from "./SelectedFrame";
import { SelectedGroup } from "./SelectedGroup";
import { SelectedLine } from "./SelectedLine";
import { SelectedLink } from "./SelectedLink";
import { SelectedNode, flowNodeFor } from "./SelectedNode";
import { SelectedNote } from "./SelectedNote";
import { SelectedResource } from "./SelectedResource";
import type { Card, Key } from "../../ipc/generated";

/** A card, or the Flow node of it that is selected (a statement key falls back to the card). */
function CardOrNode({ target, card, nodeKey }: { readonly target: PageTarget; readonly card: Card; readonly nodeKey: Key | null }) {
  const project = useResolvedCardProject(target.board, card);
  const query = useQuery({ ...cardQuery(project ?? "", cardKey(card)), enabled: project !== null });
  const flow = query.data?.type === "block" ? query.data.flow : null;
  const node = nodeKey !== null && nodeKey !== cardKey(card) ? flowNodeFor(flow, nodeKey) : undefined;
  if (project !== null && flow && node) {
    return <SelectedNode target={target} card={card} project={project} node={node} model={flow} />;
  }
  return <SelectedCard target={target} card={card} project={project} />;
}

function SelectedBody({ target }: { readonly target: PageTarget }) {
  const selection = usePageView(target, (view) => view.selection);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  if (page && selection.kind === "card") {
    const card = findCard(page, selection.cardId);
    if (card) {
      return <CardOrNode key={card.id} target={target} card={card} nodeKey={selection.key} />;
    }
  }
  if (page && selection.kind === "group") {
    const group = page.groups.find((item) => item.id === selection.groupId);
    if (group) {
      return <SelectedGroup key={group.id} target={target} group={group} />;
    }
  }
  if (page && selection.kind === "link") {
    const link = page.links.find((item) => item.id === selection.linkId);
    const kind = link?.kind;
    if (link && kind !== undefined) {
      return <SelectedConnection key={link.id} target={target} link={{ ...link, kind }} />;
    }
    if (link) {
      return <SelectedLink key={link.id} target={target} link={link} />;
    }
  }
  if (page && selection.kind === "frame") {
    const frame = findFrame(page, selection.frameId);
    if (frame) {
      return <SelectedFrame key={frame.id} target={target} frame={frame} />;
    }
  }
  if (page && selection.kind === "resource") {
    const resource = findResource(page, selection.resourceId);
    if (resource) {
      return <SelectedResource key={resource.id} target={target} resource={resource} />;
    }
  }
  if (page && selection.kind === "note") {
    const note = findNote(page, selection.noteId);
    if (note) {
      return <SelectedNote key={note.id} target={target} note={note} />;
    }
  }
  if (page && selection.kind === "line") {
    return <SelectedLine key={selection.arrowId} target={target} line={selection} />;
  }
  if (page && selection.kind === "calls") {
    return <SelectedCalls key={selection.arrowId} target={target} calls={selection} />;
  }
  return <p className="selected-note">Select a card, a group, a frame, a resource card, a note, or a link to see it here.</p>;
}

export function SelectedSection({ target }: { readonly target: PageTarget }) {
  const collapsed = useLayout((layout) => layout.selectedCollapsed);
  return (
    <section className={`board-selected${collapsed ? " collapsed" : ""}`} aria-label="Selected">
      <SectionLabel
        trailing={
          <IconButton
            icon={collapsed ? "chevron-right" : "chevron-down"}
            label={collapsed ? "Show Selected" : "Hide Selected"}
            size={24}
            onClick={() => {
              setSelectedCollapsed(!collapsed);
            }}
          />
        }
      >
        Selected
      </SectionLabel>
      {collapsed ? null : (
        <div className="board-selected-body">
          <SelectedBody target={target} />
        </div>
      )}
    </section>
  );
}
