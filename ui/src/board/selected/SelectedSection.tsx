/**
 * Selected: what is selected on the page, pinned to the bottom of the board sidebar under a rule. A
 * card, a Flow node inside one, a group, a drawn link, or a line between cards; a hint when nothing is. It takes at most half
 * the sidebar's height and scrolls on its own; its collapse is remembered per window.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { IconButton, SectionLabel } from "../../components";
import { cardKey, findCard, findPage, useOpenBoard, usePageView, useResolvedCardProject, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed, useLayout } from "../../state/layout";
import { SelectedCard } from "./SelectedCard";
import { SelectedGroup } from "./SelectedGroup";
import { SelectedLine } from "./SelectedLine";
import { SelectedLink } from "./SelectedLink";
import { SelectedNode, flowNodeFor } from "./SelectedNode";
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
    if (link) {
      return <SelectedLink key={link.id} target={target} link={link} />;
    }
  }
  if (page && selection.kind === "line") {
    return <SelectedLine key={selection.arrowId} target={target} line={selection} />;
  }
  return <p className="selected-note">Select a card, a group, or a link to see it here.</p>;
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
