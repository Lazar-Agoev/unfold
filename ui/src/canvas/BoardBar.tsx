/**
 * The bar at the bottom of a board: Back, the breadcrumb trail of focus moves ("POST /orders ›
 * createOrder"), and the camera controls (− + Fit) with Arrange by call depth.
 */

import { ArrowLeft, Maximize, Minus, Plus, Workflow } from "lucide-react";
import { IconButton, Tooltip } from "../components";
import { ActionIds, useShortcut } from "../keyboard";
import { focusCard } from "../state/boards/api";
import { pushFocusMove, useBoards } from "../state/boards/store";
import type { TrailItem } from "../state/boards/types";

const NO_TRAIL: readonly TrailItem[] = [];

export interface BoardBarProps {
  readonly boardId: string;
  readonly onBack: () => void;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onFit: () => void;
  readonly onArrange: () => void;
}

export function BoardBar({ boardId, onBack, onZoomIn, onZoomOut, onFit, onArrange }: BoardBarProps) {
  const trail = useBoards((state) => state.boards[boardId]?.view.trail ?? NO_TRAIL);
  const canGoBack = useBoards((state) => (state.boards[boardId]?.view.back.length ?? 0) > 0);
  const hasCards = useBoards((state) => (state.boards[boardId]?.document.cards.length ?? 0) > 0);
  const backKey = useShortcut(ActionIds.back);
  const fitKey = useShortcut(ActionIds.fitBoard);
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
                    pushFocusMove(boardId);
                    focusCard(boardId, item.cardId);
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
        <IconButton icon={Minus} label="Zoom Out" {...(zoomOutKey !== null ? { shortcut: zoomOutKey } : {})} size="small" tooltipSide="top" onClick={onZoomOut} />
        <IconButton icon={Plus} label="Zoom In" {...(zoomInKey !== null ? { shortcut: zoomInKey } : {})} size="small" tooltipSide="top" onClick={onZoomIn} />
        <IconButton icon={Maximize} label="Fit Board" {...(fitKey !== null ? { shortcut: fitKey } : {})} size="small" tooltipSide="top" onClick={onFit} disabled={!hasCards} />
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
