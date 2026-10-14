/**
 * A definition's card (a module, class, or function, an inline function too), as the reader drew it
 * (core-scope.html's board window). Its header is the chevron that makes it compact or expanded, its
 * mark (`ƒ`, `λ`, `M`, or `class`), its name (the page's, else its code name: `callback to …` for an
 * inline function), and where it is; the review banner and the description follow.
 *
 * Compact, that is all: its arrows leave from its edge. Expanded, it says what it has from its code
 * (the signature, the doc as a description from code, a module's imports, and what runs when it is
 * defined: decorators, bases, Python defaults), then its drawn flow (`FlowView`, laid out by
 * `layoutFlow`): a class's flow is its class-level code. Each arrow leaves the card's edge level with
 * the call that makes it (`CallAnchor`), in the calls it opens in place too, and so does a connection
 * from its step. The body never scrolls: the card sizes itself to its content.
 *
 * Its header's `</>` opens its code in the code panel, and so does a block's, or View code in a block's
 * menu (right-click). While the panel is open, a line hovered in it lights its block here (a compact
 * card lights whole when it holds the block), and a selected block that isn't drawn on its own outlines
 * the gap tick that counts its line.
 */

import type { UseQueryResult } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { CodeButton } from "../../code/CodeButton";
import { markedLine, useCodeView } from "../../code/view";
import { viewCode } from "../../code/open";
import { linkNote } from "../../code/text";
import { useContextMenu } from "../../components";
import { flowActions, flowSelection } from "../../flow/actions";
import { FlowView, type FlowLine, type FlowSelection } from "../../flow/FlowView";
import { layoutFlow, type FlowGeometry } from "../../flow/layout";
import type { Card, CardView, DrawnFlow, Header, Key, LinkEnd } from "../../ipc/generated";
import type { UnfoldError } from "../../ipc/unfold";
import { cardKey, select, usePageView, type PageTarget } from "../../state/boards";
import { flowBlocks, isMissing } from "../../state/cards";
import { useCodeHover, useCodePanel } from "../../state/codePanel";
import { callPort, stepPort } from "../arrows";
import { useMeasure } from "../measure";
import { compactCardWidth, expandedCardWidth, headerText } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardMark, CardNote, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";

export interface BlockCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly expanded: boolean;
  /** What the reader drew for it (an app's or entrance's start card draws as `EntranceCard` once loaded). */
  readonly query: UseQueryResult<CardView, UnfoldError>;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

const NO_ROWS: ReadonlySet<Key> = new Set();
const NO_VIEWS: ReadonlyMap<string, CardView> = new Map();

function stop(event: MouseEvent): void {
  event.stopPropagation();
}

/** How a link from the header's code reads: accent for the project's, muted otherwise (its tooltip says why: `linkNote`). */
function linkClass(to: LinkEnd): string {
  return `card-link ${to.outcome}`;
}

/** What an expanded card says from its code: its signature, its doc, its imports, and what runs when it is defined. */
function FromCode({ header }: { readonly header: Header }) {
  const { signature, description, imports, definedWith } = header;
  if (signature === undefined && description === undefined && imports.length === 0 && definedWith.length === 0) {
    return null;
  }
  return (
    <div className="card-code">
      {signature !== undefined ? <p className="card-signature">{signature}</p> : null}
      {description !== undefined ? (
        <p className="card-doc">
          <span className="card-source-tag">From code</span>
          {description}
        </p>
      ) : null}
      {imports.length > 0 ? (
        <p className="card-links">
          <span className="card-links-label">Imports</span>
          {imports.map((item, index) => (
            <span key={index} className={linkClass(item.to)} title={linkNote(item.to) ?? undefined}>
              {item.text}
            </span>
          ))}
        </p>
      ) : null}
      {definedWith.length > 0 ? (
        <p className="card-links">
          <span className="card-links-label">Runs when defined</span>
          {definedWith.map((call) => (
            <span key={call.key} className={linkClass(call.to)} title={linkNote(call.to) ?? undefined}>
              {call.role === "decorator" ? "@" : ""}
              {call.callee}
              {call.role !== undefined ? <i> {call.role}</i> : null}
            </span>
          ))}
        </p>
      ) : null}
    </div>
  );
}

/**
 * Where an expanded card's body places each of `ports` (its calls', and connections' steps): level
 * with the call's box, else (a connection's step that isn't a call) the block's. Ports it can't place
 * stay at the header.
 */
function placePorts(geometry: FlowGeometry, ports: readonly string[]): { readonly placed: Map<string, number>; readonly header: string[] } {
  const spots = new Map<string, number>();
  for (const anchor of geometry.anchors) {
    for (const port of [callPort(anchor.call), stepPort(anchor.call)]) {
      if (!spots.has(port)) {
        spots.set(port, anchor.at.y);
      }
    }
  }
  for (const [key, rect] of geometry.blocks) {
    if (!spots.has(stepPort(key))) {
      spots.set(stepPort(key), rect.y + Math.min(rect.height / 2, 13));
    }
  }
  const placed = new Map<string, number>();
  const header: string[] = [];
  for (const port of ports) {
    const y = spots.get(port);
    if (y === undefined) {
      header.push(port);
    } else {
      placed.set(port, y);
    }
  }
  return { placed, header };
}

/**
 * The gap tick the code panel outlines in card `cardId`: the one it was opened from, or, for a block of
 * this card selected while not drawn on its own, the one that counts its marked line.
 */
function useOutlineLine(cardId: string, selected: FlowSelection | null, geometry: FlowGeometry): FlowLine | null {
  const open = useCodePanel();
  const code = useCodeView(open);
  if (!open) {
    return null;
  }
  if (open.gap) {
    return open.gap;
  }
  const view = code.data;
  const line = view ? markedLine(view) : undefined;
  const undrawn = open.cardId === cardId && selected?.kind === "block" && !geometry.blocks.has(selected.key);
  return undrawn && view?.file !== undefined && line !== undefined ? { path: view.file, line } : null;
}

/** An expanded card's drawn flow, with its ports where its calls are, and a block's menu. */
function ExpandedBody({
  target,
  card,
  view,
  geometry,
  ports,
  onToggleRow,
}: {
  readonly target: PageTarget;
  readonly card: Card;
  readonly view: CardView;
  readonly geometry: FlowGeometry;
  readonly ports: ReadonlyMap<string, number>;
  readonly onToggleRow: (key: Key) => void;
}) {
  const selection = usePageView(target, (page) => page.selection);
  const selected = useMemo(() => flowSelection(selection, card.id), [selection, card.id]);
  const actions = useMemo(() => flowActions(target, card.id, view), [target, card.id, view]);
  const outline = useOutlineLine(card.id, selected, geometry);
  const hover = useCodeHover();
  const menu = useContextMenu("Block actions", 200);
  return (
    <>
      <FlowView
        geometry={geometry}
        selected={selected}
        outline={outline}
        hover={hover}
        {...actions}
        onToggleRow={onToggleRow}
        onBlockMenu={(event, block) => {
          actions.onSelect?.(block);
          menu.open(event, [{ id: "code", label: "View code", onSelect: () => actions.onViewCode?.(block) }]);
        }}
      />
      {renderPorts([...ports.keys()], ports)}
      {/* Like the card's menu: a choice never also clicks the card it was opened on. */}
      {menu.element ? (
        <span className="card-menu-layer" onClick={stop} onDoubleClick={stop} onContextMenu={stop}>
          {menu.element}
        </span>
      ) : null}
    </>
  );
}

/** Whether a line hovered in the code panel is on a block this compact card holds. */
function useHoverHeld(flow: DrawnFlow | undefined, expanded: boolean): boolean {
  const key = useCodeHover()?.key ?? null;
  const keys = useMemo(() => new Set(flow && !expanded ? flowBlocks(flow).map((block) => block.key) : []), [flow, expanded]);
  return key !== null && keys.has(key);
}

export function BlockCard({ target, card, expanded, query, ports, selected }: BlockCardProps) {
  const key = cardKey(card);
  const flow = query.data?.type === "definition" ? query.data.flow : undefined;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const menu = useCardMenu(target, card, flow !== undefined && flow.header.kind !== "type");
  // Thin rows listed one call per row (a view of this card only, never saved).
  const [listed, setListed] = useState<ReadonlySet<Key>>(NO_ROWS);
  const geometry = useMemo(() => (expanded && flow ? layoutFlow(flow, { measure, listed }) : null), [expanded, flow, measure, listed]);
  const text = useMemo(() => (flow ? headerText(flow.header, true) : null), [flow]);
  const name = text?.name ?? key;
  const width = useMemo(() => {
    if (!flow || !text) {
      return undefined;
    }
    return geometry ? expandedCardWidth(flow.header, geometry, measure) : compactCardWidth(text, measure);
  }, [flow, text, geometry, measure]);
  const placement = useMemo(() => (geometry ? placePorts(geometry, ports) : { placed: new Map<string, number>(), header: [...ports] }), [geometry, ports]);
  const lit = useHoverHeld(flow, expanded);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, width, geometry]);

  const toggleRow = (rowKey: Key) => {
    setListed((current) => {
      const next = new Set(current);
      if (!next.delete(rowKey)) {
        next.add(rowKey);
      }
      return next;
    });
  };
  const missing = query.isError && isMissing(query.error);
  let body: ReactNode = null;
  if (query.isPending) {
    body = <CardNote>Loading…</CardNote>;
  } else if (query.isError) {
    body = <CardNote warn>{missing ? `This ${"entrance" in card.ref ? "entrance" : "reference"} is missing from the graph.` : query.error.message}</CardNote>;
  } else if (geometry) {
    body = (
      <div className="card-body nodrag nopan">
        <ExpandedBody target={target} card={card} view={query.data} geometry={geometry} ports={placement.placed} onToggleRow={toggleRow} />
      </div>
    );
  }

  return (
    <article
      className={`board-card block-card${expanded ? " expanded" : " compact"}${selected ? " selected" : ""}${lit ? " lit" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={width !== undefined ? { width } : undefined}
      aria-label={name}
      onClick={() => {
        select(target, { kind: "card", cardId: card.id, key: null });
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag">
        <ExpandToggle target={target} cardId={card.id} expanded={expanded} name={name} />
        {text ? <CardMark mark={text.mark} /> : null}
        <h3 className="card-title" title={name}>
          {name}
        </h3>
        {text ? (
          <span className="card-location" title={flow ? `${flow.header.path}:${flow.header.line}` : undefined}>
            {text.location}
          </span>
        ) : null}
        <CodeButton
          className="card-view-code"
          onView={() => {
            viewCode(target, { kind: "card", cardId: card.id, key: null }, NO_VIEWS);
          }}
        />
        <HeaderPorts ids={placement.header} />
      </header>
      <ReviewBanner target={target} cardId={card.id} view={query.data} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} wrap={!expanded} />
      {expanded && flow ? <FromCode header={flow.header} /> : null}
      {body}
      {menu.element}
    </article>
  );
}
