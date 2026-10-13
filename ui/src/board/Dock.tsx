/**
 * The floating dock at the bottom of the canvas, centered on the uncovered canvas: Add, Arrange, and
 * the camera (−, the zoom, +, Fit). On a narrow canvas it sheds parts (board.css, by the lane's
 * width): Arrange's label first, then Arrange; the zoom controls always stay.
 */

import { useStore } from "@xyflow/react";
import { Button, IconButton } from "../components";
import { openAdd } from "../state/overlays";

export interface DockProps {
  /** Whether the page has cards or resource cards to arrange and fit. */
  readonly hasItems: boolean;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onFit: () => void;
  readonly onArrange: () => void;
}

/** A rule between two groups; `part` names the group after it, which the rule goes away with. */
function Divider({ part }: { readonly part?: "arrange" }) {
  return <span className={part !== undefined ? `board-dock-divider before-${part}` : "board-dock-divider"} aria-hidden="true" />;
}

export function Dock({ hasItems, onZoomIn, onZoomOut, onFit, onArrange }: DockProps) {
  const zoom = useStore((state) => state.transform[2]);
  return (
    <div className="board-dock-lane">
      <div className="board-dock" role="toolbar" aria-label="Board">
        <Button
          variant="primary"
          icon="plus"
          className="board-dock-add"
          onClick={() => {
            openAdd();
          }}
        >
          Add
        </Button>
        <Divider part="arrange" />
        <Button variant="ghost" icon="arrange" className="board-dock-arrange" aria-label="Arrange" disabled={!hasItems} title="Arrange by call depth: callers left, callees right" onClick={onArrange}>
          Arrange
        </Button>
        <Divider />
        <div className="board-dock-group" role="group" aria-label="Zoom">
          <IconButton icon="minus" label="Zoom out" tooltipSide="top" onClick={onZoomOut} />
          <span className="board-dock-zoom" aria-live="polite">
            {Math.round(zoom * 100)}%
          </span>
          <IconButton icon="plus" label="Zoom in" tooltipSide="top" onClick={onZoomIn} />
          <Button variant="ghost" disabled={!hasItems} onClick={onFit}>
            Fit
          </Button>
        </div>
      </div>
    </div>
  );
}
