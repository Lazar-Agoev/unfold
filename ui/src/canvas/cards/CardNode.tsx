/**
 * The canvas node for one card. Its data is only the board, the card id, and its ports, so a card
 * re-renders when its own document entry, model, or ports change, never because another card did
 * or because the camera moved (zoom is a camera and never changes what a card shows).
 */

import type { Node, NodeProps } from "@xyflow/react";
import { memo } from "react";
import type { Card } from "../../ipc/generated";
import { effectiveView, useBoards } from "../../state/boards/store";
import { BlockCard } from "./BlockCard";
import { EntranceCard } from "./EntranceCard";
import "./cards.css";

export interface CardNodeData extends Record<string, unknown> {
  readonly boardId: string;
  /** Occurrence keys (or entrance target handles) with an arrow, sorted. */
  readonly ports: readonly string[];
}

export type CardNodeType = Node<CardNodeData, "card">;

export const CardNode = memo(function CardNode({ id, data }: NodeProps<CardNodeType>) {
  const { boardId, ports } = data;
  const card = useBoards((state) => state.boards[boardId]?.document.cards.find((item) => item.id === id));
  const defaultView = useBoards((state) => state.boards[boardId]?.document.defaultView ?? "code");
  const lens = useBoards((state) => state.boards[boardId]?.view.lens ?? null);
  const exception = useBoards((state) => state.boards[boardId]?.view.lensExceptions.includes(id) ?? false);
  const selected = useBoards((state) => state.boards[boardId]?.view.selectedCardId === id);
  if (!card) {
    return null;
  }
  if ("entrance" in card.ref) {
    return <EntranceCard boardId={boardId} card={card as Card & { ref: { entrance: string } }} ports={ports} selected={selected} />;
  }
  const view = effectiveView(lens, card.view, defaultView, exception);
  return <BlockCard boardId={boardId} card={card as Card & { ref: { block: string } }} view={view} ports={ports} selected={selected} />;
});
