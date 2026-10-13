/**
 * The canvas top bar: the page's name and the board's publish state on the left (and "Claude is
 * working…" while an agent works on the page), the page's Diagram | Code lens centered over the
 * uncovered canvas, and Share and Publish on the right. Share opens the share
 * menu; Publish opens the publish dialog. On a canvas too narrow to center the lens between its
 * neighbours, it moves aside and the title and state shorten (`placeLens`).
 */

import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { Button, SegmentedControl, useContextMenu } from "../components";
import { Icon } from "../icons";
import { shareMenuEntries } from "../menus";
import { findPage, pageLens, setPageLens, useOpenBoard, usePageView, useWorkingAgents, workingText, type Lens, type PageTarget } from "../state/boards";
import { openDialog } from "../state/overlays";
import { CENTERED, placeLens, sameLensPlace, type LensPlace } from "./lensPlace";
import { usePublishState } from "./publishState";

/** The share menu's width. */
const SHARE_MENU_WIDTH = 196;

/** The page's two lenses, in order. */
const LENS_OPTIONS = [
  { value: "diagram", label: "Diagram" },
  { value: "code", label: "Code" },
] as const;

export function TopBar({ target }: { readonly target: PageTarget }) {
  const title = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.title) ?? "";
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const state = usePublishState(target.board);
  const working = useWorkingAgents(target);
  const share = useContextMenu("Share", SHARE_MENU_WIDTH);
  const header = useRef<HTMLElement>(null);
  const start = useRef<HTMLDivElement>(null);
  const lensBox = useRef<HTMLDivElement>(null);
  const shareButton = useRef<HTMLButtonElement>(null);
  const [place, setPlace] = useState<LensPlace>(CENTERED);

  // Places the lens (`placeLens`) on every render (the covered width and the text change with it) and
  // whenever the bar or the lens resizes.
  const measure = useCallback(() => {
    const bar = header.current;
    const left = start.current;
    if (!bar || !left || !lensBox.current || !shareButton.current) {
      return;
    }
    // Natural widths: the title and state may be shortened right now.
    const parts = [...left.children].map((child) => child.scrollWidth).filter((width) => width > 0);
    const gap = parseFloat(getComputedStyle(left).columnGap) || 0;
    const next = placeLens({
      width: bar.clientWidth,
      covered: parseFloat(getComputedStyle(bar).getPropertyValue("--covered-right")) || 0,
      lensWidth: lensBox.current.offsetWidth,
      leftEnd: left.offsetLeft + parts.reduce((sum, width) => sum + width, 0) + gap * Math.max(0, parts.length - 1),
      shareLeft: shareButton.current.offsetLeft,
    });
    setPlace((current) => (sameLensPlace(current, next) ? current : next));
  }, []);
  useLayoutEffect(() => {
    measure();
  });
  useLayoutEffect(() => {
    const observer = new ResizeObserver(measure);
    for (const element of [header.current, lensBox.current]) {
      if (element) {
        observer.observe(element);
      }
    }
    return () => {
      observer.disconnect();
    };
  }, [measure]);

  return (
    <header ref={header} className="board-topbar" style={place.room !== null ? { ["--topbar-left-room" as string]: `${place.room}px` } : undefined}>
      <div ref={start} className={`board-topbar-left${place.room !== null ? " narrow" : ""}`}>
        <h2 className="board-topbar-title">{title}</h2>
        <span className="board-publish-state">
          {state.dot !== null ? <span className={`board-publish-dot ${state.dot}`} aria-hidden="true" /> : null}
          <span className="board-publish-text">{state.text}</span>
        </span>
        {working.length > 0 ? (
          <span className="board-working" role="status">
            <Icon name="agent" size={10} />
            <span className="board-working-text">{workingText(working)}</span>
          </span>
        ) : null}
      </div>
      <span className="board-topbar-spacer" />
      <div ref={lensBox} className="board-lens" style={place.left !== null ? { left: place.left, translate: "none" } : undefined}>
        <SegmentedControl<Lens>
          options={LENS_OPTIONS}
          value={pageLens(lens, defaultView)}
          onChange={(next) => {
            setPageLens(target, next);
          }}
          label="Lens"
        />
      </div>
      <Button
        ref={shareButton}
        variant="secondary"
        trailingIcon="chevron-down"
        aria-haspopup="menu"
        onClick={(event) => {
          share.open(event.currentTarget, shareMenuEntries(target));
        }}
      >
        Share
      </Button>
      <Button
        variant="primary"
        disabled={!state.publishable}
        onClick={() => {
          openDialog({ kind: "publish", board: target.board });
        }}
      >
        Publish
      </Button>
      {share.element}
    </header>
  );
}

