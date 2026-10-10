/**
 * The floating dock at the bottom of the canvas (components §6), centered on the uncovered canvas
 * (V-38): Add, Back and Forward, the trail of focus moves ("POST /orders › createOrder"), Arrange, and
 * the camera (−, the zoom, +, Fit). It replaces V1's board bar.
 */

import { useStore } from "@xyflow/react";
import { Button, IconButton, KeyChip } from "../components";
import { Icon } from "../icons";
import { ActionIds, useShortcut } from "../keyboard";
import { focusCard, pushFocusMove, usePageView, type PageTarget } from "../state/boards";
import { openAdd } from "../state/overlays";

export interface DockProps {
  readonly target: PageTarget;
  readonly hasCards: boolean;
  readonly onTravel: (direction: "back" | "forward") => void;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onFit: () => void;
  readonly onArrange: () => void;
}

/** A shortcut prop for a control, only when the action has one. */
function shortcutOf(shortcut: string | null): { shortcut?: string } {
  return shortcut !== null ? { shortcut } : {};
}

function Divider() {
  return <span className="board-dock-divider" aria-hidden="true" />;
}

export function Dock({ target, hasCards, onTravel, onZoomIn, onZoomOut, onFit, onArrange }: DockProps) {
  const trail = usePageView(target, (view) => view.trail);
  const canGoBack = usePageView(target, (view) => view.back.length > 0);
  const canGoForward = usePageView(target, (view) => view.forward.length > 0);
  const zoom = useStore((state) => state.transform[2]);
  const addKey = useShortcut(ActionIds.add);
  const backKey = useShortcut(ActionIds.back);
  const forwardKey = useShortcut(ActionIds.forward);
  const zoomInKey = useShortcut(ActionIds.zoomIn);
  const zoomOutKey = useShortcut(ActionIds.zoomOut);
  return (
    <div className="board-dock" role="toolbar" aria-label="Board">
      <Button
        variant="primary"
        icon="plus"
        className="board-dock-add"
        aria-label="Add"
        aria-keyshortcuts={addKey ?? "A"}
        onClick={() => {
          openAdd();
        }}
      >
        Add
        <KeyChip variant="filled" shortcut={addKey ?? "A"} />
      </Button>
      <Divider />
      <div className="board-dock-group">
        <IconButton icon="chevron-left" label="Back" {...shortcutOf(backKey)} tooltipSide="top" disabled={!canGoBack} onClick={() => { onTravel("back"); }} />
        <IconButton icon="chevron-right" label="Forward" {...shortcutOf(forwardKey)} tooltipSide="top" disabled={!canGoForward} onClick={() => { onTravel("forward"); }} />
      </div>
      {trail.length > 0 ? (
        <>
          <Divider />
          <nav className="board-trail" aria-label="Trail">
            <ol>
              {trail.map((item, index) => (
                <li key={`${item.cardId}-${index}`} className={index === trail.length - 1 ? "current" : undefined}>
                  {index > 0 ? <Icon name="chevron-right" size={12} className="board-trail-separator" /> : null}
                  <button
                    type="button"
                    aria-current={index === trail.length - 1 ? "step" : undefined}
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
        </>
      ) : null}
      <Divider />
      <Button variant="ghost" icon="arrange" disabled={!hasCards} title="Arrange by call depth: callers left, callees right" onClick={onArrange}>
        Arrange
      </Button>
      <Divider />
      <div className="board-dock-group" role="group" aria-label="Zoom">
        <IconButton icon="minus" label="Zoom out" {...shortcutOf(zoomOutKey)} tooltipSide="top" onClick={onZoomOut} />
        <span className="board-dock-zoom" aria-live="polite">
          {Math.round(zoom * 100)}%
        </span>
        <IconButton icon="plus" label="Zoom in" {...shortcutOf(zoomInKey)} tooltipSide="top" onClick={onZoomIn} />
        <Button variant="ghost" disabled={!hasCards} onClick={onFit}>
          Fit
        </Button>
      </div>
    </div>
  );
}
