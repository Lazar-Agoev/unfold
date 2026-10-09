/**
 * A board tab: the active page's canvas with its cards and arrows, and the bottom bar with Back, the
 * trail, the camera controls, and Arrange. While active it answers Back/Forward (⌘[ ⌘]), Fit (⇧1),
 * zoom (⌘= ⌘-), and Arrange (⇧A); undo and redo reach it through its command scope when it has focus
 * (or was clicked last). Switching pages remounts the canvas on that page's own camera.
 */

import { ReactFlowProvider, useReactFlow, useStoreApi } from "@xyflow/react";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { showToast } from "../components";
import { ActionIds, useCommandScope, useRegisterActions } from "../keyboard";
import {
  boardKey,
  canUndo,
  findPage,
  redo,
  requestCardFocus,
  travel,
  undo,
  useOpenBoard,
  usePageView,
  type BoardRef,
  type PageTarget,
} from "../state/boards";
import { arrangeBoard } from "./arrangeBoard";
import { BoardBanner } from "./BoardBanner";
import { BoardBar } from "./BoardBar";
import { BoardCanvas } from "./BoardCanvas";
import { useBoardArrows } from "./useBoardArrows";
import "./canvas.css";

export interface BoardTabProps {
  readonly board: BoardRef;
  readonly active: boolean;
}

const EMPTY_CARDS: readonly never[] = [];

function PageContent({ target, active }: { readonly target: PageTarget; readonly active: boolean }) {
  const flow = useReactFlow();
  const store = useStoreApi();
  const { board } = target;
  const cards = useOpenBoard(board, (open) => findPage(open.document, target.page)?.cards) ?? EMPTY_CARDS;
  const hasCamera = usePageView(target, (view) => view.camera !== null);
  // Computed once here: the canvas draws them, Arrange orders by them.
  const boardArrows = useBoardArrows(board.project, cards);
  const { arrows } = boardArrows;

  // A page shown for the first time fits itself once its cards are measured.
  const fitted = useRef(false);
  useEffect(() => {
    if (active && !hasCamera && !fitted.current && cards.length > 0) {
      fitted.current = true;
      requestCardFocus(target, [], "fit");
    }
  }, [active, hasCamera, cards.length, target]);

  const back = useCallback(
    (direction: "back" | "forward") => {
      const move = travel(target, direction);
      if (move?.camera) {
        void flow.setViewport(move.camera, { duration: 240 });
      }
    },
    [target, flow],
  );
  const fit = useCallback(() => {
    requestCardFocus(target, [], "fit");
  }, [target]);
  const zoomIn = useCallback(() => void flow.zoomIn({ duration: 160 }), [flow]);
  const zoomOut = useCallback(() => void flow.zoomOut({ duration: 160 }), [flow]);
  const arrange = useCallback(() => {
    const lookup = store.getState().nodeLookup;
    const moved = arrangeBoard(target, arrows, (cardId, handle) => {
      const bounds = lookup.get(cardId)?.internals.handleBounds?.source?.find((item) => item.id === handle);
      return bounds ? bounds.y + bounds.height / 2 : undefined;
    });
    requestCardFocus(target, [], "fit");
    if (moved > 0) {
      showToast({
        title: `Arranged ${moved} ${moved === 1 ? "card" : "cards"} by call depth.`,
        action: {
          label: "Undo",
          run: () => {
            if (canUndo(board)) {
              undo(board);
            }
          },
        },
      });
    }
  }, [target, board, arrows, store]);

  return (
    <>
      <BoardCanvas target={target} active={active} cards={cards} boardArrows={boardArrows} />
      <BoardBanner board={board} />
      {cards.length === 0 ? <p className="board-empty">Open an entrance or a definition to start this page.</p> : null}
      <BoardBar
        target={target}
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
    { id: ActionIds.fit, title: "Fit", category: "Board", run: onFit },
    { id: ActionIds.zoomIn, title: "Zoom In", category: "Board", repeatable: true, run: onZoomIn },
    { id: ActionIds.zoomOut, title: "Zoom Out", category: "Board", repeatable: true, run: onZoomOut },
    { id: ActionIds.arrange, title: "Arrange by Call Depth", category: "Board", run: onArrange },
  ]);
  return null;
}

export function BoardTab({ board, active }: BoardTabProps) {
  const root = useRef<HTMLDivElement>(null);
  const page = useOpenBoard(board, (open) => open.activePage);
  // Subscribed so the scope's handlers below are current.
  const undoable = useOpenBoard(board, (open) => open.history.past.length > 0) ?? false;
  const redoable = useOpenBoard(board, (open) => open.history.future.length > 0) ?? false;
  const { project, id } = board;
  const target = useMemo(() => (page !== undefined ? { board: { project, id }, page } : null), [project, id, page]);

  useCommandScope(root, {
    undo: undoable ? () => undo(board) : undefined,
    redo: redoable ? () => redo(board) : undefined,
  });

  return (
    <div ref={root} className="board-tab" data-board-key={boardKey(board)} tabIndex={-1}>
      {target ? (
        // Each page has its own React Flow store, so switching pages starts from that page's camera.
        <ReactFlowProvider key={target.page}>
          <PageContent target={target} active={active} />
        </ReactFlowProvider>
      ) : (
        <p className="board-empty">Opening the board…</p>
      )}
    </div>
  );
}
