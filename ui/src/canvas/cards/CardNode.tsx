/**
 * The canvas node for one card. Its data is only the page and the card's ports, so a card re-renders
 * when its own document entry, model, or ports change, never because another card did or because the
 * camera moved (zoom is a camera and never changes what a card shows). The card reads from the board's
 * project. What it shows
 * follows the view the page's lens gives it (`shownView`, `cardBody`): in Code, Code (an entrance its
 * handlers); in Diagram, a function or an entrance compact (a Diagram node) or expanded (its Flow, an
 * entrance's evidence and targets) by its own chevron; a type declaration always shows its fields, and
 * a class shows its fields compact, its Flow expanded, and its Code in the Code lens.
 */

import type { Node, NodeProps } from "@xyflow/react";
import { memo } from "react";
import type { Card } from "../../ipc/generated";
import { cardBody } from "../../code/typeKinds";
import { cardKey, findPage, pageLens, shownView, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { BlockCard } from "./BlockCard";
import { DiagramCard } from "./DiagramCard";
import { EntranceCard } from "./EntranceCard";
import { TypeCard } from "./TypeCard";
import "./cards.css";

interface CardNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** Occurrence keys (or entrance target handles) with an arrow, sorted. */
  readonly ports: readonly string[];
}

export type CardNodeType = Node<CardNodeData, "card">;

export const CardNode = memo(function CardNode({ id, data }: NodeProps<CardNodeType>) {
  const { target, ports } = data;
  // The card itself and the page's default view, not the page: another card's edit doesn't redraw this one.
  const card = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards.find((item) => item.id === id));
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const selected = usePageView(target, (view) => view.selection.kind === "card" && view.selection.cardId === id);
  if (!card) {
    return null;
  }
  const { project } = target.board;
  const view = shownView(lens, card, defaultView);
  const body = cardBody(cardKey(card), view);
  if (body === "node") {
    return <DiagramCard target={target} project={project} card={card} ports={ports} selected={selected} />;
  }
  if ("entrance" in card.ref) {
    const entrance = card as Card & { ref: { entrance: string } };
    const expandable = pageLens(lens, defaultView) === "diagram";
    return <EntranceCard target={target} project={project} card={entrance} expandable={expandable} ports={ports} selected={selected} />;
  }
  const block = card as Card & { ref: { block: string } };
  if (body === "fields") {
    return <TypeCard target={target} project={project} card={block} ports={ports} selected={selected} />;
  }
  return <BlockCard target={target} project={project} card={block} view={body === "flow" ? "flow" : "code"} ports={ports} selected={selected} />;
});
