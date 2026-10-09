/**
 * The inspector (right; architecture §11.5, U-29). Closed by default (⌥⌘B or `]`), and it never
 * shows code. It follows the active board's selection: a card shows its description and relations,
 * a Flow node its type, lines, label, and paths. Every edit is a board edit signed with the
 * person's name, so ⌘Z undoes it here as on the canvas.
 */

import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useRef } from "react";
import { cardQuery } from "../canvas/cardData";
import { IconButton } from "../components";
import { ActionIds, actions, useCommandScope, useFocusTarget } from "../keyboard";
import { useActiveBoardId } from "../state/boards/api";
import { cardKey, findCard } from "../state/boards/document";
import { redo, undo, useBoards } from "../state/boards/store";
import { CardInspector } from "./CardInspector";
import { NodeInspector, flowNodeFor } from "./NodeInspector";
import "./inspector.css";

function Selected({ boardId, cardId, selectedKey }: { readonly boardId: string; readonly cardId: string; readonly selectedKey: string | null }) {
  const card = useBoards((state) => {
    const board = state.boards[boardId];
    return board ? findCard(board.document, cardId) : undefined;
  });
  const key = card ? cardKey(card) : "";
  const block = card !== undefined && "block" in card.ref;
  const nodeKey = block && selectedKey !== null && selectedKey !== key ? selectedKey : null;
  // The card model carries the code-name fallbacks; the board's labels are applied over them.
  const plain = useQuery({ ...cardQuery(key), enabled: nodeKey !== null });
  if (!card) {
    return <p className="pane-hint">Select a card or a Flow node to see its details.</p>;
  }
  const model = plain.data?.type === "block" ? plain.data.flow : null;
  const node = nodeKey !== null ? flowNodeFor(model, nodeKey) : undefined;
  if (node && model) {
    return <NodeInspector key={nodeKey} boardId={boardId} card={card} node={node} model={model} />;
  }
  return <CardInspector key={card.id} boardId={boardId} card={card} />;
}

export function Inspector() {
  const root = useRef<HTMLElement>(null);
  const boardId = useActiveBoardId();
  const cardId = useBoards((state) => (boardId !== null ? (state.boards[boardId]?.view.selectedCardId ?? null) : null));
  const selectedKey = useBoards((state) => (boardId !== null ? (state.boards[boardId]?.view.selectedKey ?? null) : null));
  useFocusTarget("inspector", root);
  useCommandScope(root, {
    undo: boardId !== null ? () => undo(boardId) : undefined,
    redo: boardId !== null ? () => redo(boardId) : undefined,
  });

  return (
    <aside ref={root} className="pane inspector" aria-labelledby="inspector-title" tabIndex={-1}>
      <div className="pane-header-row">
        <h2 className="pane-header" id="inspector-title">
          Inspector
        </h2>
        <IconButton
          icon={X}
          label="Close Inspector"
          size="small"
          onClick={() => {
            actions.run(ActionIds.toggleInspector);
          }}
        />
      </div>
      {boardId !== null && cardId !== null ? (
        <Selected boardId={boardId} cardId={cardId} selectedKey={selectedKey} />
      ) : (
        <p className="pane-hint">Select a card or a Flow node to see its details.</p>
      )}
    </aside>
  );
}
