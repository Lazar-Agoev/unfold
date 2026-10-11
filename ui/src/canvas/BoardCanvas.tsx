/**
 * A page's canvas (React Flow): cards as custom nodes at their reflowed positions, frames as nodes
 * behind what they hold (`pageGeometry`), one memoized arrow per opened call occurrence with its closed
 * label, the page's drawn links, and a camera that pans and zooms without changing what any card
 * shows. No minimap. Large pages render only the cards in view. While a link is being drawn, the next
 * card or frame clicked ends it; Escape or a click on the canvas cancels. A selected line shows its
 * contract; Escape or a click on the canvas closes it. A card in Diagram view sends one arrow to each
 * card it calls; a type card, one from each field to each type it names on the page. A collapsed
 * frame is one node: what it holds isn't drawn, and their arrows and links attach to it, merged
 * (`routeArrows`, `routeLinks`). Dragging a frame moves everything in it, as one undo step.
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
import type { Card, Frame, Link, Resource } from "../ipc/generated";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { copyLink } from "../agent";
import { useContextMenu } from "../components";
import { authorOf } from "../code/text";
import { useRegisterActions } from "../keyboard";
import {
  MAX_FRAME_DEPTH,
  displayedRects,
  editPage,
  findFrame,
  findPage,
  foldAll,
  getOpenBoard,
  getPageView,
  moveFrame,
  moveItems,
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
import { portsByCard, type Arrow } from "./arrows";
import type { BoardArrows } from "./useBoardArrows";
import { CardNode, type CardNodeType } from "./cards/CardNode";
import { TARGET_HANDLE } from "./cards/parts";
import { FrameNode, type FrameNodeType } from "./frames/FrameNode";
import { collapsedPlaces, fallbackItemSize, pageGeometry, type Drag } from "./frames/geometry";
import { routeArrows, routeLinks } from "./frames/routes";
import { boundsOf, type Rect } from "./layout";
import { LabelLayout, LabelLayoutContext } from "./lines/labelLayout";
import { useLineLabels } from "./lines/useLineLabels";
import { LinkEdge, type LinkEdgeType } from "./LinkEdge";
import { linkEnds } from "./linkSides";
import "./canvas.css";

type CanvasNode = CardNodeType | FrameNodeType;
type CanvasEdge = ArrowEdgeType | LinkEdgeType;

const nodeTypes = { card: CardNode, frame: FrameNode };
const edgeTypes = { arrow: ArrowEdge, link: LinkEdge };
const NO_PORTS: readonly string[] = [];
const NO_LINKS: readonly Link[] = [];
const NO_FRAMES: readonly Frame[] = [];
const NO_RESOURCES: readonly Resource[] = [];
/** Fit leaves room for the dock at the bottom, so nothing covers a card. */
const FIT_PADDING = { top: "32px", right: "40px", bottom: "96px", left: "40px" } as const;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 1.5;
/** Above this many cards, only the cards in view render. */
const VISIBLE_ONLY_ABOVE = 40;

/** Everything about an arrow its edge draws, as one string. */
function arrowSignature(arrow: Arrow): string {
  const calls = arrow.calls?.map((call) => `${call.source}\u0001${call.seq}\u0001${call.line}`).join("\u0002") ?? "";
  const merged = arrow.merged ? `${arrow.merged.calls}/${arrow.merged.fields}` : "";
  return [arrow.source, arrow.sourceHandle, arrow.target, arrow.project, arrow.occurrence, arrow.seq, arrow.line ?? "", arrow.kind, arrow.dashed, calls, arrow.type?.text ?? "", merged].join("\u0000");
}

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
  /** The page's frames. */
  readonly frames?: readonly Frame[] | undefined;
  /** The page's resource cards (frames hold them). */
  readonly resources?: readonly Resource[] | undefined;
  /** The page's drawn links. */
  readonly links: readonly Link[] | undefined;
  /** The arrows and ports derived from the cards' models (`useBoardArrows`, computed by the tab). */
  readonly boardArrows: BoardArrows;
}

export function BoardCanvas({ target, cards, frames = NO_FRAMES, resources = NO_RESOURCES, links = NO_LINKS, boardArrows }: BoardCanvasProps) {
  const flow = useReactFlow<CanvasNode, CanvasEdge>();
  const sizes = usePageView(target, (view) => view.sizes);
  const focus = usePageView(target, (view) => view.focus);
  const linking = usePageView(target, (view) => view.linking !== null);
  const selectedLink = usePageView(target, ({ selection }) => (selection.kind === "link" ? selection.linkId : null));
  const selectedLine = usePageView(target, ({ selection }) => (selection.kind === "line" ? selection : null));
  const listedCalls = usePageView(target, ({ selection }) => (selection.kind === "calls" ? selection.arrowId : null));
  const working = useWorkingItems(target);
  const [labelLayout] = useState(() => new LabelLayout());
  const [initialCamera] = useState(() => getPageView(target).camera);

  const { bodies, settled } = boardArrows;
  const framed = useMemo(() => ({ cards, frames, resources }), [cards, frames, resources]);
  // What a collapsed frame hides is drawn as the frame: its arrows and links attach there.
  const shownAs = useMemo(() => collapsedPlaces(framed), [framed]);
  const arrows = useMemo(() => routeArrows(boardArrows.arrows, shownAs), [boardArrows.arrows, shownAs]);
  const ports = useMemo(() => portsByCard(arrows), [arrows]);
  const labels = useLineLabels(arrows);
  const paneMenu = useContextMenu("Board actions");

  // Positions: saved positions reflowed by measured sizes, frames around what they hold; a dragged
  // card or frame follows the pointer.
  const [drag, setDrag] = useState<Drag | null>(null);
  const sizeOf = useCallback((id: string) => sizes[id] ?? fallbackItemSize(framed, id, bodies.get(id)), [sizes, framed, bodies]);
  const geometry = useMemo(() => pageGeometry(framed, sizeOf, drag), [framed, sizeOf, drag]);
  const display = geometry.rects;
  // An expanded frame's size is its box's, never measured: only collapsed frames' nodes are.
  const boxes = useMemo(() => new Set(geometry.frames.flatMap(({ frame, collapsed }) => (collapsed ? [] : [frame.id]))), [geometry]);

  // Node objects keep their identity while nothing about them changes, so React Flow skips them.
  const [frameCache] = useState(() => new Map<string, FrameNodeType>());
  const frameNodes = useMemo(() => {
    const live = new Set<string>();
    const list = geometry.frames.map(({ frame, rect, depth, collapsed }) => {
      const framePorts = ports.get(frame.id) ?? NO_PORTS;
      const cached = frameCache.get(frame.id);
      const className = working.has(frame.id) ? "agent-working" : undefined;
      const box = collapsed ? undefined : { width: rect.width, height: rect.height };
      const node: FrameNodeType =
        cached?.position.x === rect.x &&
        cached.position.y === rect.y &&
        cached.className === className &&
        cached.data.box?.width === box?.width &&
        cached.data.box?.height === box?.height &&
        cached.data.ports.join("\u0000") === framePorts.join("\u0000")
          ? cached
          : {
              id: frame.id,
              type: "frame",
              position: { x: rect.x, y: rect.y },
              dragHandle: ".frame-drag",
              // Behind the lines and cards, outer frames behind inner ones; a collapsed frame is a node like a card.
              zIndex: collapsed ? 0 : depth - MAX_FRAME_DEPTH,
              ...(className !== undefined ? { className } : {}),
              data: { target, ports: framePorts, ...(box ? { box } : {}) },
            };
      frameCache.set(frame.id, node);
      live.add(frame.id);
      return node;
    });
    for (const id of [...frameCache.keys()]) {
      if (!live.has(id)) {
        frameCache.delete(id);
      }
    }
    return list;
  }, [geometry, ports, target, frameCache, working]);

  const [nodeCache] = useState(() => new Map<string, CardNodeType>());
  const cardNodes = useMemo(() => {
    const live = new Set<string>();
    const list = cards.flatMap((card) => {
      const at = display.get(card.id);
      if (!at) {
        return [];
      }
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
      return [node];
    });
    for (const id of [...nodeCache.keys()]) {
      if (!live.has(id)) {
        nodeCache.delete(id);
      }
    }
    return list;
  }, [cards, display, ports, target, nodeCache, working]);
  const nodes = useMemo((): CanvasNode[] => [...frameNodes, ...cardNodes], [frameNodes, cardNodes]);

  const [edgeCache] = useState(() => new Map<string, { readonly signature: string; readonly edge: ArrowEdgeType }>());
  const arrowEdges = useMemo(() => {
    const live = new Set<string>();
    const list = arrows.map((arrow) => {
      const handler = arrow.kind === "handler";
      const label = handler || arrow.kind === "type" || arrow.calls || arrow.merged ? undefined : labels.get(arrow.id);
      const selected = arrow.id === selectedLine?.arrowId;
      const listed = arrow.id === listedCalls;
      // What the edge draws, so an unchanged arrow keeps its edge (and its label doesn't render again).
      const signature = [
        arrowSignature(arrow),
        selected ? `${selectedLine.source}\u0001${selectedLine.seq}` : "",
        String(listed),
      ].join("\u0000");
      const cached = edgeCache.get(arrow.id);
      const kept = cached?.edge.data;
      if (cached?.signature === signature && kept !== undefined && kept.label === label && kept.page === target) {
        live.add(arrow.id);
        return cached.edge;
      }
      const ends = arrow.cards ?? { from: arrow.source, to: arrow.target };
      const line = { arrowId: arrow.id, project: arrow.project, source: arrow.occurrence, seq: arrow.seq, from: ends.from, to: ends.to, line: arrow.line };
      const edge: ArrowEdgeType = {
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
        ...(handler || arrow.kind === "type" || arrow.merged ? {} : { ariaRole: "group" as const }),
        data: {
          dashed: arrow.dashed,
          handler,
          selected,
          page: target,
          ...(label !== undefined ? { label } : {}),
          ...(arrow.type ? { type: arrow.type } : {}),
          ...(arrow.merged ? { merged: arrow.merged } : {}),
          ...(arrow.calls
            ? {
                calls: {
                  arrowId: arrow.id,
                  project: arrow.project,
                  from: ends.from,
                  to: ends.to,
                  calls: arrow.calls.map(({ source, seq, line: at }) => ({ source, seq, line: at })),
                },
                listed,
                ...(selected ? { line: selectedLine } : {}),
              }
            : handler || arrow.kind === "type" || arrow.merged
              ? {}
              : { line }),
        },
      };
      edgeCache.set(arrow.id, { signature, edge });
      live.add(arrow.id);
      return edge;
    });
    for (const id of [...edgeCache.keys()]) {
      if (!live.has(id)) {
        edgeCache.delete(id);
      }
    }
    return list;
  }, [arrows, edgeCache, labels, selectedLine, listedCalls, target]);

  // Links leave and enter by the sides facing each other, so they follow the cards as they move. Links
  // that join at a collapsed frame merge into one that counts them.
  const [linkCache] = useState(() => new Map<string, LinkEdgeType>());
  const linkEdges = useMemo(() => {
    const live = new Set<string>();
    const list = routeLinks(links, shownAs).flatMap(({ id, from: start, to: end, link, count }): LinkEdgeType[] => {
      const from = display.get(start);
      const to = display.get(end);
      if (!from || !to) {
        return [];
      }
      const ends = linkEnds(from, to);
      const label = link ? (link.label?.text ?? null) : `${count} links`;
      const writer = link?.label ? authorOf(link.label.author) : null;
      const author = writer?.name ?? null;
      const agent = writer?.agent ?? false;
      const busy = working.has(id);
      const selected = id === selectedLink;
      const cached = linkCache.get(id);
      const edge: LinkEdgeType =
        cached?.source === start &&
        cached.target === end &&
        cached.sourceHandle === `link-out-${ends.from}` &&
        cached.targetHandle === `link-in-${ends.to}` &&
        cached.data?.label === label &&
        cached.data.author === author &&
        cached.data.agent === agent &&
        cached.data.working === busy &&
        cached.data.selected === selected
          ? cached
          : {
              id,
              type: "link",
              source: start,
              sourceHandle: `link-out-${ends.from}`,
              target: end,
              targetHandle: `link-in-${ends.to}`,
              className: "board-link-edge",
              focusable: false,
              selectable: false,
              data: { target, selected, label, author, agent, working: busy },
            };
      linkCache.set(id, edge);
      live.add(id);
      return [edge];
    });
    for (const id of [...linkCache.keys()]) {
      if (!live.has(id)) {
        linkCache.delete(id);
      }
    }
    return list;
  }, [links, shownAs, display, selectedLink, target, linkCache, working]);
  const edges = useMemo((): CanvasEdge[] => [...arrowEdges, ...linkEdges], [arrowEdges, linkEdges]);

  // The cards (and collapsed frames) as drawn, which the lines' labels keep clear of.
  const cardRects = useMemo(
    () => [...display].flatMap(([id, rect]) => (boxes.has(id) ? [] : [rect])),
    [display, boxes],
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

  // Escape closes a selected line's contract, or lets go of an arrow's calls (later registrations win,
  // so it goes before the peek sheet's).
  useRegisterActions([
    {
      id: "board.closeLine",
      title: "Close the Line's Contract",
      category: "Board",
      shortcuts: ["Escape"],
      showInAdd: false,
      enabled: () => ["line", "calls"].includes(getPageView(target).selection.kind),
      run: () => {
        select(target, { kind: "none" });
      },
    },
  ]);

  const onNodesChange = useCallback(
    (changes: NodeChange<CanvasNode>[]) => {
      const measured: Record<string, { width: number; height: number }> = {};
      const board = getOpenBoard(target.board);
      const page = board ? findPage(board.document, target.page) : undefined;
      const box = (id: string) => {
        const frame = page ? findFrame(page, id) : undefined;
        return frame !== undefined && frame.collapsed !== true;
      };
      for (const change of changes) {
        if (change.type === "dimensions" && change.dimensions && !box(change.id)) {
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
    (_event: unknown, node: CanvasNode) => {
      if (node.type === "frame") {
        // A frame moves everything in it by how far its box moved.
        const from = pageGeometry(framed, sizeOf).rects.get(node.id);
        const dx = from ? Math.round(node.position.x - from.x) : 0;
        const dy = from ? Math.round(node.position.y - from.y) : 0;
        if (dx !== 0 || dy !== 0) {
          editPage(target, (page) => {
            moveFrame(page, node.id, dx, dy);
          });
        }
      } else {
        editPage(target, (page) => {
          moveItems(page, { [node.id]: [node.position.x, node.position.y] });
        });
      }
      setDrag(null);
    },
    [target, framed, sizeOf],
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
    // Fit without names waits for everything drawn, so a board opened in a new window fits what it
    // shows; a frame's box waits for what it draws.
    const drawn = (id: string) => geometry.inside.get(id) ?? [geometry.shownAs.get(id) ?? id];
    const measuredIds = focus.cardIds.length > 0 ? focus.cardIds.flatMap(drawn) : [...display.keys()].filter((id) => !boxes.has(id));
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
  }, [focus, width, height, settled, sizes, target, flow, geometry, display, boxes]);

  return (
    <LabelLayoutContext.Provider value={labelLayout}>
      <ReactFlow<CanvasNode, CanvasEdge>
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
          if (edge.type === "link" && links.some((link) => link.id === edge.id)) {
            select(target, { kind: "link", linkId: edge.id });
          }
        }}
        onEdgeContextMenu={(event, edge) => {
          if (edge.type !== "link" || !links.some((link) => link.id === edge.id)) {
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
