/**
 * The bar at the bottom of a board: Back, the breadcrumb trail of focus moves ("POST /orders ›
 * createOrder"), and the camera controls (− + Fit) with Arrange by call depth.
 */

import { ArrowLeft, Workflow } from "lucide-react";
import { IconButton, Tooltip } from "../components";
import { ActionIds, useShortcut } from "../keyboard";
import { findPage, focusCard, pushFocusMove, useOpenBoard, usePageView, type PageTarget } from "../state/boards";

export interface BoardBarProps {
  readonly target: PageTarget;
  readonly onBack: () => void;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onFit: () => void;
  readonly onArrange: () => void;
}

export function BoardBar({ target, onBack, onZoomIn, onZoomOut, onFit, onArrange }: BoardBarProps) {
  const trail = usePageView(target, (view) => view.trail);
  const canGoBack = usePageView(target, (view) => view.back.length > 0);
  const hasCards = useOpenBoard(target.board, (board) => (findPage(board.document, target.page)?.cards.length ?? 0) > 0) ?? false;
  const backKey = useShortcut(ActionIds.back);
  const fitKey = useShortcut(ActionIds.fit);
  const zoomInKey = useShortcut(ActionIds.zoomIn);
  const zoomOutKey = useShortcut(ActionIds.zoomOut);
  return (
    <div className="board-bar" role="toolbar" aria-label="Board navigation">
      <Tooltip content="Back" {...(backKey !== null ? { shortcut: backKey } : {})} side="top" describe={false}>
        <button type="button" className="board-back" onClick={onBack} disabled={!canGoBack} aria-label="Back">
          <ArrowLeft size={13} aria-hidden="true" />
          Back
        </button>
      </Tooltip>
      {trail.length > 0 ? (
        <nav className="board-trail" aria-label="Trail">
          <ol>
            {trail.map((item, index) => (
              <li key={`${item.cardId}-${index}`} className={index === trail.length - 1 ? "current" : undefined}>
                {index > 0 ? (
                  <span className="board-trail-separator" aria-hidden="true">
                    ›
                  </span>
                ) : null}
                <button
                  type="button"
                  onClick={() => {
                    pushFocusMove(target);
                    focusCard(target, item.cardId);
                  }}
                >
                  {item.label}
                </button>
              </li>
            ))}
          </ol>
        </nav>
      ) : null}
      <div className="board-zoom" role="group" aria-label="Camera">
        <IconButton icon="minus" label="Zoom Out" {...(zoomOutKey !== null ? { shortcut: zoomOutKey } : {})} size={24} tooltipSide="top" onClick={onZoomOut} />
        <IconButton icon="plus" label="Zoom In" {...(zoomInKey !== null ? { shortcut: zoomInKey } : {})} size={24} tooltipSide="top" onClick={onZoomIn} />
        <IconButton icon="layers" label="Fit Board" {...(fitKey !== null ? { shortcut: fitKey } : {})} size={24} tooltipSide="top" onClick={onFit} disabled={!hasCards} />
        <span className="board-zoom-divider" aria-hidden="true" />
        <Tooltip content="Arrange by call depth: callers left, callees right" side="top" describe={false}>
          <button type="button" className="board-arrange" onClick={onArrange} disabled={!hasCards}>
            <Workflow size={13} aria-hidden="true" />
            Arrange
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
