/**
 * On this page: the page's cards in follow order, indented by call depth, with their groups, proposed
 * steps ("proposed"), drawn links, and connections, then the resource cards, frames as headings over
 * what they hold, a review mark where something needs review, and "collapsed" on a collapsed frame.
 * A click brings the card into view (a focus move: Back returns), selects a group, selects a proposed
 * step and brings its card into view, selects a link, or selects a frame or a resource card and brings
 * it into view. The last line steps through the Flow nodes still showing code names.
 */

import { useMemo, type RefObject } from "react";
import { KindGlyph, SectionLabel, TextButton } from "../components";
import { plural } from "../lib/format";
import { RESOURCE_ICONS } from "../canvas/resources/kinds";
import { findPage, focusCard, focusFrame, focusItem, pushFocusMove, requestCardFocus, select, useOpenBoard, usePageView, type PageTarget, type Selection } from "../state/boards";
import { pageRows, type PageRow } from "./pageRows";
import { stepUnlabeled, useUnlabeled } from "./unlabeled";
import { useWindowed } from "./windowed";

const ROW_HEIGHT = 28;
/** Indentation per level of call depth. */
const INDENT = 16;

function isSelected(row: PageRow, selection: Selection): boolean {
  switch (row.kind) {
    case "card":
      return selection.kind === "card" && selection.cardId === row.id;
    case "group":
      return selection.kind === "group" && selection.groupId === row.id;
    case "proposal":
      return selection.kind === "proposal" && selection.proposalId === row.id;
    case "link":
      return selection.kind === "link" && selection.linkId === row.id;
    case "frame":
      return selection.kind === "frame" && selection.frameId === row.id;
    case "resource":
      return selection.kind === "resource" && selection.resourceId === row.id;
  }
}

function activate(target: PageTarget, row: PageRow): void {
  if (row.kind === "card") {
    pushFocusMove(target, [{ cardId: row.id, label: row.name }]);
    focusCard(target, row.id);
  } else if (row.kind === "group") {
    select(target, { kind: "group", groupId: row.id });
    requestCardFocus(target, [row.group.card], "center");
  } else if (row.kind === "proposal") {
    select(target, { kind: "proposal", proposalId: row.id });
    requestCardFocus(target, [row.proposal.card], "center");
  } else if (row.kind === "frame") {
    focusFrame(target, row.id);
  } else if (row.kind === "resource") {
    focusItem(target, row.id);
  } else {
    select(target, { kind: "link", linkId: row.id });
    requestCardFocus(target, [row.link.from, row.link.to], "fit");
  }
}

function LayerRow({ target, row, selected }: { readonly target: PageTarget; readonly row: PageRow; readonly selected: boolean }) {
  const icon = row.kind === "frame" && row.frame.icon !== undefined ? (`frame-${row.frame.icon}` as const) : row.kind === "resource" ? RESOURCE_ICONS[row.resource.kind] : undefined;
  const label =
    row.kind === "card"
      ? row.name
      : row.kind === "group"
        ? row.group.title
        : row.kind === "proposal"
          ? row.proposal.title
          : row.kind === "frame"
            ? row.frame.title
            : row.kind === "resource"
              ? row.resource.name
              : row.label;
  const note = row.kind === "frame" && row.frame.collapsed === true ? "collapsed" : row.kind === "proposal" ? "proposed" : null;
  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={0}
      className={`layer-row ${row.kind}${selected ? " selected" : ""}`}
      data-row-id={row.id}
      onClick={() => {
        activate(target, row);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          activate(target, row);
        }
      }}
    >
      <span className="layer-glyph" style={row.depth > 0 ? { marginLeft: row.depth * INDENT } : undefined}>
        {row.kind === "proposal" ? (
          <span className="layer-proposal-glyph" aria-hidden="true">
            <span className="proposal-glyph" />
          </span>
        ) : (
          <KindGlyph kind={row.kind === "card" ? (row.entrance ? "entrance" : "function") : row.kind} {...(icon !== undefined ? { icon } : {})} selected={selected} />
        )}
      </span>
      <span className="layer-label" title={label}>
        {label}
      </span>
      {note !== null ? <span className="layer-note">{note}</span> : null}
      <span className="layer-trailing">{row.review ? <span className="board-review-dot" aria-label="Needs review" /> : null}</span>
    </div>
  );
}

export function OnThisPage({ target, scroller }: { readonly target: PageTarget; readonly scroller: RefObject<HTMLElement | null> }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const outline = useOpenBoard(target.board, (board) => board.outlines[target.page]);
  const health = useOpenBoard(target.board, (board) => board.health);
  const selection = usePageView(target, (view) => view.selection);
  const rows = useMemo(() => (page ? pageRows(page, outline, health?.items ?? []) : []), [page, outline, health]);
  const unlabeled = useUnlabeled(target);
  const { listRef, rows: windowRows, height, offsetOf } = useWindowed(rows.length, ROW_HEIGHT, scroller);
  const row = (item: PageRow) => <LayerRow key={item.id} target={target} row={item} selected={isSelected(item, selection)} />;

  return (
    <section className="board-section" aria-label="On this page">
      <SectionLabel trailing="follow order">On this page</SectionLabel>
      {rows.length === 0 ? <p className="board-section-hint">No cards yet. Add one with A.</p> : null}
      <div
        ref={listRef}
        role="listbox"
        aria-label="Cards on this page"
        className="layer-list"
        style={windowRows ? { height } : undefined}
      >
        {windowRows
          ? windowRows.map((virtual) => {
              const item = rows[virtual.index];
              return item ? (
                <div key={item.id} className="layer-slot" style={{ transform: `translateY(${offsetOf(virtual)}px)` }}>
                  {row(item)}
                </div>
              ) : null;
            })
          : rows.map(row)}
      </div>
      {unlabeled.length > 0 ? (
        <p className="board-unlabeled">
          <span>{plural(unlabeled.length, "Flow step")} unlabeled ·</span>
          <TextButton
            onClick={() => {
              stepUnlabeled(target, unlabeled);
            }}
          >
            Label them
          </TextButton>
        </p>
      ) : null}
    </section>
  );
}
