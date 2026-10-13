/**
 * The shown board: the top bar over the stage, where the active page's canvas sits with the banners at
 * its top, the dock at its bottom, and the link-drawing hint. Only the shown board is mounted; each
 * page has its own React Flow store, so switching pages starts from that page's camera.
 *
 * It keeps a View code peek sheet following the page's selection, and applies Edit › Undo and Redo to
 * the board (the preload keeps them for a focused text field). Floating chrome centers on the canvas a
 * sheet leaves uncovered.
 */

import { ReactFlowProvider, useReactFlow, useStoreApi } from "@xyflow/react";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { BoardCanvas, arrangeBoard, useBoardArrows } from "../canvas";
import { isModalOpen, showToast } from "../components";
import { onShellEvent } from "../ipc";
import { boardKey, canUndo, findPage, redo, requestCardFocus, undo, useOpenBoard, usePageView, type BoardRef, type PageTarget } from "../state/boards";
import { useLayout } from "../state/layout";
import { usePeekTarget } from "../state/overlays";
import { BoardBanner } from "./BoardBanner";
import { Dock } from "./Dock";
import { TopBar } from "./TopBar";
import { useFollowSelection } from "./viewCode";
import "./board.css";

const EMPTY_CARDS: readonly never[] = [];

function PageStage({ target }: { readonly target: PageTarget }) {
  const flow = useReactFlow();
  const store = useStoreApi();
  const { board } = target;
  const cards = useOpenBoard(board, (open) => findPage(open.document, target.page)?.cards) ?? EMPTY_CARDS;
  const frames = useOpenBoard(board, (open) => findPage(open.document, target.page)?.frames);
  const resources = useOpenBoard(board, (open) => findPage(open.document, target.page)?.resources);
  const notes = useOpenBoard(board, (open) => findPage(open.document, target.page)?.notes);
  const links = useOpenBoard(board, (open) => findPage(open.document, target.page)?.links);
  const hasCamera = usePageView(target, (view) => view.camera !== null);
  const linking = usePageView(target, (view) => view.linking !== null);
  // Computed once here: the canvas draws them, Arrange orders by them.
  const boardArrows = useBoardArrows(target, cards);
  const { arrows, bodies } = boardArrows;

  // A page shown for the first time fits itself once its cards (or resource cards) are measured.
  const hasItems = cards.length > 0 || (resources ?? []).length > 0;
  const fitted = useRef(false);
  useEffect(() => {
    if (!hasCamera && !fitted.current && hasItems) {
      fitted.current = true;
      requestCardFocus(target, [], "fit");
    }
  }, [hasCamera, hasItems, target]);

  const onFit = useCallback(() => {
    requestCardFocus(target, [], "fit");
  }, [target]);
  const onZoomIn = useCallback(() => void flow.zoomIn({ duration: 160 }), [flow]);
  const onZoomOut = useCallback(() => void flow.zoomOut({ duration: 160 }), [flow]);
  const onArrange = useCallback(() => {
    const lookup = store.getState().nodeLookup;
    const moved = arrangeBoard(target, arrows, bodies, (arrow) => {
      const bounds = lookup.get(arrow.source)?.internals.handleBounds?.source?.find((item) => item.id === arrow.sourceHandle);
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
  }, [target, board, arrows, bodies, store]);

  return (
    <>
      <BoardCanvas target={target} cards={cards} frames={frames} resources={resources} notes={notes} links={links} boardArrows={boardArrows} />
      <div className="board-notices">
        <BoardBanner board={board} />
        {linking ? (
          <p className="board-hint" role="status">
            Click the card to link to · <kbd>esc</kbd> cancels
          </p>
        ) : null}
      </div>
      {cards.length === 0 && resources === undefined && notes === undefined ? <p className="board-empty">Open an entrance or a definition to start this page.</p> : null}
      <Dock hasItems={hasItems} onZoomIn={onZoomIn} onZoomOut={onZoomOut} onFit={onFit} onArrange={onArrange} />
    </>
  );
}

/** The width a floating sheet covers on the right of the content area (0 when none, or when it is pinned). */
function useCoveredRight(): string {
  const peek = usePeekTarget();
  const pinned = useLayout((layout) => layout.peek.pinned);
  return peek !== null && !pinned ? "var(--peek-width)" : "0px";
}

/**
 * Applies Edit › Undo and Redo to the board while it is shown (a focused text field never sends them),
 * except while a dialog covers it.
 */
function useMenuUndo(board: BoardRef): void {
  useEffect(
    () =>
      onShellEvent("menu_command", ({ command }) => {
        if (isModalOpen()) {
          return;
        }
        if (command === "undo") {
          undo(board);
        } else {
          redo(board);
        }
      }),
    [board],
  );
}

export function BoardView({ board }: { readonly board: BoardRef }) {
  const page = useOpenBoard(board, (open) => open.activePage);
  const { project, id } = board;
  const target = useMemo(() => (page !== undefined ? { board: { project, id }, page } : null), [project, id, page]);
  const covered = useCoveredRight();
  useMenuUndo(board);

  return (
    <div className="board-view" data-board-key={boardKey(board)} tabIndex={-1} style={{ ["--covered-right" as string]: covered }}>
      {target ? (
        <>
          <FollowSelection target={target} />
          <TopBar target={target} />
          <div className="board-stage">
            {/* Each page has its own React Flow store, so switching pages starts from that page's camera. */}
            <ReactFlowProvider key={target.page}>
              <PageStage target={target} />
            </ReactFlowProvider>
          </div>
        </>
      ) : (
        <p className="board-empty">Opening the board…</p>
      )}
    </div>
  );
}

function FollowSelection({ target }: { readonly target: PageTarget }) {
  useFollowSelection(target);
  return null;
}
