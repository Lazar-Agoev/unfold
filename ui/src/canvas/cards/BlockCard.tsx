/**
 * A definition's card (a module, class, or function, an inline function too), as the reader drew it.
 * Its header is the chevron that makes it compact or expanded, its
 * mark (`ƒ`, `λ`, `M`, or `class`), its name (its code name, `callback to …` for an inline function; or
 * a title the page gave it, in words, with the code name after it, `CardTitle`), and where it is; a
 * line saying its file changed since the last compile (or doesn't
 * parse), whose words are then the last compile's, the review banner (which alone says when its code is
 * missing from the graph), and the description, all of it, follow.
 *
 * Compact, that is all: its arrows leave from its edge. Expanded, it says what it has from its code
 * (the signature, the doc as a description from code, a module's imports, and what runs when it is
 * defined: decorators, bases, Python defaults), then its drawn flow (`FlowView`, laid out by
 * `layoutFlow`): a class's flow is its class-level code. Each arrow starts at the call that makes it
 * (`CallAnchor`), in the calls it opens in place too, and so does a connection from its step: the card
 * draws the line from the box's right side across itself to its edge, where the line goes on (under a
 * box beside the row, it steps down and leaves below that box). The body never scrolls: the card
 * sizes itself to its content.
 *
 * Its header's `</>` opens its code in the code panel, and so does a block's, or View code in a block's
 * menu (right-click), which also opens calls in place, names, describes, combines, and proposes steps as
 * the block's overlay actions allow (`blockMenuEntries`). While the panel is open, a line hovered in it lights its block here (a compact
 * card lights whole when it holds the block), a selected block that isn't drawn on its own outlines
 * the gap tick that counts its line, and while nothing is selected, the gap tick or row of other calls
 * the panel was opened from is outlined.
 */

import type { UseQueryResult } from "@tanstack/react-query";
import { flowBlocks } from "@unfold/desktop/drawing";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { CodeButton } from "../../code/CodeButton";
import { markedLine, useCodeView } from "../../code/view";
import { viewCode } from "../../code/open";
import { linkNote } from "../../code/text";
import { useContextMenu } from "../../components";
import { CombineSteps, ProposeStep } from "../../explain";
import { flowActions, flowSelection, selectForMenu } from "../../flow/actions";
import { FlowView, type FlowOutline } from "../../flow/FlowView";
import { layoutFlow, type FlowGeometry, type Point as FlowPoint } from "../../flow/layout";
import type { Card, CardView, DrawnFlow, FlowState, Header, Key, LinkEnd } from "../../ipc/generated";
import type { UnfoldError } from "../../ipc/unfold";
import { cardKey, clickItem, usePageView, type PageTarget, type Selection } from "../../state/boards";
import { heldBlocks, isMissing } from "../../state/cards";
import { openGap, useCodeHover, useCodePanel } from "../../state/codePanel";
import { useMeasure } from "../measure";
import type { CardPort } from "../arrows";
import { roundedPath } from "../lines/paths";
import { bodyRight, compactCardWidth, expandedCardWidth, flowPortSpots, headerText, nameFit } from "../sizing";
import { useItemWriting } from "../writing";
import { blockMenuEntries, type CombineRequest, type ProposeRequest } from "./blockMenu";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardMark, CardNote, CardTitle, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts, titleLabel } from "./parts";
import { ReviewBanner } from "./ReviewBanner";

export interface BlockCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly expanded: boolean;
  /** What the reader drew for it (an app's or entrance's start card draws as `EntranceCard` once loaded). */
  readonly query: UseQueryResult<CardView, UnfoldError>;
  readonly ports: readonly CardPort[];
  readonly selected: boolean;
}

const NO_ROWS: ReadonlySet<Key> = new Set();
const NO_VIEWS: ReadonlyMap<string, CardView> = new Map();

function stop(event: MouseEvent | KeyboardEvent): void {
  event.stopPropagation();
}

/** What a card says when its file isn't the one its drawing came from, in the code panel's words. */
const STATE_TEXT: Readonly<Record<Exclude<FlowState, "current">, string>> = {
  changed: "Changed since the last compile",
  doesNotParse: "Doesn't parse",
};

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

/** The part of a line inside an expanded card: its port's look, and its path from its box to the card's edge. */
interface PortLine {
  readonly port: CardPort;
  readonly path: readonly FlowPoint[];
}

/** Where a card's ports go: at the card's edge where their lines leave its body, or at the header. */
interface PortPlacement {
  readonly placed: Map<string, number>;
  readonly header: string[];
  readonly lines: readonly PortLine[];
}

/**
 * Where an expanded card's body places each of `ports` (its calls', and connections' steps), and the
 * part of each line inside the card, up to `right` (`flowPortSpots`): a line starts at its box (its
 * call's, else its step's), and its port is where it leaves the card. Ports it can't place stay at the
 * header.
 */
function placePorts(geometry: FlowGeometry, ports: readonly CardPort[], right: number): PortPlacement {
  const spots = flowPortSpots(geometry, right);
  const placed = new Map<string, number>();
  const header: string[] = [];
  const lines: PortLine[] = [];
  for (const port of ports) {
    const spot = spots.get(port.id);
    if (spot === undefined) {
      header.push(port.id);
    } else {
      placed.set(port.id, spot.y);
      lines.push({ port, path: spot.path });
    }
  }
  return { placed, header, lines };
}

const ARROW_LINES: ReadonlySet<CardPort["line"]> = new Set(["call", "start", "type"]);

/** The part of each line inside the card, drawn as the line is: an arrow's color and dash, or a connection's kind; heavier while selected. */
function PortLines({ lines }: { readonly lines: readonly PortLine[] }) {
  if (lines.length === 0) {
    return null;
  }
  return (
    <svg className="card-port-lines" aria-hidden="true">
      {lines.map(({ port, path }) => {
        const d = roundedPath(path);
        return ARROW_LINES.has(port.line) ? (
          <g key={port.id} className={`board-arrow${port.line === "start" ? " start" : ""}${port.selected ? " selected" : ""}`}>
            <path d={d} className={`board-arrow-line${port.dashed ? " dashed" : ""}`} />
          </g>
        ) : (
          <g key={port.id} className={`board-connection kind-${port.line}${port.selected ? " open" : ""}`}>
            <path d={d} className="connection-line" />
          </g>
        );
      })}
    </svg>
  );
}

/**
 * What the code panel outlines in card `cardId` when nothing selected is drawn here: while nothing is
 * selected, the gap tick (by its line) or the row of other calls (by its first call) it was opened
 * from; for a block of this card selected while not drawn on its own, the gap tick that counts its
 * marked line.
 */
function useOutline(cardId: string, selection: Selection, geometry: FlowGeometry): FlowOutline | null {
  const open = useCodePanel();
  const code = useCodeView(open);
  if (!open) {
    return null;
  }
  if (selection.kind === "none") {
    const gap = openGap(open);
    if (gap) {
      return { line: gap };
    }
    return open.from?.kind === "row" && open.cardId === cardId ? { block: open.from.key } : null;
  }
  const view = code.data;
  const line = view ? markedLine(view) : undefined;
  const undrawn = open.cardId === cardId && selection.kind === "card" && selection.key !== null && !geometry.blocks.has(selection.key);
  return undrawn && view?.file !== undefined && line !== undefined ? { line: { path: view.file, line } } : null;
}

/** A popover a block's menu opened, beside the block it was opened on. */
type Explaining = { readonly anchor: HTMLElement } & ({ readonly kind: "combine"; readonly request: CombineRequest } | { readonly kind: "propose"; readonly request: ProposeRequest });

/** An expanded card's drawn flow, with its lines from their calls to its edge and its ports there, and a block's menu (`blockMenuEntries`). */
function ExpandedBody({
  target,
  card,
  view,
  geometry,
  ports,
  lines,
  onToggleRow,
}: {
  readonly target: PageTarget;
  readonly card: Card;
  readonly view: CardView;
  readonly geometry: FlowGeometry;
  readonly ports: ReadonlyMap<string, number>;
  readonly lines: readonly PortLine[];
  readonly onToggleRow: (key: Key) => void;
}) {
  const selection = usePageView(target, (page) => page.selection);
  const selected = useMemo(() => flowSelection(selection, card.id), [selection, card.id]);
  const actions = useMemo(() => flowActions(target, card.id, view), [target, card.id, view]);
  const outline = useOutline(card.id, selection, geometry);
  const hover = useCodeHover();
  const menu = useContextMenu("Block actions", 220);
  const [explaining, setExplaining] = useState<Explaining | null>(null);
  const flow = view.type === "definition" ? view.flow : undefined;
  const close = () => {
    setExplaining(null);
  };
  let popover: ReactNode = null;
  if (explaining && flow) {
    popover =
      explaining.kind === "combine" ? (
        <CombineSteps target={target} anchor={explaining.anchor} flow={flow} block={explaining.request.key} steps={explaining.request.steps} onClose={close} />
      ) : (
        <ProposeStep target={target} anchor={explaining.anchor} place={explaining.request.place} label={explaining.request.label} onClose={close} />
      );
  }
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
          if (!flow) {
            return;
          }
          const anchor = event.currentTarget as HTMLElement;
          selectForMenu(target, card.id, block);
          menu.open(
            event,
            blockMenuEntries(target, card, flow, block, {
              onViewCode: actions.onViewCode,
              onToggleOpen: actions.onToggleOpen,
              onCombine: (request) => {
                setExplaining({ kind: "combine", request, anchor });
              },
              onPropose: (request) => {
                setExplaining({ kind: "propose", request, anchor });
              },
            }),
          );
        }}
      />
      <PortLines lines={lines} />
      {renderPorts([...ports.keys()], ports)}
      {/* Like the card's menu: a choice never also clicks the card it was opened on, and typing never reaches the canvas. */}
      {menu.element || popover ? (
        <span className="card-menu-layer" onClick={stop} onDoubleClick={stop} onContextMenu={stop} onKeyDown={stop}>
          {menu.element}
          {popover}
        </span>
      ) : null}
    </>
  );
}

/** Whether a line hovered in the code panel is on a block this compact card holds (drawn, or in a collapsed combined step). */
function useHoverHeld(flow: DrawnFlow | undefined, expanded: boolean): boolean {
  const hover = useCodeHover();
  const keys = useMemo(() => new Set(flow && !expanded ? [...flowBlocks(flow).map((block) => block.key), ...heldBlocks(flow)] : []), [flow, expanded]);
  return [hover?.key, hover?.block].some((key) => key !== null && key !== undefined && keys.has(key));
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
  // Its title and, beside a title the page gave it, its code name; its key while nothing is drawn.
  const name = text ? titleLabel(text) : key;
  const width = useMemo(() => {
    if (!flow || !text) {
      return undefined;
    }
    return geometry ? expandedCardWidth(flow.header, geometry, measure) : compactCardWidth(text, measure);
  }, [flow, text, geometry, measure]);
  const fit = useMemo(() => (text && width !== undefined ? nameFit(text, width, measure) : undefined), [text, width, measure]);
  const writing = useItemWriting(target, card.id);
  const placement = useMemo(
    (): PortPlacement => (geometry && width !== undefined ? placePorts(geometry, ports, bodyRight(width)) : { placed: new Map(), header: ports.map((port) => port.id), lines: [] }),
    [geometry, ports, width],
  );
  const lit = useHoverHeld(flow, expanded);

  const portsKey = ports.map((port) => port.id).join("\u0000");
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
    // Missing code is said once, by the review banner ("Missing from the graph").
    body = missing ? null : <CardNote warn>{query.error.message}</CardNote>;
  } else if (geometry) {
    body = (
      <div className="card-body nodrag nopan">
        <ExpandedBody target={target} card={card} view={query.data} geometry={geometry} ports={placement.placed} lines={placement.lines} onToggleRow={toggleRow} />
      </div>
    );
  }

  return (
    <article
      className={`board-card block-card${expanded ? " expanded" : " compact"}${selected ? " selected" : ""}${lit ? " lit" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={width !== undefined ? ({ width, "--name-fit": fit } as CSSProperties) : undefined}
      aria-label={name}
      onClick={() => {
        clickItem(target, card.id, { kind: "card", cardId: card.id, key: null });
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag">
        <ExpandToggle target={target} cardId={card.id} expanded={expanded} name={name} />
        {text ? <CardMark mark={text.mark} /> : null}
        <CardTitle text={text ?? { name: key }} writing={writing} />
        {text?.async === true ? <span className="card-tag">async</span> : null}
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
      {flow && flow.header.state !== "current" ? (
        <p className="card-state" role="status">
          {STATE_TEXT[flow.header.state]}
        </p>
      ) : null}
      <ReviewBanner target={target} cardId={card.id} view={query.data} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} wrap />
      {expanded && flow ? <FromCode header={flow.header} /> : null}
      {body}
      {menu.element}
    </article>
  );
}
