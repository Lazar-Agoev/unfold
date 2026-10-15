/**
 * Renders a laid-out drawn flow (`layoutFlow`): every piece absolutely placed at its rect, the wires in
 * one SVG over them. It decides nothing: selection, opening calls in place, combined steps' toggles,
 * listing thin rows, gap ticks' lines, a block's or a thin row's `</>`, and a block's menu all go to
 * the callbacks.
 *
 * The selection is outlined on the drawn thing that holds it (`FlowGeometry.blocks`: a block in a
 * collapsed combined step outlines the step, a hidden control block its gap tick), and so is `outline`
 * (a gap tick or a row of other calls the code panel was opened from) when the selection isn't drawn
 * here. A line hovered in the code panel lights its block (a call's, else the line's own block), or the
 * gap tick that counts it.
 */

import { Fragment, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { CodeButton } from "../code/CodeButton";
import type { CallBox, Key, OtherCall, RepoPath, ThinRow } from "../ipc/generated";
import { Icon } from "../icons";
import { callbackText, insetPlace, thinCount, type CallRow, type FlowGeometry, type FlowPiece, type FlowWire, type Point, type Rect } from "./layout";
import { callMeta, combinedSummary, exitWord, frameWord, gapWords, hasMeta, listedCalls, outcomeWords, sectionWords } from "./words";
import "./flow.css";

/** What can be selected in a flow: a block, or a combined or proposed step (by its page id). */
export type FlowSelection =
  | { readonly kind: "block"; readonly key: Key }
  | { readonly kind: "combined"; readonly item: string }
  | { readonly kind: "proposed"; readonly item: string };

/** A line of a file. */
interface FlowLine {
  readonly path: RepoPath;
  readonly line: number;
}

/** What is outlined when nothing selected is drawn here: a gap tick, by a line it counts, or a drawn block (a row of other calls, by its first call). */
export type FlowOutline = { readonly line: FlowLine } | { readonly block: Key };

/** A line hovered in the code panel: its block (a call's, when on its name), the line's own block (null for a line no block covers), and the line. */
interface FlowHover extends FlowLine {
  readonly key: Key | null;
  readonly block: Key | null;
}

export interface FlowViewProps {
  readonly geometry: FlowGeometry;
  /** What shows selected. */
  readonly selected?: FlowSelection | null;
  /** What is outlined when nothing selected is drawn here. */
  readonly outline?: FlowOutline | null;
  /** A line hovered in the code panel. */
  readonly hover?: FlowHover | null;
  readonly onSelect?: (selection: FlowSelection) => void;
  /** Opens a block's (or a combined step's) code in the code panel: its `</>`. */
  readonly onViewCode?: (selection: FlowSelection) => void;
  /** Opens a thin row's lines (in `path`) in the code panel: its `</>`. */
  readonly onViewRow?: (path: RepoPath, row: ThinRow) => void;
  /** Opens a block's (or a combined step's) menu. */
  readonly onBlockMenu?: (event: MouseEvent, selection: FlowSelection) => void;
  /** Opens or closes the call or callback at an open path (outermost first) in place. */
  readonly onToggleOpen?: (path: readonly Key[]) => void;
  /** Opens (`collapsed: false`) or collapses a combined step. */
  readonly onToggleCombined?: (item: string, collapsed: boolean) => void;
  /** Lists a thin row one call per row, or folds it (`FlowLayoutOptions.listed`). */
  readonly onToggleRow?: (key: Key) => void;
  /** Opens a gap tick's lines (first and last) in their file. */
  readonly onOpenLines?: (path: RepoPath, lines: readonly [number, number]) => void;
}

interface Handlers {
  readonly selected: FlowSelection | null;
  readonly select: (selection: FlowSelection) => void;
  readonly toggleOpen: (path: readonly Key[]) => void;
  readonly viewCode: ((selection: FlowSelection) => void) | undefined;
  readonly menu: ((event: MouseEvent, selection: FlowSelection) => void) | undefined;
}

function place(rect: Rect) {
  return { left: rect.x, top: rect.y, width: rect.width, height: rect.height };
}

function classes(...names: (string | false | null | undefined)[]): string {
  return names.filter((name) => typeof name === "string" && name.length > 0).join(" ");
}

function isBlock(selected: FlowSelection | null, key: Key): boolean {
  return selected?.kind === "block" && selected.key === key;
}

function stop(event: MouseEvent | KeyboardEvent): void {
  event.stopPropagation();
}

/** Props that make an element select `selection` on a click, Enter, or Space, and open its menu. */
function selectable(handlers: Handlers, selection: FlowSelection) {
  const { menu } = handlers;
  return {
    role: "button",
    tabIndex: 0,
    onClick: (event: MouseEvent) => {
      stop(event);
      handlers.select(selection);
    },
    ...(menu && selection.kind !== "proposed"
      ? {
          onContextMenu: (event: MouseEvent) => {
            stop(event);
            menu(event, selection);
          },
        }
      : {}),
    onKeyDown: (event: KeyboardEvent) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        stop(event);
        handlers.select(selection);
      }
    },
  } as const;
}

/** A block's `</>`, shown while it is hovered or selected. */
function ViewCode({ handlers, selection }: { readonly handlers: Handlers; readonly selection: FlowSelection }) {
  const { viewCode } = handlers;
  return viewCode ? (
    <CodeButton
      className="flow-code"
      onView={() => {
        viewCode(selection);
      }}
    />
  ) : null;
}

function Chevron({ open, name, onToggle }: { readonly open: boolean; readonly name: string; readonly onToggle: () => void }) {
  return (
    <button
      type="button"
      className="flow-chevron nodrag"
      aria-expanded={open}
      aria-label={open ? `Close ${name}` : `Open ${name} in place`}
      onClick={(event) => {
        stop(event);
        onToggle();
      }}
    >
      <Icon name="chevron-right-small" size={10} />
    </button>
  );
}

function Unknown() {
  return (
    <span className="flow-unknown" aria-label="unknown target">
      ?
    </span>
  );
}

/** A call's name: the overlay's name with the code name beside it, or the code name. */
function CallName({ box, className }: { readonly box: CallBox; readonly className: string }) {
  return box.name !== undefined ? (
    <>
      <span className={`${className} named`}>{box.name}</span>
      <span className="flow-code-name">{box.callee}</span>
    </>
  ) : (
    <span className={className}>{box.callee}</span>
  );
}

/** The library and unknown calls written inside a call drawn on a test, a header, or an exit: `+ x.foo ?, logger.info`. */
function Listed({ calls }: { readonly calls: readonly OtherCall[] }) {
  const listed = listedCalls(calls);
  return listed.length > 0 ? (
    <span className="flow-listed">
      {"+ "}
      {listed.map((call, index) => (
        <Fragment key={call.key}>
          {index > 0 ? ", " : null}
          {call.callee}
          {call.to.outcome === "unknown" ? <Unknown /> : null}
        </Fragment>
      ))}
    </span>
  ) : null;
}

/** A project call written in a test, a header, or an exit's value: a small box that opens in place like a call, with the calls written inside it. */
function Chip({ box, path, handlers }: { readonly box: CallBox; readonly path: readonly Key[]; readonly handlers: Handlers }) {
  const own = [...path, box.key];
  return (
    <span className={classes("flow-chip", box.sometimes && "sometimes", isBlock(handlers.selected, box.key) && "selected")} data-key={box.key} {...selectable(handlers, { kind: "block", key: box.key })}>
      <CallName box={box} className="flow-chip-name" />
      {box.to.outcome === "unknown" ? <Unknown /> : null}
      <Listed calls={box.calls} />
      {box.actions.open ? <Chevron open={box.inset !== undefined} name={box.name ?? box.callee} onToggle={() => {
            handlers.toggleOpen(own);
          }} /> : null}
      <ViewCode handlers={handlers} selection={{ kind: "block", key: box.key }} />
    </span>
  );
}

/** An unknown call written in a test or a header, listed after it: a chip with its `?`. */
function UnknownChip({ call, handlers }: { readonly call: OtherCall; readonly handlers: Handlers }) {
  return (
    <span className={classes("flow-chip", "other", call.sometimes && "sometimes", isBlock(handlers.selected, call.key) && "selected")} data-key={call.key} {...selectable(handlers, { kind: "block", key: call.key })}>
      <span className="flow-chip-name">{call.callee}</span>
      <Unknown />
      <ViewCode handlers={handlers} selection={{ kind: "block", key: call.key }} />
    </span>
  );
}

/** What a test or a header draws after its words: its boxes as chips, then its unknown calls. */
function Chips({ boxes, unknown, path, handlers }: { readonly boxes: readonly CallBox[]; readonly unknown: readonly OtherCall[]; readonly path: readonly Key[]; readonly handlers: Handlers }) {
  return (
    <>
      {boxes.map((box) => (
        <Chip key={box.key} box={box} path={path} handlers={handlers} />
      ))}
      {unknown.map((call) => (
        <UnknownChip key={call.key} call={call} handlers={handlers} />
      ))}
    </>
  );
}

function CallPiece({ piece, handlers }: { readonly piece: Extract<FlowPiece, { kind: "call" }>; readonly handlers: Handlers }) {
  const { box } = piece;
  const meta = callMeta(box);
  const project = box.to.outcome === "project";
  return (
    <div
      className={classes("flow-call", project ? "project" : "other", box.sometimes && "sometimes", piece.dim && "dim", isBlock(handlers.selected, box.key) && "selected")}
      style={place(piece.rect)}
      data-key={box.key}
      {...selectable(handlers, { kind: "block", key: box.key })}
    >
      <div className="flow-call-head">
        <CallName box={box} className="flow-call-name" />
        {box.to.outcome === "unknown" ? <Unknown /> : null}
        {box.actions.open ? <Chevron open={box.inset !== undefined} name={box.name ?? box.callee} onToggle={() => {
            handlers.toggleOpen(piece.path);
          }} /> : null}
      </div>
      {hasMeta(meta) ? (
        <div className="flow-call-meta">
          {meta.binds !== null ? <span className="flow-binds">{meta.binds}</span> : null}
          {meta.feeds !== null ? <span className="flow-feeds">{meta.feeds}</span> : null}
          {meta.tags.map((tag) => (
            <span key={tag} className={classes("flow-tag", tag === "sometimes" && "sometimes")}>
              {tag}
            </span>
          ))}
          {meta.why !== null ? <span className="flow-why">{meta.why}</span> : null}
        </div>
      ) : null}
      {piece.rows.length > 0 ? (
        <div className="flow-call-rows">
          {piece.rows.map((row) => (
            <CallRowView key={row.call.key} row={row} handlers={handlers} />
          ))}
        </div>
      ) : null}
      <ViewCode handlers={handlers} selection={{ kind: "block", key: box.key }} />
    </div>
  );
}

function CallRowView({ row, handlers }: { readonly row: CallRow; readonly handlers: Handlers }) {
  const { call } = row;
  const why = outcomeWords(call.to);
  return (
    <div
      className={classes("flow-call-row", isBlock(handlers.selected, call.key) && "selected")}
      style={{ paddingLeft: row.depth * 14 }}
      data-key={call.key}
      {...selectable(handlers, { kind: "block", key: call.key })}
    >
      <span className="flow-row-name">{call.callee}</span>
      {call.to.outcome === "unknown" ? <Unknown /> : null}
      {why.length > 0 ? <span className="flow-why">{why}</span> : null}
    </div>
  );
}

function ThinPiece({
  piece,
  handlers,
  onToggleRow,
  onViewRow,
}: {
  readonly piece: Extract<FlowPiece, { kind: "thin" }>;
  readonly handlers: Handlers;
  readonly onToggleRow: ((key: Key) => void) | undefined;
  readonly onViewRow: ((path: RepoPath, row: ThinRow) => void) | undefined;
}) {
  const { row, listed } = piece;
  const count = thinCount(row);
  const toggle = () => onToggleRow?.(piece.key);
  return (
    <div
      className={classes("flow-thin", listed && "listed", piece.dim && "dim")}
      style={place(piece.rect)}
      role="button"
      tabIndex={0}
      aria-expanded={listed}
      aria-label={listed ? "Fold these calls into one row" : "List these calls one per row"}
      onClick={(event) => {
        stop(event);
        toggle();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          stop(event);
          toggle();
        }
      }}
    >
      <span className="flow-thin-mark">{listed ? "Library and unknown calls" : "·"}</span>
      {row.calls.map((call, index) => {
        const inside = insideNames(call.inside);
        const why = outcomeWords(call.to);
        return (
          <span
            key={call.key}
            className={classes("flow-thin-item", call.to.outcome === "builtin" && "builtin", isBlock(handlers.selected, call.key) && "selected")}
            data-key={call.key}
            {...(listed ? selectable(handlers, { kind: "block", key: call.key }) : {})}
          >
            <span className="flow-thin-name">
              {call.role === "decorator" ? "@" : ""}
              {call.callee}
              {call.to.outcome === "unknown" ? <Unknown /> : null}
            </span>
            {call.binds !== undefined ? <span className="flow-binds">→ {call.binds}</span> : null}
            {listed && why.length > 0 ? <span className="flow-why">{why}</span> : null}
            {listed && call.feeds !== undefined ? <span className="flow-thin-small">into {call.feeds.name ?? call.feeds.callee}(…)</span> : null}
            {inside.length > 0 ? <span className="flow-thin-small">+ {inside.join(", ")}</span> : null}
            {!listed && index < row.calls.length - 1 ? <span className="flow-thin-sep">,</span> : null}
          </span>
        );
      })}
      {!listed && count !== null ? <span className="flow-thin-count">{count}</span> : null}
      {onViewRow ? (
        <CodeButton
          className="flow-code"
          onView={() => {
            onViewRow(piece.path, row);
          }}
        />
      ) : null}
    </div>
  );
}

function insideNames(calls: CallRow["call"]["inside"]): string[] {
  return calls.flatMap((call) => [call.callee, ...insideNames(call.inside)]);
}

function ExitPiece({ piece, handlers }: { readonly piece: Extract<FlowPiece, { kind: "exit" }>; readonly handlers: Handlers }) {
  const { exit } = piece;
  const value = exit.value;
  return (
    <div className={classes("flow-exit", `x-${exit.form}`, piece.dim && "dim", isBlock(handlers.selected, exit.key) && "selected")} style={place(piece.rect)} data-key={exit.key} {...selectable(handlers, { kind: "block", key: exit.key })}>
      <span className="flow-exit-word">{exitWord(exit.form)}</span>
      {exit.name !== undefined ? <span className="flow-name">{exit.name}</span> : null}
      {value?.kind === "call" && value.to.outcome === "project" ? <Chip box={value} path={piece.path} handlers={handlers} /> : null}
      {value?.kind === "call" && value.to.outcome !== "project" ? (
        <>
          <span className="flow-exit-value" data-key={value.key}>
            {value.callee}
            {value.to.outcome === "unknown" ? <Unknown /> : null}
          </span>
          <Listed calls={value.calls} />
        </>
      ) : null}
      {value?.kind === "text" ? <span className="flow-exit-value">{value.text}</span> : null}
      {piece.destination !== null ? <span className="flow-exit-destination">→ {piece.destination}</span> : null}
      <ViewCode handlers={handlers} selection={{ kind: "block", key: exit.key }} />
    </div>
  );
}

function FramePiece({ piece, handlers }: { readonly piece: Extract<FlowPiece, { kind: "frame" }>; readonly handlers: Handlers }) {
  const { frame, look } = piece;
  const cases = frame.form === "switch" || frame.form === "match";
  return (
    <div className={classes("flow-frame", look, piece.dim && "dim", isBlock(handlers.selected, frame.key) && "selected")} style={place(piece.rect)} data-key={frame.key} {...selectable(handlers, { kind: "block", key: frame.key })}>
      {look === "try" ? null : (
        <div className={classes("flow-frame-title", frame.headerCalls.length + frame.headerUnknown.length > 0 && !cases && "with-chips")}>
          <span className="flow-frame-word">{frameWord(frame.form)}</span>
          {frame.name !== undefined ? <span className="flow-name">{frame.name}</span> : null}
          {!cases && frame.header !== undefined ? <span className="flow-frame-header">{frame.header}</span> : null}
          {!cases ? <Chips boxes={frame.headerCalls} unknown={frame.headerUnknown} path={piece.path} handlers={handlers} /> : null}
          {frame.binds !== undefined ? <span className="flow-binds">→ {frame.binds}</span> : null}
        </div>
      )}
      <ViewCode handlers={handlers} selection={{ kind: "block", key: frame.key }} />
    </div>
  );
}

function HeadPiece({ piece, handlers }: { readonly piece: Extract<FlowPiece, { kind: "head" }>; readonly handlers: Handlers }) {
  const { lane, shown } = piece;
  const props = lane ? { "data-key": lane.key, ...selectable(handlers, { kind: "block", key: lane.key }) } : {};
  const words =
    shown.name !== null ? (
      <>
        <span className="flow-name">{shown.name}</span>
        {shown.text ? <span className="flow-code-name">{shown.text}</span> : null}
      </>
    ) : (
      shown.text
    );
  // Words cut short are whole in the tooltip.
  const whole = lane?.name !== undefined ? `${lane.name}\n${piece.text}` : piece.text;
  const cut = shown.text !== piece.text || shown.name !== (lane?.name ?? null);
  return (
    <div className={classes("flow-head", piece.look, piece.dim && "dim", lane && isBlock(handlers.selected, lane.key) && "selected")} style={place(piece.rect)} {...props}>
      {piece.look === "test" ? (
        <span className="flow-diamond" aria-hidden="true">
          ◆
        </span>
      ) : null}
      <span className="flow-head-text" style={{ width: piece.textWidth }} title={cut ? whole : undefined}>
        {words}
      </span>
      <Chips boxes={piece.chips} unknown={piece.unknown} path={piece.path} handlers={handlers} />
      {lane ? <ViewCode handlers={handlers} selection={{ kind: "block", key: lane.key }} /> : null}
    </div>
  );
}

function SectionPiece({ piece, handlers }: { readonly piece: Extract<FlowPiece, { kind: "section" }>; readonly handlers: Handlers }) {
  const { lane } = piece;
  const words = sectionWords(lane, piece.loop);
  const chips = lane.test?.calls ?? [];
  const unknown = lane.test?.unknown ?? [];
  return (
    <section className={classes("flow-section", piece.look, piece.dim && "dim", isBlock(handlers.selected, lane.key) && "selected")} style={place(piece.rect)} data-key={lane.key} {...selectable(handlers, { kind: "block", key: lane.key })}>
      <div className={classes("flow-section-title", chips.length + unknown.length > 0 && "with-chips")}>
        <span className="flow-section-word">{words.title}</span>
        {lane.name !== undefined ? <span className="flow-name">{lane.name}</span> : null}
        {words.clause !== null ? <span className="flow-frame-header">{words.clause}</span> : null}
        <Chips boxes={chips} unknown={unknown} path={piece.path} handlers={handlers} />
        {words.note !== null ? <span className="flow-section-note">{words.note}</span> : null}
      </div>
      <ViewCode handlers={handlers} selection={{ kind: "block", key: lane.key }} />
    </section>
  );
}

function CombinedPiece({
  piece,
  handlers,
  onToggleCombined,
}: {
  readonly piece: Extract<FlowPiece, { kind: "combined" }>;
  readonly handlers: Handlers;
  readonly onToggleCombined: ((item: string, collapsed: boolean) => void) | undefined;
}) {
  const { step } = piece;
  const selected = handlers.selected?.kind === "combined" && handlers.selected.item === step.item;
  const toggle = onToggleCombined ? (
    <button
      type="button"
      className="flow-combined-toggle nodrag"
      aria-expanded={!step.collapsed}
      onClick={(event) => {
        stop(event);
        onToggleCombined(step.item, !step.collapsed);
      }}
    >
      {step.collapsed ? "open" : "fold"}
    </button>
  ) : null;
  const head = (
    <div className="flow-combined-head">
      <span className="flow-combined-glyph" aria-hidden="true">
        ⊞
      </span>
      <span className="flow-combined-title">{step.title}</span>
      {toggle}
    </div>
  );
  const summary = combinedSummary(step);
  return (
    <div
      className={classes("flow-combined", step.collapsed ? "collapsed" : "open", piece.dim && "dim", selected && "selected")}
      style={place(piece.rect)}
      data-item={step.item}
      {...selectable(handlers, { kind: "combined", item: step.item })}
    >
      {head}
      {step.collapsed && summary.length > 0 ? (
        <div className="flow-combined-summary">
          {summary.map((item) => (
            <span key={item.text} className={item.tone}>
              {item.text}
              {item.tone === "unknown" ? <Unknown /> : null}
            </span>
          ))}
        </div>
      ) : null}
      {piece.exits.length > 0 ? (
        <div className="flow-combined-exits">
          {piece.exits.map((exit) => (
            <span key={`${exit.kind} ${exit.destination}`} className={`flow-combined-exit r-${exit.kind}`}>
              <b>{exit.verb}</b>→ {exit.destination}
            </span>
          ))}
        </div>
      ) : null}
      <ViewCode handlers={handlers} selection={{ kind: "combined", item: step.item }} />
    </div>
  );
}

function Piece({ piece, handlers, props }: { readonly piece: FlowPiece; readonly handlers: Handlers; readonly props: FlowViewProps }): ReactNode {
  switch (piece.kind) {
    case "start":
      return (
        <div
          className={classes("flow-pill", "start", piece.named && "named", isBlock(handlers.selected, piece.key) && "selected")}
          style={place(piece.rect)}
          data-key={piece.key}
          {...selectable(handlers, { kind: "block", key: piece.key })}
        >
          {piece.label}
        </div>
      );
    case "end":
      return (
        <div className="flow-pill end" style={place(piece.rect)}>
          End
        </div>
      );
    case "call":
      return <CallPiece piece={piece} handlers={handlers} />;
    case "callback": {
      const { callback } = piece;
      return (
        <div
          className={classes("flow-callback", callback.inset !== undefined && "open", piece.dim && "dim", isBlock(handlers.selected, callback.key) && "selected")}
          style={place(piece.rect)}
          data-key={callback.key}
          {...selectable(handlers, { kind: "block", key: callback.key })}
        >
          <span className="flow-callback-word">callback</span>
          {callback.name !== undefined ? <span className="flow-name">{callback.name}</span> : null}
          <span className="flow-callback-text">{callbackText(callback)}</span>
          {callback.actions.open ? <Chevron open={callback.inset !== undefined} name={callback.name ?? callback.label} onToggle={() => {
            handlers.toggleOpen(piece.path);
          }} /> : null}
          <ViewCode handlers={handlers} selection={{ kind: "block", key: callback.key }} />
        </div>
      );
    }
    case "thin":
      return <ThinPiece piece={piece} handlers={handlers} onToggleRow={props.onToggleRow} onViewRow={props.onViewRow} />;
    case "exit":
      return <ExitPiece piece={piece} handlers={handlers} />;
    case "frame":
      return <FramePiece piece={piece} handlers={handlers} />;
    case "head":
      return <HeadPiece piece={piece} handlers={handlers} />;
    case "section":
      return <SectionPiece piece={piece} handlers={handlers} />;
    case "combined":
      return <CombinedPiece piece={piece} handlers={handlers} onToggleCombined={props.onToggleCombined} />;
    case "proposed": {
      const selected = handlers.selected?.kind === "proposed" && handlers.selected.item === piece.step.item;
      return (
        <div className={classes("flow-proposed", piece.dim && "dim", selected && "selected")} style={place(piece.rect)} data-item={piece.step.item} {...selectable(handlers, { kind: "proposed", item: piece.step.item })}>
          <span className="flow-proposed-word">Proposed</span>
          <span className="flow-proposed-title">{piece.step.title}</span>
        </div>
      );
    }
    case "inset": {
      const { header } = piece.flow;
      return (
        <div className={classes("flow-inset", piece.dim && "dim")} style={place(piece.rect)}>
          <div className={classes("flow-inset-title", isBlock(handlers.selected, header.key) && "selected")} data-key={header.key} {...selectable(handlers, { kind: "block", key: header.key })}>
            <span className={classes("flow-inset-name", header.name !== undefined && "named")}>{header.name ?? header.label}</span>
            <span className="flow-inset-place">{insetPlace(piece.flow)}</span>
            <ViewCode handlers={handlers} selection={{ kind: "block", key: header.key }} />
          </div>
        </div>
      );
    }
    case "gap": {
      const { gap } = piece;
      const { onOpenLines } = props;
      // Until something opens its lines, a tick is only words.
      return onOpenLines ? (
        <button
          type="button"
          className="flow-gap nodrag"
          style={place(piece.rect)}
          onClick={(event) => {
            stop(event);
            onOpenLines(piece.path, gap.lines);
          }}
        >
          {gapWords(gap)}
        </button>
      ) : (
        <span className="flow-gap" style={place(piece.rect)}>
          {gapWords(gap)}
        </span>
      );
    }
    case "note":
      return (
        <div className={classes("flow-note", piece.tone, piece.dim && "dim")} style={place(piece.rect)}>
          {piece.text}
        </div>
      );
  }
}

/** The arrowhead at the end of a polyline: a 6px-long, 9px-wide triangle along its last segment. */
function arrowhead(points: readonly Point[]): string {
  const tip = points.at(-1);
  const before = points.at(-2);
  if (!tip || !before) {
    return "";
  }
  const length = Math.hypot(tip.x - before.x, tip.y - before.y) || 1;
  const dx = (tip.x - before.x) / length;
  const dy = (tip.y - before.y) / length;
  const base = { x: tip.x - dx * 6, y: tip.y - dy * 6 };
  return `${tip.x},${tip.y} ${base.x - dy * 4.5},${base.y + dx * 4.5} ${base.x + dy * 4.5},${base.y - dx * 4.5}`;
}

function WireView({ wire }: { readonly wire: FlowWire }) {
  const d = wire.points.map((point, index) => `${index === 0 ? "M" : "L"}${point.x} ${point.y}`).join(" ");
  return (
    <g className={classes("flow-wire", wire.dashed && "dashed", wire.error && "error", wire.routes.length === 0 && "bare")}>
      <path d={d} />
      <polygon points={arrowhead(wire.points)} />
      {wire.label ? (
        <text x={wire.label.at.x} y={wire.label.at.y} textAnchor={wire.label.anchor}>
          {wire.label.text}
        </text>
      ) : null}
    </g>
  );
}

/** The gap tick of a flow that counts a line. */
function gapRect(geometry: FlowGeometry, at: FlowLine): Rect | undefined {
  for (const piece of geometry.pieces) {
    if (piece.kind === "gap" && piece.path === at.path && piece.gap.lines[0] <= at.line && at.line <= piece.gap.lines[1]) {
      return piece.rect;
    }
  }
  return undefined;
}

/** The box of the first of `keys` the flow draws. */
function drawnRect(geometry: FlowGeometry, keys: readonly (Key | null)[]): Rect | undefined {
  for (const key of keys) {
    const rect = key !== null ? geometry.blocks.get(key) : undefined;
    if (rect) {
      return rect;
    }
  }
  return undefined;
}

function outlineRect(geometry: FlowGeometry, outline: FlowOutline): Rect | undefined {
  return "line" in outline ? gapRect(geometry, outline.line) : geometry.blocks.get(outline.block);
}

/** The drawn box that holds a selection: its own, or the one holding it. */
function selectedRect(geometry: FlowGeometry, selected: FlowSelection | null): Rect | undefined {
  if (!selected) {
    return undefined;
  }
  return selected.kind === "block" ? geometry.blocks.get(selected.key) : geometry.items.get(selected.item);
}

/** The corners an outline takes, from the piece it goes around. */
const OUTLINE_SHAPES: Readonly<Partial<Record<FlowPiece["kind"], string>>> = {
  start: "round",
  end: "round",
  exit: "round",
  frame: "card",
  section: "tab",
  combined: "tab",
  inset: "inset",
  gap: "tick",
  call: "box",
  head: "box",
  proposed: "box",
};

function Outline({ geometry, rect, lit }: { readonly geometry: FlowGeometry; readonly rect: Rect; readonly lit: boolean }) {
  const kind = geometry.pieces.find((piece) => piece.rect === rect)?.kind;
  const shape = (kind && OUTLINE_SHAPES[kind]) ?? "chip";
  return <div className={classes("flow-outline", shape, lit && "lit")} style={place(rect)} aria-hidden="true" />;
}

/** A drawn flow, laid out by `layoutFlow`: its pieces at their rects and its wires over them. */
export function FlowView(props: FlowViewProps) {
  const { geometry, selected = null, outline = null, hover = null, onSelect, onToggleOpen, onViewCode, onBlockMenu } = props;
  const handlers: Handlers = {
    selected,
    select: (selection) => onSelect?.(selection),
    toggleOpen: (path) => onToggleOpen?.(path),
    viewCode: onViewCode,
    menu: onBlockMenu,
  };
  const outlined = selectedRect(geometry, selected) ?? (outline ? outlineRect(geometry, outline) : undefined);
  const lit = hover ? (drawnRect(geometry, [hover.key, hover.block]) ?? gapRect(geometry, hover)) : undefined;
  return (
    <div className="flow" style={{ width: geometry.width, height: geometry.height }}>
      {/* A geometry's pieces and wires are a list in paint order: their place in it is their identity. */}
      {geometry.pieces.map((piece, index) => (
        <Piece key={index} piece={piece} handlers={handlers} props={props} />
      ))}
      {lit && lit !== outlined ? <Outline geometry={geometry} rect={lit} lit /> : null}
      {outlined ? <Outline geometry={geometry} rect={outlined} lit={false} /> : null}
      <svg className="flow-wires" width={geometry.width} height={geometry.height} aria-hidden="true">
        {geometry.wires.map((wire, index) => (
          <WireView key={index} wire={wire} />
        ))}
      </svg>
    </div>
  );
}
