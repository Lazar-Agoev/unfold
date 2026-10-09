/**
 * The canvas node for one card. Its data is only the page and the card's ports, so a card re-renders
 * when its own document entry, model, or ports change, never because another card did or because the
 * camera moved (zoom is a camera and never changes what a card shows). The card reads from the project
 * its ref resolves to; one from a project that isn't connected shows as a placeholder.
 */

import type { Node, NodeProps } from "@xyflow/react";
import { memo } from "react";
import type { Card } from "../../ipc/generated";
import { cardKey, effectiveView, findPage, keyDisplayName, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { resolveRefProject, useProjectIndex } from "../../state/workspace";
import { CARD_MIN_WIDTH } from "../layout";
import { BlockCard } from "./BlockCard";
import { EntranceCard } from "./EntranceCard";
import { TargetHandle } from "./parts";
import "./cards.css";

export interface CardNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** Occurrence keys (or entrance target handles) with an arrow, sorted. */
  readonly ports: readonly string[];
}

export type CardNodeType = Node<CardNodeData, "card">;

/** A card whose project isn't connected: where it would be, and where it comes from. */
function NotConnectedCard({ card }: { readonly card: Card }) {
  const name = keyDisplayName(cardKey(card));
  return (
    <article className="board-card block-card placeholder" data-card-id={card.id} data-key={cardKey(card)} style={{ width: CARD_MIN_WIDTH }} aria-label={name}>
      <TargetHandle />
      <header className="card-header card-drag">
        <div className="card-title">
          <h3>{name}</h3>
        </div>
      </header>
      <div className="card-body">
        <p className="card-note">From {card.ref.project} · not connected</p>
      </div>
    </article>
  );
}

export const CardNode = memo(function CardNode({ id, data }: NodeProps<CardNodeType>) {
  const { target, ports } = data;
  // The card itself and the page's default view, not the page: another card's edit doesn't redraw this one.
  const card = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards.find((item) => item.id === id));
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const exception = usePageView(target, (view) => view.lensExceptions.includes(id));
  const selected = usePageView(target, (view) => view.selection.kind === "card" && view.selection.cardId === id);
  const index = useProjectIndex();
  if (!card) {
    return null;
  }
  const project = resolveRefProject(index, target.board.project, card.ref.project);
  if (project === null) {
    return <NotConnectedCard card={card} />;
  }
  if ("entrance" in card.ref) {
    return <EntranceCard target={target} project={project} card={card as Card & { ref: { entrance: string } }} ports={ports} selected={selected} />;
  }
  const view = effectiveView(lens, card.view, defaultView, exception);
  return <BlockCard target={target} project={project} card={card as Card & { ref: { block: string } }} view={view} ports={ports} selected={selected} />;
});
