/**
 * Selected: what is selected on the page, pinned to the bottom of the board sidebar under a rule. A
 * card, a block drawn in one, a combined step, a connection, a frame, a resource card, a note, a
 * proposed step, a call's arrow between cards, or the calls an arrow stands for; a hint when nothing
 * is. It takes at most half the sidebar's height and scrolls on its own; its collapse is remembered per
 * window.
 */

import { IconButton, SectionLabel } from "../../components";
import { cardKey, findCard, findCombined, findFrame, findNote, findPage, findProposal, findResource, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed, useLayout } from "../../state/layout";
import { SelectedBlock } from "./SelectedBlock";
import { SelectedCalls } from "./SelectedCalls";
import { SelectedCard } from "./SelectedCard";
import { SelectedCombined } from "./SelectedCombined";
import { SelectedConnection } from "./SelectedConnection";
import { SelectedFrame } from "./SelectedFrame";
import { SelectedLine } from "./SelectedLine";
import { SelectedNote } from "./SelectedNote";
import { SelectedProposal } from "./SelectedProposal";
import { SelectedResource } from "./SelectedResource";

function SelectedBody({ target }: { readonly target: PageTarget }) {
  const selection = usePageView(target, (view) => view.selection);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  if (page && selection.kind === "card") {
    const card = findCard(page, selection.cardId);
    if (card && (selection.key === null || selection.key === cardKey(card))) {
      return <SelectedCard key={card.id} target={target} card={card} />;
    }
    if (card && selection.key !== null) {
      return <SelectedBlock key={`${card.id}:${selection.key}`} target={target} card={card} blockKey={selection.key} />;
    }
  }
  if (page && selection.kind === "combined") {
    const combined = findCombined(page, selection.combinedId);
    if (combined) {
      return <SelectedCombined key={combined.id} target={target} combined={combined} />;
    }
  }
  if (page && selection.kind === "link") {
    const link = page.links.find((item) => item.id === selection.linkId);
    if (link) {
      return <SelectedConnection key={link.id} target={target} link={link} />;
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
  if (page && selection.kind === "proposal") {
    const proposal = findProposal(page, selection.proposalId);
    if (proposal) {
      return <SelectedProposal key={proposal.id} target={target} proposal={proposal} />;
    }
  }
  if (page && selection.kind === "line") {
    return <SelectedLine key={selection.arrowId} target={target} line={selection} />;
  }
  if (page && selection.kind === "calls") {
    return <SelectedCalls key={selection.arrowId} target={target} calls={selection} />;
  }
  return <p className="selected-note">Select a card, a block, a frame, a resource card, a note, or a connection to see it here.</p>;
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
