/**
 * A board in its tab: the top bar over the stage, where the active page's canvas sits with the banners
 * at its top, the dock at its bottom, and the link-drawing hint. Only the active board tab is mounted;
 * each page has its own React Flow store, so switching pages starts from that page's camera.
 *
 * It registers the board's actions while mounted (Back, Forward, Fit, zoom, Arrange, the lens, New
 * page, Publish, the Share and Board commands), and the board's undo scope: ⌘Z reaches it when the
 * board or its sidebar has focus or was clicked last. Floating chrome centers on the canvas a sheet
 * leaves uncovered.
 */

import { ReactFlowProvider, useReactFlow, useStoreApi } from "@xyflow/react";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { BoardCanvas, arrangeBoard, showCallers, useBoardArrows } from "../canvas";
import { showToast } from "../components";
import { toUnfoldError, type View } from "../ipc";
import { ActionIds, requestFocus, useCommandScope, useRegisterActions } from "../keyboard";
import {
  addPage,
  boardKey,
  canUndo,
  copyPageMermaid,
  exportPageMermaid,
  findCard,
  findPage,
  foldAll,
  getOpenBoard,
  getPageView,
  redo,
  removeCard,
  removeLink,
  requestCardFocus,
  setPageLens,
  travel,
  undo,
  ungroup,
  useOpenBoard,
  usePageView,
  type BoardRef,
  type PageTarget,
} from "../state/boards";
import { setSelectedCollapsed, useLayout } from "../state/layout";
import { openDialog, usePeekTarget } from "../state/overlays";
import { BoardBanner } from "./BoardBanner";
import { DESCRIPTION_FIELD } from "./selected/SelectedCard";
import { Dock } from "./Dock";
import { TopBar } from "./TopBar";
import { stepUnlabeled, useUnlabeled } from "./unlabeled";
import "./board.css";

const EMPTY_CARDS: readonly never[] = [];

/** The selected card of a page, when it is a block card (Show callers, Make a group from lines). */
function selectedBlockCard(target: PageTarget): string | null {
  const { selection } = getPageView(target);
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const card = selection.kind === "card" && page ? findCard(page, selection.cardId) : undefined;
  return card && "block" in card.ref ? card.id : null;
}

/** Removes whatever is selected on the page: a card, a group (ungrouped), or a link (not a line between cards). */
function removeSelected(target: PageTarget): void {
  const { selection } = getPageView(target);
  if (selection.kind === "card") {
    removeCard(target, selection.cardId);
  } else if (selection.kind === "group") {
    ungroup(target, selection.groupId);
  } else if (selection.kind === "link") {
    removeLink(target, selection.linkId);
  }
}

/** The board's own commands (the page's, while it is shown). */
function useBoardActions(target: PageTarget): void {
  const { board } = target;
  const unlabeled = useUnlabeled(target);
  // A selected line is derived from the cards: there is nothing of it to remove.
  const hasSelection = () => !["none", "line"].includes(getPageView(target).selection.kind);
  const setLens = (view: View) => () => {
    setPageLens(target, view);
  };
  useRegisterActions([
    { id: ActionIds.newPage, title: "New Page", category: "Board", icon: "page", run: () => void addPage(board) },
    { id: ActionIds.lensCode, title: "Code", category: "View", run: setLens("code") },
    { id: ActionIds.lensFlow, title: "Flow", category: "View", run: setLens("flow") },
    {
      id: ActionIds.publish,
      title: "Publish",
      category: "Board",
      icon: "publish",
      run: () => {
        openDialog({ kind: "publish", board });
      },
    },
    { id: "board.foldAll", title: "Fold All Cards", category: "Board", run: () => { foldAll(target, true); } },
    { id: "board.unfoldAll", title: "Unfold All Cards", category: "Board", run: () => { foldAll(target, false); } },
    {
      id: "board.groupFromLines",
      title: "Make a Group from Lines…",
      category: "Board",
      icon: "group",
      enabled: () => selectedBlockCard(target) !== null,
      run: () => {
        const cardId = selectedBlockCard(target);
        if (cardId !== null) {
          openDialog({ kind: "groupFromLines", target, cardId });
        }
      },
    },
    {
      id: "board.describeSelected",
      title: "Describe the Selected Card",
      category: "Board",
      icon: "edit",
      enabled: () => getPageView(target).selection.kind === "card",
      run: () => {
        setSelectedCollapsed(false);
        requestFocus(DESCRIPTION_FIELD);
      },
    },
    {
      id: "board.nextUnlabeled",
      title: "Label the Next Flow Step",
      category: "Board",
      enabled: () => unlabeled.length > 0,
      run: () => {
        stepUnlabeled(target, unlabeled);
      },
    },
    { id: "board.removeSelected", title: "Remove the Selection from the Page", category: "Board", enabled: hasSelection, run: () => { removeSelected(target); } },
    {
      id: "board.showCallers",
      title: "Show Callers",
      category: "Board",
      enabled: () => selectedBlockCard(target) !== null,
      run: () => {
        const cardId = selectedBlockCard(target);
        if (cardId !== null) {
          void showCallers(target, cardId);
        }
      },
    },
    {
      id: "board.exportMermaid",
      title: "Export Mermaid…",
      category: "Share",
      group: "Share",
      run: async () => {
        try {
          await exportPageMermaid(target);
        } catch (error) {
          showToast({ title: "Couldn't export the page", body: toUnfoldError(error).message, tone: "danger" });
        }
      },
    },
    {
      id: "board.copyMermaid",
      title: "Copy Mermaid",
      category: "Share",
      group: "Share",
      icon: "copy",
      run: async () => {
        try {
          await copyPageMermaid(target);
          showToast({ title: "Copied the page as Mermaid." });
        } catch (error) {
          showToast({ title: "Couldn't copy the page", body: toUnfoldError(error).message, tone: "danger" });
        }
      },
    },
  ]);
}

function PageStage({ target }: { readonly target: PageTarget }) {
  const flow = useReactFlow();
  const store = useStoreApi();
  const { board } = target;
  const cards = useOpenBoard(board, (open) => findPage(open.document, target.page)?.cards) ?? EMPTY_CARDS;
  const links = useOpenBoard(board, (open) => findPage(open.document, target.page)?.links);
  const hasCamera = usePageView(target, (view) => view.camera !== null);
  const linking = usePageView(target, (view) => view.linking !== null);
  // Computed once here: the canvas draws them, Arrange orders by them.
  const boardArrows = useBoardArrows(board.project, cards);
  const { arrows } = boardArrows;

  // A page shown for the first time fits itself once its cards are measured.
  const fitted = useRef(false);
  useEffect(() => {
    if (!hasCamera && !fitted.current && cards.length > 0) {
      fitted.current = true;
      requestCardFocus(target, [], "fit");
    }
  }, [hasCamera, cards.length, target]);

  const onTravel = useCallback(
    (direction: "back" | "forward") => {
      const move = travel(target, direction);
      if (move?.camera) {
        void flow.setViewport(move.camera, { duration: 240 });
      }
    },
    [target, flow],
  );
  const onFit = useCallback(() => {
    requestCardFocus(target, [], "fit");
  }, [target]);
  const onZoomIn = useCallback(() => void flow.zoomIn({ duration: 160 }), [flow]);
  const onZoomOut = useCallback(() => void flow.zoomOut({ duration: 160 }), [flow]);
  const onArrange = useCallback(() => {
    const lookup = store.getState().nodeLookup;
    const moved = arrangeBoard(target, arrows, (arrow) => {
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
  }, [target, board, arrows, store]);

  useRegisterActions([
    { id: ActionIds.back, title: "Back", category: "Go", icon: "back", run: () => { onTravel("back"); } },
    { id: ActionIds.forward, title: "Forward", category: "Go", icon: "forward", run: () => { onTravel("forward"); } },
    { id: ActionIds.fit, title: "Fit", category: "Board", run: onFit },
    { id: ActionIds.zoomIn, title: "Zoom In", category: "Board", repeatable: true, run: onZoomIn },
    { id: ActionIds.zoomOut, title: "Zoom Out", category: "Board", repeatable: true, run: onZoomOut },
    { id: ActionIds.arrange, title: "Arrange by Call Depth", category: "Board", icon: "arrange", run: onArrange },
  ]);

  return (
    <>
      <BoardCanvas target={target} cards={cards} links={links} boardArrows={boardArrows} />
      <div className="board-notices">
        <BoardBanner board={board} />
        {linking ? (
          <p className="board-hint" role="status">
            Click the card to link to · <kbd>esc</kbd> cancels
          </p>
        ) : null}
      </div>
      {cards.length === 0 ? <p className="board-empty">Open an entrance or a definition to start this page.</p> : null}
      <Dock target={target} hasCards={cards.length > 0} onTravel={onTravel} onZoomIn={onZoomIn} onZoomOut={onZoomOut} onFit={onFit} onArrange={onArrange} />
    </>
  );
}

/** The width a floating sheet covers on the right of the content area (0 when none, or when it is pinned). */
function useCoveredRight(): string {
  const peek = usePeekTarget();
  const pinned = useLayout((layout) => layout.peek.pinned);
  return peek !== null && !pinned ? "var(--peek-width)" : "0px";
}

export function BoardView({ board }: { readonly board: BoardRef }) {
  const root = useRef<HTMLDivElement>(null);
  const page = useOpenBoard(board, (open) => open.activePage);
  // Subscribed so the scope's handlers below are current.
  const undoable = useOpenBoard(board, (open) => open.history.past.length > 0) ?? false;
  const redoable = useOpenBoard(board, (open) => open.history.future.length > 0) ?? false;
  const { project, id } = board;
  const target = useMemo(() => (page !== undefined ? { board: { project, id }, page } : null), [project, id, page]);
  const covered = useCoveredRight();

  useCommandScope(root, {
    undo: undoable ? () => undo(board) : undefined,
    redo: redoable ? () => redo(board) : undefined,
  });

  return (
    <div ref={root} className="board-view" data-board-key={boardKey(board)} tabIndex={-1} style={{ ["--covered-right" as string]: covered }}>
      {target ? (
        <>
          <BoardActions target={target} />
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

function BoardActions({ target }: { readonly target: PageTarget }) {
  useBoardActions(target);
  return null;
}
