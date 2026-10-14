/**
 * A page's canvas (React Flow): cards as custom nodes at their reflowed positions, frames as nodes
 * behind what they hold (`pageGeometry`), resource cards and notes as nodes of their own, one memoized
 * arrow per call (`computeArrows`), the page's connections, and a camera that pans and zooms without
 * changing what any card shows. No minimap. Large pages render only the cards in view. A selected
 * arrow shows its call's contract in Selected; Escape or a click on the canvas clears it. An expanded
 * card sends an arrow from each call to the callee's card; a compact card, one from its edge to each
 * card it calls; a type card, one from each field to each type it names on the page. A connection
 * leaves from its step's call on an expanded card (`lineHandles`), a relationship from a table's
 * column. A collapsed frame is one node: what it holds isn't drawn, and their arrows and connections
 * attach to it, merged (`routeArrows`, `routeLinks`). Dragging a frame moves everything in it, as one
 * undo step; an attached note moves with its item.
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
import { useQueries } from "@tanstack/react-query";
import type { Card, Connection, Frame, Health, Key, Note, Resource } from "../ipc/generated";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useContextMenu } from "../components";
import { authorOf } from "../code/text";
import { engineQueryOptions } from "../ipc/queries";
import { isTyping } from "../lib/typing";
import { useAuthor } from "../state/author";
import {
  MAX_FRAME_DEPTH,
  displayedRects,
  editPage,
  findFrame,
  findNote,
  findPage,
  findResource,
  getOpenBoard,
  getPageView,
  itemsFor,
  moveFrame,
  moveItems,
  placeNote,
  requestCardFocus,
  select,
  setCamera,
  updatePageView,
  useOpenBoard,
  usePageView,
  useWorkingItems,
  type PageTarget,
} from "../state/boards";
import { findBlock } from "../state/cards";
import { ArrowEdge, type ArrowEdgeType } from "./ArrowEdge";
import { portsByCard, type Arrow } from "./arrows";
import type { BoardArrows } from "./useBoardArrows";
import { CardNode, type CardNodeType } from "./cards/CardNode";
import { TARGET_HANDLE } from "./cards/parts";
import { claimProblem } from "./cards/ReviewBanner";
import { chipForms, chipText } from "./connections/chip";
import { ConnectionEdge, type ConnectionData, type ConnectionEdgeType } from "./connections/ConnectionEdge";
import { linkMenuEntries } from "./connections/linkMenu";
import { lineHandles, relatedColumns, stepPorts, type DrawsStep } from "./connections/routing";
import { FrameNode, type FrameNodeType } from "./frames/FrameNode";
import { attachPoint, collapsedPlaces, fallbackItemSize, pageGeometry, type Drag } from "./frames/geometry";
import { routeArrows, routeLinks, type RoutedLink } from "./frames/routes";
import { boundsOf, type Rect } from "./layout";
import { LabelLayout, LabelLayoutContext } from "./lines/labelLayout";
import { NoteNode, type NoteNodeType } from "./notes/NoteNode";
import { ResourceNode, type ResourceNodeType } from "./resources/ResourceNode";
import "./canvas.css";

type CanvasNode = CardNodeType | FrameNodeType | ResourceNodeType | NoteNodeType;
type CanvasEdge = ArrowEdgeType | ConnectionEdgeType;

const nodeTypes = { card: CardNode, frame: FrameNode, resource: ResourceNode, note: NoteNode };
const edgeTypes = { arrow: ArrowEdge, connection: ConnectionEdge };
const NO_PORTS: readonly string[] = [];
const NO_LINKS: readonly Connection[] = [];
const NO_FRAMES: readonly Frame[] = [];
const NO_RESOURCES: readonly Resource[] = [];
const NO_NOTES: readonly Note[] = [];
/** Fit leaves room for the dock at the bottom, so nothing covers a card. */
const FIT_PADDING = { top: "32px", right: "40px", bottom: "96px", left: "40px" } as const;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 1.5;
/** Above this many cards, only the cards in view render. */
const VISIBLE_ONLY_ABOVE = 40;

/** Everything about an arrow its edge draws, as one string. */
function arrowSignature(arrow: Arrow): string {
  return [arrow.source, arrow.sourceHandle, arrow.target, arrow.kind, arrow.dashed, arrow.calls.join("\u0001"), arrow.cards?.from ?? "", arrow.cards?.to ?? "", arrow.merged === true].join("\u0000");
}

/** React Flow error 008 (an edge whose port isn't measured yet) is expected while cards load. */
function onFlowError(code: string, message: string): void {
  if (code !== "008") {
    console.warn(message);
  }
}

/** What a connection's edge reads besides the line: the page, its health, the selection, agents' work, and carried types' names. */
interface ConnectionContext {
  readonly target: PageTarget;
  readonly page: { readonly resources?: readonly Resource[] | undefined };
  readonly health: Health | undefined;
  readonly selectedLink: string | null;
  readonly working: ReadonlySet<string>;
  readonly typeName: (key: Key) => string | undefined;
}

/** What a connection's edge draws: its kind, chip, author, selection, health, ends' marks, and carried type. */
function connectionData(routed: RoutedLink, context: ConnectionContext): ConnectionData {
  const { link, kind } = routed;
  const { target, page } = context;
  const subject = link ? (findResource(page, link.to) ?? findResource(page, link.from))?.name : undefined;
  const carries = link?.carries !== undefined ? context.typeName(link.carries) : undefined;
  const forms = chipForms({
    kind,
    ...(link?.channel !== undefined ? { channel: link.channel } : {}),
    ...(carries !== undefined ? { carries } : {}),
    ...(link?.label ? { label: link.label.text } : {}),
    ...(subject !== undefined ? { subject } : {}),
    ...(link?.cardinality !== undefined ? { cardinality: link.cardinality } : {}),
    ...(routed.count > 1 ? { count: routed.count } : {}),
  });
  const writer = link ? authorOf(link.author) : null;
  const issue = link ? itemsFor(context.health, target.page, { connection: link.id })[0] : undefined;
  return {
    target,
    kind,
    forms,
    full: chipText(forms[0] ?? []),
    agent: writer?.agent === true ? writer.name : null,
    selected: routed.id === context.selectedLink,
    working: context.working.has(routed.id),
    problem: issue ? claimProblem(issue) : null,
    ...(link ? { linkId: link.id } : {}),
    ...(link?.cardinality !== undefined ? { cardinality: link.cardinality } : {}),
  };
}

export interface BoardCanvasProps {
  /** The page shown (a stable object while it is the same page). */
  readonly target: PageTarget;
  /** The page's cards (the document's list). */
  readonly cards: readonly Card[];
  /** The page's frames. */
  readonly frames?: readonly Frame[] | undefined;
  /** The page's resource cards. */
  readonly resources?: readonly Resource[] | undefined;
  /** The page's notes. */
  readonly notes?: readonly Note[] | undefined;
  /** The page's connections. */
  readonly links: readonly Connection[] | undefined;
  /** What the cards drew and the arrows derived from it (`useBoardArrows`, computed by the caller). */
  readonly boardArrows: BoardArrows;
}

export function BoardCanvas({ target, cards, frames = NO_FRAMES, resources = NO_RESOURCES, notes = NO_NOTES, links = NO_LINKS, boardArrows }: BoardCanvasProps) {
  const flow = useReactFlow<CanvasNode, CanvasEdge>();
  const sizes = usePageView(target, (view) => view.sizes);
  const focus = usePageView(target, (view) => view.focus);
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "compact";
  const selectedLink = usePageView(target, ({ selection }) => (selection.kind === "link" ? selection.linkId : null));
  const selectedArrow = usePageView(target, ({ selection }) => (selection.kind === "line" || selection.kind === "calls" ? selection.arrowId : null));
  const working = useWorkingItems(target);
  const health = useOpenBoard(target.board, (board) => board.health);
  const author = useAuthor();
  const [labelLayout] = useState(() => new LabelLayout());
  const [initialCamera] = useState(() => getPageView(target).camera);

  const { views, shapes, settled } = boardArrows;
  const framed = useMemo(() => ({ cards, defaultView, frames, resources, notes, links }), [cards, defaultView, frames, resources, notes, links]);
  // Where a connection can leave from its step: an expanded card whose drawn flow draws it.
  const drawsStep: DrawsStep = useCallback(
    (cardId, step) => {
      const view = views.get(cardId);
      return shapes.get(cardId) === "expanded" && view?.type === "definition" && findBlock(view.flow, step) !== undefined;
    },
    [views, shapes],
  );
  // The names of the types connections carry, from their definitions.
  const carried = useMemo(() => [...new Set(links.flatMap((link) => (link.carries !== undefined ? [link.carries] : [])))], [links]);
  const carriedNames = useQueries({
    queries: carried.map((key) => engineQueryOptions("type_definition", { project: target.board.project, key })),
    combine: (results) => results.map((result) => result.data?.name),
  });
  const typeName = useCallback((key: Key) => carriedNames[carried.indexOf(key)], [carried, carriedNames]);
  // What a collapsed frame hides is drawn as the frame: its arrows and links attach there.
  const shownAs = useMemo(() => collapsedPlaces(framed), [framed]);
  const arrows = useMemo(() => routeArrows(boardArrows.arrows, shownAs), [boardArrows.arrows, shownAs]);
  // A card's ports: its arrows', and the steps its connections leave from (on an expanded card).
  const ports = useMemo(() => {
    const merged = portsByCard(arrows);
    for (const [card, steps] of stepPorts(links, shownAs, drawsStep)) {
      merged.set(card, [...(merged.get(card) ?? []), ...steps].sort());
    }
    return merged;
  }, [arrows, links, shownAs, drawsStep]);
  const columns = useMemo(() => relatedColumns(links, shownAs), [links, shownAs]);
  const paneMenu = useContextMenu("Board actions");

  // Positions: saved positions reflowed by measured sizes, frames around what they hold; a dragged
  // card or frame follows the pointer.
  const [drag, setDrag] = useState<Drag | null>(null);
  const sizeOf = useCallback((id: string) => sizes[id] ?? fallbackItemSize(framed, id), [sizes, framed]);
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
  // Resource cards (with the columns relationships use) and notes.
  const [itemCache] = useState(() => new Map<string, { readonly signature: string; readonly node: ResourceNodeType | NoteNodeType }>());
  const itemNodes = useMemo(() => {
    const live = new Set<string>();
    const cached = (id: string, signature: string, make: () => ResourceNodeType | NoteNodeType) => {
      const known = itemCache.get(id);
      const node = known?.signature === signature ? known.node : make();
      itemCache.set(id, { signature, node });
      live.add(id);
      return node;
    };
    const placed = (id: string) => {
      const at = display.get(id);
      const className = working.has(id) ? "agent-working" : undefined;
      return at ? { at, extra: className !== undefined ? { className } : {}, signature: [at.x, at.y, className].join("\u0001") } : undefined;
    };
    const list = [
      ...resources.flatMap((resource) => {
        const spot = placed(resource.id);
        const used = columns.get(resource.id) ?? NO_PORTS;
        return spot
          ? [cached(resource.id, `${spot.signature}\u0001${used.join("\u0000")}`, (): ResourceNodeType => ({ id: resource.id, type: "resource", position: spot.at, dragHandle: ".resource-drag", ...spot.extra, data: { target, columns: used } }))]
          : [];
      }),
      ...notes.flatMap((note) => {
        const spot = placed(note.id);
        return spot ? [cached(note.id, spot.signature, (): NoteNodeType => ({ id: note.id, type: "note", position: spot.at, dragHandle: ".note-drag", ...spot.extra, data: { target } }))] : [];
      }),
    ];
    for (const id of [...itemCache.keys()]) {
      if (!live.has(id)) {
        itemCache.delete(id);
      }
    }
    return list;
  }, [resources, notes, display, columns, working, target, itemCache]);
  const nodes = useMemo((): CanvasNode[] => [...frameNodes, ...cardNodes, ...itemNodes], [frameNodes, cardNodes, itemNodes]);

  const [edgeCache] = useState(() => new Map<string, { readonly signature: string; readonly edge: ArrowEdgeType }>());
  const arrowEdges = useMemo(() => {
    const live = new Set<string>();
    const list = arrows.map((arrow) => {
      const selected = arrow.id === selectedArrow;
      // What the edge draws, so an unchanged arrow keeps its edge.
      const signature = [arrowSignature(arrow), String(selected)].join("\u0000");
      const cached = edgeCache.get(arrow.id);
      if (cached?.signature === signature && cached.edge.data?.page === target) {
        live.add(arrow.id);
        return cached.edge;
      }
      const ends = arrow.cards ?? { from: arrow.source, to: arrow.target };
      const [call] = arrow.calls;
      const selects = arrow.kind === "call" && arrow.merged !== true && call !== undefined;
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
        // A call's arrow is itself a button (ArrowEdge), which an `img` would hide.
        ...(selects ? { ariaRole: "group" as const } : {}),
        data: {
          dashed: arrow.dashed,
          start: arrow.kind === "start",
          selected,
          page: target,
          ...(!selects
            ? {}
            : arrow.calls.length > 1
              ? { calls: { arrowId: arrow.id, from: ends.from, to: ends.to, calls: arrow.calls } }
              : { line: { arrowId: arrow.id, call, from: ends.from, to: ends.to } }),
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
  }, [arrows, edgeCache, selectedArrow, target]);

  // Connections leave from their step or column where they can (`lineHandles`), else by the sides
  // facing each other, so they follow the cards as they move. Connections of a kind that join at a
  // collapsed frame merge into one that counts them.
  const [linkCache] = useState(() => new Map<string, { readonly signature: string; readonly edge: ConnectionEdgeType }>());
  const linkEdges = useMemo(() => {
    const live = new Set<string>();
    const list = routeLinks(links, shownAs).flatMap((routed): ConnectionEdgeType[] => {
      const from = display.get(routed.from);
      const to = display.get(routed.to);
      if (!from || !to) {
        return [];
      }
      const handles = lineHandles(routed.link, routed, from, to, drawsStep);
      const ends = { id: routed.id, source: routed.from, sourceHandle: handles.source, target: routed.to, targetHandle: handles.target, focusable: false, selectable: false };
      const data = connectionData(routed, { target, page: framed, health, selectedLink, working, typeName });
      const signature = JSON.stringify([ends, routed.kind, data.full, data.forms.map(chipText), data.agent, data.selected, data.working, data.problem, data.cardinality]);
      const known = linkCache.get(routed.id);
      const edge = known?.signature === signature ? known.edge : { ...ends, type: "connection" as const, className: "board-connection-edge", data };
      linkCache.set(routed.id, { signature, edge });
      live.add(routed.id);
      return [edge];
    });
    for (const id of [...linkCache.keys()]) {
      if (!live.has(id)) {
        linkCache.delete(id);
      }
    }
    return list;
  }, [links, shownAs, display, drawsStep, selectedLink, target, linkCache, working, framed, health, typeName]);
  const edges = useMemo((): CanvasEdge[] => [...arrowEdges, ...linkEdges], [arrowEdges, linkEdges]);

  // The cards (and collapsed frames) as drawn, which the lines' labels keep clear of.
  const cardRects = useMemo(
    () => [...display].flatMap(([id, rect]) => (boxes.has(id) ? [] : [rect])),
    [display, boxes],
  );
  useLayoutEffect(() => {
    labelLayout.setCards(cardRects);
  }, [labelLayout, cardRects]);

  // Escape lets go of a selected arrow, unless a field, a menu, or a dialog had it first.
  const arrowSelected = selectedArrow !== null;
  useEffect(() => {
    if (!arrowSelected) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented && !isTyping(event.target)) {
        event.preventDefault();
        select(target, { kind: "none" });
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, [arrowSelected, target]);

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
      } else if (node.type === "note" && findNote(framed, node.id)?.attach !== undefined) {
        // An attached note keeps its attachment, at its new offset from what it is attached to as drawn.
        const attach = findNote(framed, node.id)?.attach;
        const from = attach !== undefined ? attachPoint(framed, pageGeometry(framed, sizeOf).rects, attach) : undefined;
        if (attach !== undefined && from) {
          editPage(target, (page) => {
            placeNote(page, node.id, [node.position.x - from.x, node.position.y - from.y], attach);
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
    // Fit without names waits for everything drawn, so a board just opened fits what it shows; a
    // frame's box waits for what it draws.
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
          select(target, { kind: "none" });
        }}
        onEdgeClick={(_event, edge) => {
          if (edge.type === "connection" && links.some((link) => link.id === edge.id)) {
            select(target, { kind: "link", linkId: edge.id });
          }
        }}
        onEdgeContextMenu={(event, edge) => {
          if (edge.type !== "connection" || !links.some((link) => link.id === edge.id)) {
            return;
          }
          event.preventDefault();
          select(target, { kind: "link", linkId: edge.id });
          paneMenu.open({ x: event.clientX, y: event.clientY }, linkMenuEntries(target, edge.id, author));
        }}
        onPaneContextMenu={(event) => {
          event.preventDefault();
          paneMenu.open({ x: event.clientX, y: event.clientY }, [
            { id: "fit", label: "Fit", disabled: cards.length === 0 && resources.length === 0, onSelect: () => { requestCardFocus(target, [], "fit"); } },
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
    </LabelLayoutContext.Provider>
  );
}
