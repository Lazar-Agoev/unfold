/**
 * A page's canvas (React Flow): cards as custom nodes at their reflowed positions, one memoized arrow
 * per opened call occurrence with its closed label, the page's drawn links, and a camera that pans
 * and zooms without changing what any card shows. No minimap. Large pages render only the cards in
 * view. While a link is being drawn, the next card clicked ends it; Escape or a click on the canvas
 * cancels. A selected line shows its contract; Escape or a click on the canvas closes it.
 */

import {
  ReactFlow,
  getViewportForBounds,
  useReactFlow,
  useStore,
  type NodeChange,
  type OnMove,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { Card, Link } from "../ipc/generated";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { copyLink } from "../agent";
import { useContextMenu } from "../components";
import { authorOf } from "../code/text";
import { useRegisterActions } from "../keyboard";
import {
  displayedRects,
  editPage,
  foldAll,
  getOpenBoard,
  getPageView,
  moveCards,
  removeLink,
  requestCardFocus,
  select,
  setCamera,
  setLinking,
  updatePageView,
  usePageView,
  useWorkingItems,
  type PageTarget,
} from "../state/boards";
import { ArrowEdge, type ArrowEdgeType } from "./ArrowEdge";
import type { BoardArrows } from "./useBoardArrows";
import { CardNode, type CardNodeType } from "./cards/CardNode";
import { TARGET_HANDLE } from "./cards/parts";
import { boundsOf, fallbackSize, reflow, type Rect } from "./layout";
import { LabelLayout, LabelLayoutContext } from "./lines/labelLayout";
import { useLineLabels } from "./lines/useLineLabels";
import { LinkEdge, type LinkEdgeType } from "./LinkEdge";
import { linkEnds } from "./linkSides";
import "./canvas.css";

type CanvasEdge = ArrowEdgeType | LinkEdgeType;

const nodeTypes = { card: CardNode };
const edgeTypes = { arrow: ArrowEdge, link: LinkEdge };
const NO_PORTS: readonly string[] = [];
const NO_LINKS: readonly Link[] = [];
/** Fit leaves room for the dock at the bottom, so nothing covers a card. */
const FIT_PADDING = { top: "32px", right: "40px", bottom: "96px", left: "40px" } as const;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 1.5;
/** Above this many cards, only the cards in view render. */
const VISIBLE_ONLY_ABOVE = 40;

/** React Flow error 008 (an edge whose port isn't measured yet) is expected while cards load. */
function onFlowError(code: string, message: string): void {
  if (code !== "008") {
    console.warn(message);
  }
}

export interface BoardCanvasProps {
  /** The page shown (a stable object while it is the same page). */
  readonly target: PageTarget;
  /** The page's cards (the document's list). */
  readonly cards: readonly Card[];
  /** The page's drawn links. */
  readonly links: readonly Link[] | undefined;
  /** The arrows and ports derived from the cards' models (`useBoardArrows`, computed by the tab). */
  readonly boardArrows: BoardArrows;
}

export function BoardCanvas({ target, cards, links = NO_LINKS, boardArrows }: BoardCanvasProps) {
  const flow = useReactFlow<CardNodeType, CanvasEdge>();
  const sizes = usePageView(target, (view) => view.sizes);
  const focus = usePageView(target, (view) => view.focus);
  const linking = usePageView(target, (view) => view.linking !== null);
  const selectedLink = usePageView(target, ({ selection }) => (selection.kind === "link" ? selection.linkId : null));
  const selectedLine = usePageView(target, ({ selection }) => (selection.kind === "line" ? selection.arrowId : null));
  const working = useWorkingItems(target);
  const [labelLayout] = useState(() => new LabelLayout());
  const [initialCamera] = useState(() => getPageView(target).camera);

  const { arrows, ports, settled } = boardArrows;
  const labels = useLineLabels(arrows);
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
      // An agent changed it and is still working: the node gets the soft outline.
      const className = working.has(card.id) ? "agent-working" : undefined;
      const node: CardNodeType =
        cached && keptPorts && cached.position.x === at.x && cached.position.y === at.y && cached.className === className
          ? cached
          : {
              id: card.id,
              type: "card",
              position: at,
              dragHandle: ".card-drag",
              ...(className !== undefined ? { className } : {}),
              data: { target, ports: keptPorts ?? cardPorts },
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
  }, [cards, display, drag, ports, target, nodeCache, working]);

  const [edgeCache] = useState(() => new Map<string, ArrowEdgeType>());
  const arrowEdges = useMemo(() => {
    const live = new Set<string>();
    const list = arrows.map((arrow) => {
      const cached = edgeCache.get(arrow.id);
      const handler = arrow.kind === "handler";
      const label = handler ? undefined : labels.get(arrow.id);
      const selected = arrow.id === selectedLine;
      const kept = cached?.data;
      const edge: ArrowEdgeType =
        cached !== undefined &&
        kept !== undefined &&
        kept.label === label &&
        kept.selected === selected &&
        kept.page === target &&
        (handler || (kept.line?.line === arrow.line && kept.line.project === arrow.project && kept.line.seq === arrow.seq))
          ? cached
          : {
              id: arrow.id,
              type: "arrow",
              source: arrow.source,
              sourceHandle: arrow.sourceHandle,
              target: arrow.target,
              targetHandle: TARGET_HANDLE,
              className: "board-arrow-edge",
              focusable: false,
              selectable: false,
              // A line without room for its label is itself a button (ArrowEdge), which an `img` would hide.
              ...(handler ? {} : { ariaRole: "group" as const }),
              data: {
                dashed: arrow.dashed,
                handler,
                selected,
                page: target,
                ...(label !== undefined ? { label } : {}),
                ...(handler
                  ? {}
                  : {
                      line: {
                        arrowId: arrow.id,
                        project: arrow.project,
                        source: arrow.sourceHandle,
                        seq: arrow.seq,
                        from: arrow.source,
                        to: arrow.target,
                        line: arrow.line,
                      },
                    }),
              },
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
  }, [arrows, edgeCache, labels, selectedLine, target]);

  // Links leave and enter by the sides facing each other, so they follow the cards as they move.
  const [linkCache] = useState(() => new Map<string, LinkEdgeType>());
  const linkEdges = useMemo(() => {
    const rectOf = (id: string): Rect | undefined => {
      const at = drag?.id === id ? drag : display.get(id);
      return at ? { x: at.x, y: at.y, ...sizeOf(id) } : undefined;
    };
    const live = new Set<string>();
    const list = links.flatMap((link): LinkEdgeType[] => {
      const from = rectOf(link.from);
      const to = rectOf(link.to);
      if (!from || !to) {
        return [];
      }
      const ends = linkEnds(from, to);
      const label = link.label?.text ?? null;
      const writer = link.label ? authorOf(link.label.author) : null;
      const author = writer?.name ?? null;
      const agent = writer?.agent ?? false;
      const busy = working.has(link.id);
      const selected = link.id === selectedLink;
      const cached = linkCache.get(link.id);
      const edge: LinkEdgeType =
        cached?.source === link.from &&
        cached.target === link.to &&
        cached.sourceHandle === `link-out-${ends.from}` &&
        cached.targetHandle === `link-in-${ends.to}` &&
        cached.data?.label === label &&
        cached.data.author === author &&
        cached.data.agent === agent &&
        cached.data.working === busy &&
        cached.data.selected === selected
          ? cached
          : {
              id: link.id,
              type: "link",
              source: link.from,
              sourceHandle: `link-out-${ends.from}`,
              target: link.to,
              targetHandle: `link-in-${ends.to}`,
              className: "board-link-edge",
              focusable: false,
              selectable: false,
              data: { target, selected, label, author, agent, working: busy },
            };
      linkCache.set(link.id, edge);
      live.add(link.id);
      return [edge];
    });
    for (const id of [...linkCache.keys()]) {
      if (!live.has(id)) {
        linkCache.delete(id);
      }
    }
    return list;
  }, [links, display, drag, sizeOf, selectedLink, target, linkCache, working]);
  const edges = useMemo((): CanvasEdge[] => [...arrowEdges, ...linkEdges], [arrowEdges, linkEdges]);

  // The cards as drawn, which the lines' labels keep clear of.
  const cardRects = useMemo(
    () =>
      cards.flatMap((card) => {
        const at = drag?.id === card.id ? drag : display.get(card.id);
        return at ? [{ x: at.x, y: at.y, ...sizeOf(card.id) }] : [];
      }),
    [cards, display, drag, sizeOf],
  );
  useLayoutEffect(() => {
    labelLayout.setCards(cardRects);
  }, [labelLayout, cardRects]);

  // Escape ends link drawing (before anything else hears it).
  useEffect(() => {
    if (!linking) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setLinking(target, null);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
    };
  }, [linking, target]);

  // Escape closes a selected line's contract (later registrations win, so it goes before the peek sheet's).
  useRegisterActions([
    {
      id: "board.closeLine",
      title: "Close the Line's Contract",
      category: "Board",
      shortcuts: ["Escape"],
      showInAdd: false,
      enabled: () => getPageView(target).selection.kind === "line",
      run: () => {
        select(target, { kind: "none" });
      },
    },
  ]);

  const onNodesChange = useCallback(
    (changes: NodeChange<CardNodeType>[]) => {
      const measured: Record<string, { width: number; height: number }> = {};
      for (const change of changes) {
        if (change.type === "dimensions" && change.dimensions) {
          const old = getPageView(target).sizes[change.id];
          if (!old || Math.abs(old.width - change.dimensions.width) > 0.5 || Math.abs(old.height - change.dimensions.height) > 0.5) {
            measured[change.id] = { width: change.dimensions.width, height: change.dimensions.height };
          }
        } else if (change.type === "position" && change.position && change.dragging === true) {
          setDrag({ id: change.id, x: change.position.x, y: change.position.y });
        }
      }
      if (Object.keys(measured).length > 0) {
        updatePageView(target, (view) => {
          Object.assign(view.sizes, measured);
        });
      }
    },
    [target],
  );

  const onNodeDragStop = useCallback(
    (_event: unknown, node: CardNodeType) => {
      editPage(target, (page) => {
        moveCards(page, { [node.id]: [node.position.x, node.position.y] });
      });
      setDrag(null);
    },
    [target],
  );

  const onMoveEnd: OnMove = useCallback(
    (_event, viewport) => {
      setCamera(target, viewport);
    },
    [target],
  );

  const width = useStore((state) => state.width);
  const height = useStore((state) => state.height);

  // Focus requests: once the cards they name have loaded and been measured, center or fit them.
  const handled = useRef<number | null>(null);
  useEffect(() => {
    if (!focus || width === 0 || height === 0 || handled.current === focus.seq) {
      return undefined;
    }
    // Fit without names waits for every card, so a board opened in a new window fits what it shows.
    const measuredIds = focus.cardIds.length > 0 ? focus.cardIds : cards.map((card) => card.id);
    const ready = settled && measuredIds.every((id) => sizes[id] !== undefined);
    const run = () => {
      const board = getOpenBoard(target.board);
      if (!board || handled.current === focus.seq) {
        return;
      }
      handled.current = focus.seq;
      const rects = displayedRects(board, target.page);
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
  }, [focus, width, height, settled, sizes, target, flow, cards]);

  return (
    <LabelLayoutContext.Provider value={labelLayout}>
      <ReactFlow<CardNodeType, CanvasEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={onNodesChange}
        onNodeDragStop={onNodeDragStop}
        onMoveEnd={onMoveEnd}
        onPaneClick={() => {
          if (getPageView(target).linking) {
            setLinking(target, null);
            return;
          }
          select(target, { kind: "none" });
        }}
        onEdgeClick={(_event, edge) => {
          if (edge.type === "link") {
            select(target, { kind: "link", linkId: edge.id });
          }
        }}
        onEdgeContextMenu={(event, edge) => {
          if (edge.type !== "link") {
            return;
          }
          event.preventDefault();
          select(target, { kind: "link", linkId: edge.id });
          paneMenu.open({ x: event.clientX, y: event.clientY }, [
            { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, edge.id) },
            { kind: "separator", id: "s1" },
            { id: "remove", label: "Remove link", danger: true, onSelect: () => { removeLink(target, edge.id); } },
          ]);
        }}
        onPaneContextMenu={(event) => {
          event.preventDefault();
          const hasCards = cards.length > 0;
          paneMenu.open({ x: event.clientX, y: event.clientY }, [
            { id: "fit", label: "Fit", shortcut: "Shift+1", disabled: !hasCards, onSelect: () => { requestCardFocus(target, [], "fit"); } },
            { kind: "separator", id: "s1" },
            { id: "fold", label: "Fold all cards", disabled: !hasCards, onSelect: () => { foldAll(target, true); } },
            { id: "unfold", label: "Unfold all cards", disabled: !hasCards, onSelect: () => { foldAll(target, false); } },
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
        className={`board-canvas${linking ? " linking" : ""}`}
      />
      {paneMenu.element}
    </LabelLayoutContext.Provider>
  );
}
