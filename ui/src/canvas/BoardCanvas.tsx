/**
 * The board's canvas (React Flow): cards as custom nodes at their reflowed positions, one memoized
 * arrow per opened call occurrence, and a camera that pans and zooms without changing what any card
 * shows. No minimap and nothing over the canvas (architecture §10.7). Large boards render only the
 * cards in view.
 */

import {
  MarkerType,
  ReactFlow,
  getViewportForBounds,
  useReactFlow,
  useStore,
  type NodeChange,
  type OnMove,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { Card } from "../ipc/generated";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useContextMenu } from "../components";
import { displayedRects, foldAllCards } from "../state/boards/api";
import { moveCards } from "../state/boards/document";
import { editBoard, getBoard, requestFocus, setCamera, updateView, useBoards } from "../state/boards/store";
import { ArrowEdge, type ArrowEdgeType } from "./ArrowEdge";
import type { BoardArrows } from "./useBoardArrows";
import { CardNode, type CardNodeType } from "./cards/CardNode";
import { TARGET_HANDLE } from "./cards/parts";
import { boundsOf, fallbackSize, reflow, type Rect } from "./layout";
import { usePendingPlacement } from "./placement";
import "./canvas.css";

const nodeTypes = { card: CardNode };
const edgeTypes = { arrow: ArrowEdge };
const EMPTY_SIZES: Readonly<Record<string, { width: number; height: number }>> = {};
const NO_PORTS: readonly string[] = [];
/** Fit leaves room for the bottom bar, so nothing covers a card. */
export const FIT_PADDING = { top: "32px", right: "40px", bottom: "72px", left: "40px" } as const;
export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 1.5;
/** Above this many cards, only the cards in view render. */
const VISIBLE_ONLY_ABOVE = 40;
const ARROW_MARKER = { type: MarkerType.ArrowClosed, width: 16, height: 16, color: "var(--color-accent)" } as const;

/** React Flow error 008 (an edge whose port isn't measured yet) is expected while cards load. */
function onFlowError(code: string, message: string): void {
  if (code !== "008") {
    console.warn(message);
  }
}

export interface BoardCanvasProps {
  readonly boardId: string;
  readonly active: boolean;
  /** The board's cards (the document's list). */
  readonly cards: readonly Card[];
  /** The arrows and ports derived from the cards' models (`useBoardArrows`, computed by the tab). */
  readonly boardArrows: BoardArrows;
}

export function BoardCanvas({ boardId, active, cards, boardArrows }: BoardCanvasProps) {
  const flow = useReactFlow<CardNodeType, ArrowEdgeType>();
  const sizes = useBoards((state) => state.boards[boardId]?.view.sizes ?? EMPTY_SIZES);
  const focus = useBoards((state) => state.boards[boardId]?.view.focus ?? null);
  const [initialCamera] = useState(() => getBoard(boardId)?.view.camera ?? null);

  const { arrows, ports, settled } = boardArrows;
  usePendingPlacement(boardId, active);
  const paneMenu = useContextMenu("Board actions");

  // Positions: saved positions reflowed by measured sizes; a dragged card follows the pointer.
  const [drag, setDrag] = useState<{ id: string; x: number; y: number } | null>(null);
  const sizeOf = useCallback(
    (id: string) => {
      const measured = sizes[id];
      if (measured) {
        return measured;
      }
      const card = cards.find((item) => item.id === id);
      return fallbackSize(card !== undefined && "entrance" in card.ref);
    },
    [sizes, cards],
  );
  const display = useMemo(() => reflow(cards, sizeOf, drag?.id), [cards, sizeOf, drag?.id]);

  // Node objects keep their identity while nothing about them changes, so React Flow skips them.
  const [nodeCache] = useState(() => new Map<string, CardNodeType>());
  const nodes = useMemo(() => {
    const live = new Set<string>();
    const list = cards.map((card) => {
      const at = drag?.id === card.id ? { x: drag.x, y: drag.y } : (display.get(card.id) ?? { x: card.at[0], y: card.at[1] });
      const cardPorts = ports.get(card.id) ?? NO_PORTS;
      const cached = nodeCache.get(card.id);
      const keptPorts = cached?.data.ports.join("\u0000") === cardPorts.join("\u0000") ? cached.data.ports : undefined;
      const node: CardNodeType =
        cached && keptPorts && cached.position.x === at.x && cached.position.y === at.y
          ? cached
          : {
              id: card.id,
              type: "card",
              position: at,
              dragHandle: ".card-drag",
              data: { boardId, ports: keptPorts ?? cardPorts },
            };
      nodeCache.set(card.id, node);
      live.add(card.id);
      return node;
    });
    for (const id of [...nodeCache.keys()]) {
      if (!live.has(id)) {
        nodeCache.delete(id);
      }
    }
    return list;
  }, [cards, display, drag, ports, boardId, nodeCache]);

  const [edgeCache] = useState(() => new Map<string, ArrowEdgeType>());
  const edges = useMemo(() => {
    const live = new Set<string>();
    const list = arrows.map((arrow) => {
      const cached = edgeCache.get(arrow.id);
      const edge: ArrowEdgeType = cached ?? {
        id: arrow.id,
        type: "arrow",
        source: arrow.source,
        sourceHandle: arrow.sourceHandle,
        target: arrow.target,
        targetHandle: TARGET_HANDLE,
        markerEnd: ARROW_MARKER,
        focusable: false,
        selectable: false,
        data: { dashed: arrow.dashed, label: arrow.kind },
      };
      edgeCache.set(arrow.id, edge);
      live.add(arrow.id);
      return edge;
    });
    for (const id of [...edgeCache.keys()]) {
      if (!live.has(id)) {
        edgeCache.delete(id);
      }
    }
    return list;
  }, [arrows, edgeCache]);

  const onNodesChange = useCallback(
    (changes: NodeChange<CardNodeType>[]) => {
      const measured: Record<string, { width: number; height: number }> = {};
      for (const change of changes) {
        if (change.type === "dimensions" && change.dimensions) {
          const old = getBoard(boardId)?.view.sizes[change.id];
          if (!old || Math.abs(old.width - change.dimensions.width) > 0.5 || Math.abs(old.height - change.dimensions.height) > 0.5) {
            measured[change.id] = { width: change.dimensions.width, height: change.dimensions.height };
          }
        } else if (change.type === "position" && change.position && change.dragging === true) {
          setDrag({ id: change.id, x: change.position.x, y: change.position.y });
        }
      }
      if (Object.keys(measured).length > 0) {
        updateView(boardId, (view) => {
          Object.assign(view.sizes, measured);
        });
      }
    },
    [boardId],
  );

  const onNodeDragStop = useCallback(
    (_event: unknown, node: CardNodeType) => {
      editBoard(boardId, (document) => {
        moveCards(document, { [node.id]: [node.position.x, node.position.y] });
      });
      setDrag(null);
    },
    [boardId],
  );

  const onMoveEnd: OnMove = useCallback(
    (_event, viewport) => {
      setCamera(boardId, viewport);
    },
    [boardId],
  );

  const width = useStore((state) => state.width);
  const height = useStore((state) => state.height);

  // Focus requests: once the cards they name have loaded and been measured, center or fit them.
  const handled = useRef<number | null>(null);
  useEffect(() => {
    if (!focus || !active || width === 0 || height === 0 || handled.current === focus.seq) {
      return undefined;
    }
    // Fit without names waits for every card, so a board opened in a new window fits what it shows.
    const measuredIds = focus.cardIds.length > 0 ? focus.cardIds : cards.map((card) => card.id);
    const ready = settled && measuredIds.every((id) => sizes[id] !== undefined);
    const run = () => {
      const board = getBoard(boardId);
      if (!board || handled.current === focus.seq) {
        return;
      }
      handled.current = focus.seq;
      const rects = displayedRects(board);
      const wanted = focus.cardIds.length > 0 ? focus.cardIds.map((id) => rects.get(id)).filter((rect): rect is Rect => rect !== undefined) : [...rects.values()];
      if (focus.mode === "fit" || wanted.length !== 1) {
        const bounds = boundsOf(wanted);
        if (bounds) {
          const viewport = getViewportForBounds(bounds, width, height, MIN_ZOOM, 1, FIT_PADDING);
          void flow.setViewport(viewport, { duration: 240 });
        }
        return;
      }
      const rect = wanted[0];
      if (rect) {
        const zoom = Math.min(1, Math.max(0.6, flow.getZoom()));
        const visibleHeight = height / zoom;
        void flow.setCenter(rect.x + rect.width / 2, rect.y + Math.min(rect.height, visibleHeight - 96) / 2, { zoom, duration: 240 });
      }
    };
    const timer = setTimeout(run, ready ? 80 : 1500);
    return () => {
      clearTimeout(timer);
    };
  }, [focus, active, width, height, settled, sizes, boardId, flow, cards]);

  return (
    <>
    <ReactFlow<CardNodeType, ArrowEdgeType>
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      onNodesChange={onNodesChange}
      onNodeDragStop={onNodeDragStop}
      onMoveEnd={onMoveEnd}
      onPaneClick={() => {
        updateView(boardId, (view) => {
          view.selectedCardId = null;
          view.selectedKey = null;
        });
      }}
      onPaneContextMenu={(event) => {
        event.preventDefault();
        const hasCards = cards.length > 0;
        paneMenu.open({ x: event.clientX, y: event.clientY }, [
          { id: "fit", label: "Fit Board", shortcut: "Shift+1", disabled: !hasCards, onSelect: () => { requestFocus(boardId, [], "fit"); } },
          { kind: "separator", id: "s1" },
          { id: "fold", label: "Fold All Cards", disabled: !hasCards, onSelect: () => { foldAllCards(boardId, true); } },
          { id: "unfold", label: "Unfold All Cards", disabled: !hasCards, onSelect: () => { foldAllCards(boardId, false); } },
        ]);
      }}
      onError={onFlowError}
      defaultViewport={initialCamera ?? { x: 40, y: 40, zoom: 1 }}
      minZoom={MIN_ZOOM}
      maxZoom={MAX_ZOOM}
      onlyRenderVisibleElements={cards.length > VISIBLE_ONLY_ABOVE}
      nodesConnectable={false}
      elementsSelectable={false}
      edgesFocusable={false}
      nodesFocusable={false}
      deleteKeyCode={null}
      selectionKeyCode={null}
      multiSelectionKeyCode={null}
      disableKeyboardA11y
      zoomOnDoubleClick={false}
      panOnScroll
      proOptions={{ hideAttribution: true }}
      aria-label="Board canvas"
      className="board-canvas"
    />
    {paneMenu.element}
    </>
  );
}
