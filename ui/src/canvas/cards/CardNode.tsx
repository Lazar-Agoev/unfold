/**
 * The canvas node for one card. Its data is only the page and the card's ports, so a card re-renders
 * when its own document entry, what the reader drew for it, or its ports change, never because another
 * card did or because the camera moved (zoom is a camera and never changes what a card shows). What it
 * shows follows what the reader drew and the card's view (its own, else the page's): an app's or
 * entrance's card; a type's fields (a class's while compact); or a definition's header, compact, or
 * expanded to its drawn flow.
 */

import type { Node, NodeProps } from "@xyflow/react";
import { memo } from "react";
import type { Card, View } from "../../ipc/generated";
import { cardView, findPage, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { useCard } from "../../state/cards";
import { cardShape } from "../cardText";
import { BlockCard } from "./BlockCard";
import { EntranceCard } from "./EntranceCard";
import { TypeCard } from "./TypeCard";
import "./cards.css";

interface CardNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** The handle ids with an arrow or a connection leaving the card, sorted. */
  readonly ports: readonly string[];
}

export type CardNodeType = Node<CardNodeData, "card">;

interface DrawnCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly view: View;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

/** A card as the reader drew it. */
function DrawnCard({ target, card, view, ports, selected }: DrawnCardProps) {
  const query = useCard(target, card);
  const drawn = query.data;
  if (drawn?.type === "start") {
    return <EntranceCard target={target} card={card} start={drawn} shape={cardShape(drawn, view)} ports={ports} selected={selected} />;
  }
  if (drawn && cardShape(drawn, view) === "fields") {
    return <TypeCard target={target} card={card} drawn={drawn} ports={ports} selected={selected} />;
  }
  return <BlockCard target={target} card={card} expanded={view === "expanded"} query={query} ports={ports} selected={selected} />;
}

export const CardNode = memo(function CardNode({ id, data }: NodeProps<CardNodeType>) {
  const { target, ports } = data;
  // The card itself and the page's default view, not the page: another card's edit doesn't redraw this one.
  const card = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards.find((item) => item.id === id));
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "compact";
  const selected = usePageView(target, ({ selection }) => (selection.kind === "card" || selection.kind === "field") && selection.cardId === id);
  return card ? <DrawnCard target={target} card={card} view={cardView(card, defaultView)} ports={ports} selected={selected} /> : null;
});
