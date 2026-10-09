/**
 * A board tab (architecture §11.4): the canvas with its cards and arrows, and the bottom bar with
 * Back, the trail, the camera controls, and Arrange. While active it answers Back/Forward (⌘[ ⌘]),
 * Fit (⇧1), and zoom (⌘= ⌘-); undo and redo reach it through its command scope when it has focus
 * (or was clicked last).
 */

import { ReactFlowProvider, useReactFlow, useStoreApi } from "@xyflow/react";
import { useCallback, useEffect, useRef } from "react";
import { showToast } from "../components";
import { ActionIds, useCommandScope, useRegisterActions } from "../keyboard";
import { canUndo, redo, requestFocus, setActiveBoard, travel, undo, useBoards } from "../state/boards/store";
import { installBoardWiring } from "../state/boards/wiring";
import { arrangeBoard } from "./arrangeBoard";
import { BoardBanner } from "./BoardBanner";
import { BoardBar } from "./BoardBar";
import { BoardCanvas } from "./BoardCanvas";
import { useBoardArrows } from "./useBoardArrows";
import "./canvas.css";

export interface BoardTabProps {
  readonly boardId: string;
  readonly active: boolean;
}

const EMPTY_CARDS: readonly never[] = [];

function BoardTabContent({ boardId, active }: BoardTabProps) {
  const flow = useReactFlow();
  const store = useStoreApi();
  const root = useRef<HTMLDivElement>(null);
  const exists = useBoards((state) => state.boards[boardId] !== undefined);
  const cards = useBoards((state) => state.boards[boardId]?.document.cards ?? EMPTY_CARDS);
  const hasCamera = useBoards((state) => (state.boards[boardId]?.view.camera ?? null) !== null);
  // Subscribed so the scope's handlers below are current.
  const undoable = useBoards((state) => (state.boards[boardId]?.history.past.length ?? 0) > 0);
  const redoable = useBoards((state) => (state.boards[boardId]?.history.future.length ?? 0) > 0);
  // Computed once here: the canvas draws them, Arrange orders by them.
  const boardArrows = useBoardArrows(cards);
  const { arrows } = boardArrows;

  useEffect(() => {
    if (active) {
      setActiveBoard(boardId);
    }
  }, [active, boardId]);

  // A board shown for the first time fits itself once its cards are measured.
  const fitted = useRef(false);
  useEffect(() => {
    if (active && exists && !hasCamera && !fitted.current && cards.length > 0) {
      fitted.current = true;
      requestFocus(boardId, [], "fit");
    }
  }, [active, exists, hasCamera, cards.length, boardId]);

  useCommandScope(root, {
    undo: undoable ? () => undo(boardId) : undefined,
    redo: redoable ? () => redo(boardId) : undefined,
  });

  const back = useCallback(
    (direction: "back" | "forward") => {
      const move = travel(boardId, direction);
      if (move?.camera) {
        void flow.setViewport(move.camera, { duration: 240 });
      }
    },
    [boardId, flow],
  );
  const fit = useCallback(() => {
    requestFocus(boardId, [], "fit");
  }, [boardId]);
  const zoomIn = useCallback(() => void flow.zoomIn({ duration: 160 }), [flow]);
  const zoomOut = useCallback(() => void flow.zoomOut({ duration: 160 }), [flow]);
  const arrange = useCallback(() => {
    const lookup = store.getState().nodeLookup;
    const moved = arrangeBoard(boardId, arrows, (cardId, handle) => {
      const bounds = lookup.get(cardId)?.internals.handleBounds?.source?.find((item) => item.id === handle);
      return bounds ? bounds.y + bounds.height / 2 : undefined;
    });
    requestFocus(boardId, [], "fit");
    if (moved > 0) {
      showToast({
        message: `Arranged ${moved} ${moved === 1 ? "card" : "cards"} by call depth.`,
        action: {
          label: "Undo",
          run: () => {
            if (canUndo(boardId)) {
              undo(boardId);
            }
          },
        },
      });
    }
  }, [boardId, arrows, store]);

  return (
    <div ref={root} className="board-tab" data-board-id={boardId} tabIndex={-1}>
      {exists ? (
        <>
          <BoardCanvas boardId={boardId} active={active} cards={cards} boardArrows={boardArrows} />
          <BoardBanner boardId={boardId} />
          {cards.length === 0 ? <p className="board-empty">Open an entrance or a definition to start this board.</p> : null}
          <BoardBar
            boardId={boardId}
            onBack={() => {
              back("back");
            }}
            onZoomIn={zoomIn}
            onZoomOut={zoomOut}
            onFit={fit}
            onArrange={arrange}
          />
          {active ? <BoardActions onTravel={back} onFit={fit} onZoomIn={zoomIn} onZoomOut={zoomOut} onArrange={arrange} /> : null}
        </>
      ) : (
        <p className="board-empty">Opening the board…</p>
      )}
    </div>
  );
}

/** The active board's keyboard actions; mounted only while its tab is active. */
function BoardActions({
  onTravel,
  onFit,
  onZoomIn,
  onZoomOut,
  onArrange,
}: {
  readonly onTravel: (direction: "back" | "forward") => void;
  readonly onFit: () => void;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onArrange: () => void;
}) {
  useRegisterActions([
    { id: ActionIds.back, title: "Back", category: "Go", run: () => { onTravel("back"); } },
    { id: ActionIds.forward, title: "Forward", category: "Go", run: () => { onTravel("forward"); } },
    { id: ActionIds.fitBoard, title: "Fit Board", category: "Board", run: onFit },
    { id: ActionIds.zoomIn, title: "Zoom In", category: "Board", repeatable: true, run: onZoomIn },
    { id: ActionIds.zoomOut, title: "Zoom Out", category: "Board", repeatable: true, run: onZoomOut },
    { id: "board.arrange", title: "Arrange by Call Depth", category: "Board", run: onArrange },
  ]);
  return null;
}

export function BoardTab({ boardId, active }: BoardTabProps) {
  useEffect(() => {
    installBoardWiring();
  }, []);
  return (
    <ReactFlowProvider>
      <BoardTabContent boardId={boardId} active={active} />
    </ReactFlowProvider>
  );
}
