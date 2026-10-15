/**
 * A page's canvas (React Flow): cards as custom nodes at their reflowed positions, frames as nodes
 * behind what they hold (`pageGeometry`), resource cards and notes as nodes of their own, one memoized
 * arrow per call (`computeArrows`), the page's connections, and a camera that pans and zooms without
 * changing what any card shows, over the pages' dot grid. No minimap. Large pages render only the cards in view. A selected
 * arrow shows its call's contract in Selected; Escape or a click on the canvas clears it. An expanded
 * card sends an arrow from each call to the callee's card; a compact card, one from its edge to each
 * card it calls; a type card, one from each field to each type it names on the page. A connection
 * leaves from its step's call on an expanded card, a relationship from a table's column. Every line
 * is routed from the page itself (`routePageLines`, with the ports the canvas measured, kept per page
 * like sizes, else estimated): orthogonal, around the cards, each edge reading its route by id; while
 * an item is dragged, only its lines follow it until it drops. A collapsed frame is one node: what it
 * holds isn't drawn, and their arrows and connections attach to it, merged (`routeArrows`, `routeLinks`). Dragging a frame moves everything in it, as one
 * undo step; an attached note moves with its item. The canvas's menu has Add resource… (placed where the
 * menu opened) and Fit; while a link is being drawn, the cursor is a crosshair and a click on the
 * canvas lets go of it.
 */

import {
  Background,
  ReactFlow,
  getViewportForBounds,
  useReactFlow,
  useStore,
  type NodeChange,
  type OnMove,
  type ReactFlowState,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useQueries } from "@tanstack/react-query";
import type { Card, Connection, Frame, Health, Key, Note, Resource } from "../ipc/generated";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useContextMenu } from "../components";
import { AddResource } from "../explain";
import { engineQueryOptions } from "../ipc/queries";
import { isTyping } from "../lib/typing";
import { useAuthor } from "../state/author";
import {
  cancelLink,
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
  type PageTarget,
} from "../state/boards";
import { findBlock } from "../state/cards";
import { ArrowEdge, type ArrowEdgeType } from "./ArrowEdge";
import { portsByCard, type Arrow, type CardPort } from "./arrows";
import type { BoardArrows } from "./useBoardArrows";
import { CardNode, type CardNodeType } from "./cards/CardNode";
import { TARGET_HANDLE } from "./cards/parts";
import { claimProblem } from "./cards/ReviewBanner";
import { chipForms, chipText, connectionTitle } from "./connections/chip";
import { ConnectionEdge, type ConnectionData, type ConnectionEdgeType } from "./connections/ConnectionEdge";
import { linkMenuEntries } from "./connections/linkMenu";
import { lineHandles, relatedColumns, stepPorts, type DrawsStep } from "./connections/routing";
import { FrameNode, type FrameNodeType } from "./frames/FrameNode";
import { attachPoint, collapsedPlaces, fallbackItemSize, pageGeometry, type Drag } from "./frames/geometry";
import { routeArrows, routeLinks, type RoutedLink } from "./frames/routes";
import { FLAT, edgeInsets, frameNesting, insideZones, titleWidth } from "./frames/edges";
import { boundsOf, type Rect } from "./layout";
import { LabelLayout, LabelLayoutContext } from "./lines/labelLayout";
import { LineRoutes, LineRoutesContext } from "./lines/lineRoutes";
import { routePageLines } from "./lines/pageLines";
import { portEstimator } from "./sizing";
import { useMeasure } from "./measure";
import { NoteNode, type NoteNodeType } from "./notes/NoteNode";
import { ResourceNode, type ResourceNodeType } from "./resources/ResourceNode";
import { WritingChips } from "./writing";
import { ZoomScaleVariables } from "./zoom";
import "./canvas.css";

type CanvasNode = CardNodeType | FrameNodeType | ResourceNodeType | NoteNodeType;
type CanvasEdge = ArrowEdgeType | ConnectionEdgeType;

const nodeTypes = { card: CardNode, frame: FrameNode, resource: ResourceNode, note: NoteNode };
const edgeTypes = { arrow: ArrowEdge, connection: ConnectionEdge };
const NO_PORTS: readonly CardPort[] = [];
const NO_COLUMNS: readonly string[] = [];

/**
 * Where React Flow measured each node's source handles (its ports, side handles, and columns' handles):
 * below its top, to the half pixel, as one string, so it changes only when a handle moves.
 */
function measuredPorts(state: ReactFlowState): string {
  let out = "";
  for (const node of state.nodeLookup.values()) {
    const handles = node.internals.handleBounds?.source;
    if (!handles || handles.length === 0) {
      continue;
    }
    out += node.id;
    for (const handle of handles) {
      out += `\u0001${handle.id ?? ""}\u0002${Math.round((handle.y + handle.height / 2) * 2) / 2}`;
    }
    out += "\u0000";
  }
  return out;
}

/** The ports in `measuredPorts`' string, by node and handle. */
function parsePorts(measured: string): Map<string, Record<string, number>> {
  const out = new Map<string, Record<string, number>>();
  for (const entry of measured.split("\u0000")) {
    const [node, ...handles] = entry.split("\u0001");
    if (!node) {
      continue;
    }
    out.set(node, Object.fromEntries(handles.map((handle) => {
      const [id, offset] = handle.split("\u0002");
      return [id ?? "", Number(offset)];
    })));
  }
  return out;
}

/** Everything about a card's ports its node draws, as one string. */
function portsSignature(ports: readonly CardPort[]): string {
  return ports.map((port) => [port.id, port.line, port.dashed, port.selected].join("\u0001")).join("\u0000");
}
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
/** The dot grid (canvas.css colors it): a 2px dot every 18px, as the design pages draw the canvas. */
const DOT_GAP = 18;
const DOT_SIZE = 2;

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

/** What a connection's edge reads besides the line: the page, its health, the selection, and carried types' names. */
interface ConnectionContext {
  readonly target: PageTarget;
  readonly page: { readonly resources?: readonly Resource[] | undefined };
  readonly health: Health | undefined;
  readonly selectedLink: string | null;
  readonly typeName: (key: Key) => string | undefined;
}

/** What a connection's edge draws: its kind, chip, card, selection, health, ends' marks, and carried type. */
function connectionData(routed: RoutedLink, context: ConnectionContext): ConnectionData {
  const { link, kind } = routed;
  const { target, page } = context;
  const subject = link ? (findResource(page, link.to) ?? findResource(page, link.from))?.name : undefined;
  const carries = link?.carries !== undefined ? context.typeName(link.carries) : undefined;
  const input = {
    kind,
    ...(link?.channel !== undefined ? { channel: link.channel } : {}),
    ...(carries !== undefined ? { carries } : {}),
    ...(link?.label ? { label: link.label.text } : {}),
    ...(subject !== undefined ? { subject } : {}),
    ...(link?.cardinality !== undefined ? { cardinality: link.cardinality } : {}),
    ...(routed.count > 1 ? { count: routed.count } : {}),
  };
  const title = connectionTitle(input);
  const issue = link ? itemsFor(context.health, target.page, { connection: link.id })[0] : undefined;
  return {
    target,
    kind,
    forms: chipForms(input),
    title,
    full: chipText(title),
    description: link?.description?.text ?? null,
    selected: routed.id === context.selectedLink,
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
  const measure = useMeasure();
  const sizes = usePageView(target, (view) => view.sizes);
  const focus = usePageView(target, (view) => view.focus);
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "compact";
  const selectedLink = usePageView(target, ({ selection }) => (selection.kind === "link" ? selection.linkId : null));
  const selectedArrow = usePageView(target, ({ selection }) => (selection.kind === "line" || selection.kind === "calls" ? selection.arrowId : null));
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
  const routedLinks = useMemo(() => routeLinks(links, shownAs), [links, shownAs]);
  // A card's ports, each with its line's look: its arrows', and the steps its connections leave from (on an expanded card).
  const ports = useMemo(() => {
    const merged = portsByCard(arrows, selectedArrow);
    for (const [card, steps] of stepPorts(links, shownAs, drawsStep, selectedLink)) {
      merged.set(card, [...(merged.get(card) ?? []), ...steps].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)));
    }
    return merged;
  }, [arrows, links, shownAs, drawsStep, selectedArrow, selectedLink]);
  const columns = useMemo(() => relatedColumns(links, shownAs), [links, shownAs]);
  const paneMenu = useContextMenu("Board actions");
  // Add resource… from the canvas's menu: where the menu opened, on the screen and on the canvas.
  const [addingResource, setAddingResource] = useState<{ readonly point: { readonly x: number; readonly y: number }; readonly at: readonly [number, number] } | null>(null);
  const [resourceAnchor, setResourceAnchor] = useState<HTMLElement | null>(null);
  const linking = usePageView(target, (view) => view.linking !== null);

  // Positions: saved positions reflowed by measured sizes, frames around what they hold; a dragged
  // card or frame follows the pointer.
  const [drag, setDrag] = useState<Drag | null>(null);
  const sizeOf = useCallback((id: string) => sizes[id] ?? fallbackItemSize(framed, id), [sizes, framed]);
  const resting = useMemo(() => pageGeometry(framed, sizeOf), [framed, sizeOf]);
  const geometry = useMemo(() => (drag ? pageGeometry(framed, sizeOf, drag) : resting), [framed, sizeOf, drag, resting]);
  const display = geometry.rects;

  // Lines: the router draws every one from the page (`routePageLines`), not from the edges React Flow
  // renders (a large page renders only what is in view). Ports are where the canvas measured them
  // (kept per page, like sizes), else estimated from what the cards drew.
  const measured = useStore(measuredPorts);
  useEffect(() => {
    const found = parsePorts(measured);
    const recorded = getPageView(target).ports;
    const changed = [...found].filter(([item, offsets]) => JSON.stringify(recorded[item]) !== JSON.stringify(offsets));
    if (changed.length > 0) {
      updatePageView(target, (view) => {
        for (const [item, offsets] of changed) {
          view.ports[item] = offsets;
        }
      });
    }
  }, [measured, target]);
  const recordedPorts = usePageView(target, (view) => view.ports);
  const estimate = useMemo(
    () =>
      portEstimator((id) => {
        const card = cards.find((item) => item.id === id);
        const shape = shapes.get(id);
        return card && shape ? { model: views.get(id), shape, description: card.description } : undefined;
      }, measure),
    [cards, views, shapes, measure],
  );
  const portOffset = useCallback((item: string, port: string) => recordedPorts[item]?.[port] ?? estimate(item, port), [recordedPorts, estimate]);
  // While something is dragged, only its lines (and those of what a dragged frame holds) follow it; the
  // rest are routed again when it is dropped.
  const restingRoutes = useMemo(
    () => routePageLines({ page: framed, geometry: resting, arrows, links: routedLinks, portOffset, drawsStep }),
    [framed, resting, arrows, routedLinks, portOffset, drawsStep],
  );
  const routes = useMemo(() => {
    if (!drag) {
      return restingRoutes;
    }
    const only = new Set([drag.id, ...(geometry.inside.get(drag.id) ?? [])]);
    return new Map([...restingRoutes, ...routePageLines({ page: framed, geometry, arrows, links: routedLinks, portOffset, drawsStep, only })]);
  }, [drag, geometry, restingRoutes, framed, arrows, routedLinks, portOffset, drawsStep]);
  const [lineRoutes] = useState(() => new LineRoutes());
  useLayoutEffect(() => {
    lineRoutes.set(routes);
  }, [lineRoutes, routes]);
  // An expanded frame's size is its box's, never measured: only collapsed frames' nodes are.
  const boxes = useMemo(() => new Set(geometry.frames.flatMap(({ frame, collapsed }) => (collapsed ? [] : [frame.id]))), [geometry]);

  // Node objects keep their identity while nothing about them changes, so React Flow skips them.
  const [frameCache] = useState(() => new Map<string, FrameNodeType>());
  // Boxes in a zone get a faint fill; a frame's edge moves out past the frames nested inside it.
  const zoned = useMemo(() => insideZones(frames), [frames]);
  const frameEdges = useMemo(() => {
    const expanded = geometry.frames.filter((drawn) => !drawn.collapsed);
    const nested = frameNesting(expanded);
    return new Map(expanded.map(({ frame, rect }) => [frame.id, edgeInsets(frame.look, nested.get(frame.id) ?? FLAT, titleWidth(frame, measure), rect.width)] as const));
  }, [geometry, measure]);
  // What sits at an expanded frame's top-right corner (the writing chip) follows its edge.
  const chipOffsets = useMemo(() => new Map([...frameEdges].map(([id, edge]) => [id, { x: `calc(-1 * ${edge.right})`, y: edge.top }] as const)), [frameEdges]);
  const frameNodes = useMemo(() => {
    const live = new Set<string>();
    const deepest = geometry.frames.reduce((most, drawn) => Math.max(most, drawn.depth), 0);
    const list = geometry.frames.map(({ frame, rect, depth, collapsed }) => {
      const framePorts = (ports.get(frame.id) ?? NO_PORTS).map((port) => port.id);
      const cached = frameCache.get(frame.id);
      const box = collapsed ? undefined : { width: rect.width, height: rect.height };
      const inZone = !collapsed && zoned.has(frame.id);
      const edge = collapsed ? undefined : frameEdges.get(frame.id);
      // Behind the lines and cards, outer frames behind inner ones; a collapsed frame is a node like a card.
      const zIndex = collapsed ? 0 : depth - deepest - 1;
      const node: FrameNodeType =
        cached?.position.x === rect.x &&
        cached.position.y === rect.y &&
        cached.zIndex === zIndex &&
        cached.data.box?.width === box?.width &&
        cached.data.box?.height === box?.height &&
        cached.data.inZone === inZone &&
        cached.data.edge?.top === edge?.top &&
        cached.data.edge?.right === edge?.right &&
        cached.data.edge?.bottom === edge?.bottom &&
        cached.data.ports.join("\u0000") === framePorts.join("\u0000")
          ? cached
          : {
              id: frame.id,
              type: "frame",
              position: { x: rect.x, y: rect.y },
              dragHandle: ".frame-drag",
              zIndex,
              data: { target, ports: framePorts, inZone, ...(box ? { box } : {}), ...(edge ? { edge } : {}) },
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
  }, [geometry, ports, target, frameCache, zoned, frameEdges]);

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
      const keptPorts = cached && portsSignature(cached.data.ports) === portsSignature(cardPorts) ? cached.data.ports : undefined;
      const node: CardNodeType =
        cached && keptPorts && cached.position.x === at.x && cached.position.y === at.y
          ? cached
          : {
              id: card.id,
              type: "card",
              position: at,
              dragHandle: ".card-drag",
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
  }, [cards, display, ports, target, nodeCache]);
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
      return at ? { at, signature: [at.x, at.y].join("\u0001") } : undefined;
    };
    const list = [
      ...resources.flatMap((resource) => {
        const spot = placed(resource.id);
        const used = columns.get(resource.id) ?? NO_COLUMNS;
        return spot
          ? [cached(resource.id, `${spot.signature}\u0001${used.join("\u0000")}`, (): ResourceNodeType => ({ id: resource.id, type: "resource", position: spot.at, dragHandle: ".resource-drag", data: { target, columns: used } }))]
          : [];
      }),
      ...notes.flatMap((note) => {
        const spot = placed(note.id);
        return spot ? [cached(note.id, spot.signature, (): NoteNodeType => ({ id: note.id, type: "note", position: spot.at, dragHandle: ".note-drag", data: { target } }))] : [];
      }),
    ];
    for (const id of [...itemCache.keys()]) {
      if (!live.has(id)) {
        itemCache.delete(id);
      }
    }
    return list;
  }, [resources, notes, display, columns, target, itemCache]);
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

  // Connections attach at their step or column where they can (`lineHandles`), else by the sides they
  // are routed on. Connections of a kind that join at a collapsed frame merge into one that counts them.
  const [linkCache] = useState(() => new Map<string, { readonly signature: string; readonly edge: ConnectionEdgeType }>());
  const linkEdges = useMemo(() => {
    const live = new Set<string>();
    const list = routedLinks.flatMap((routed): ConnectionEdgeType[] => {
      if (!display.has(routed.from) || !display.has(routed.to)) {
        return [];
      }
      const handles = lineHandles(routed.link, routed, routes.get(routed.id), drawsStep);
      const ends = { id: routed.id, source: routed.from, sourceHandle: handles.source, target: routed.to, targetHandle: handles.target, focusable: false, selectable: false };
      const data = connectionData(routed, { target, page: framed, health, selectedLink, typeName });
      const signature = JSON.stringify([ends, routed.kind, data.full, data.forms.map(chipText), data.description, data.selected, data.problem, data.cardinality]);
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
  }, [routedLinks, routes, display, drawsStep, selectedLink, target, linkCache, framed, health, typeName]);
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
    <LineRoutesContext.Provider value={lineRoutes}>
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
          cancelLink(target);
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
          const point = { x: event.clientX, y: event.clientY };
          const at = flow.screenToFlowPosition(point);
          paneMenu.open(point, [
            { id: "add-resource", label: "Add resource…", onSelect: () => { setAddingResource({ point, at: [at.x, at.y] }); } },
            { kind: "separator", id: "s1" },
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
        className={`board-canvas${linking ? " linking" : ""}`}
      >
        <Background gap={DOT_GAP} size={DOT_SIZE} />
        <ZoomScaleVariables />
        <WritingChips target={target} rects={display} offsets={chipOffsets} />
      </ReactFlow>
      {paneMenu.element}
      {addingResource ? (
        <>
          <span ref={setResourceAnchor} className="canvas-point" style={{ left: addingResource.point.x, top: addingResource.point.y }} />
          {resourceAnchor ? (
            <AddResource
              target={target}
              anchor={resourceAnchor}
              at={addingResource.at}
              onClose={() => {
                setAddingResource(null);
              }}
            />
          ) : null}
        </>
      ) : null}
    </LabelLayoutContext.Provider>
    </LineRoutesContext.Provider>
  );
}
