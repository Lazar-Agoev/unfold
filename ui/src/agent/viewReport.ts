/**
 * What this window shows, reported to the main process (`report_view`, design §5.6) so agents work on
 * the view of the window used last by default: the active board and page, the selection, the lens,
 * the items in view, and the board tabs (the boards this window holds). Sent ~150 ms after any of them
 * changes, and at once when the window gains focus.
 */

import type { ViewReport, ViewSelection } from "@unfold/desktop/agent-api";
import type { Rect, Size } from "../canvas/layout";
import { windowCommand, type Page } from "../ipc";
import { displayedRects, findPage, getOpenBoard, getPageView, useBoards, type Camera, type Selection } from "../state/boards";
import { getActiveBoard, useTabs } from "../state/tabs";

/** How long the window waits after a change before reporting its view. */
export const REPORT_DELAY_MS = 150;

function viewSelection(selection: Selection): ViewSelection {
  switch (selection.kind) {
    case "none":
      return { kind: "none" };
    case "card":
      return selection.key !== null ? { kind: "step", card: selection.cardId, key: selection.key } : { kind: "card", id: selection.cardId };
    case "group":
      return { kind: "group", id: selection.groupId };
    case "link":
      return { kind: "link", id: selection.linkId };
    case "line":
      return { kind: "line", from: selection.from, to: selection.to, project: selection.project, source: selection.source, seq: selection.seq, line: selection.line };
    case "calls":
      return { kind: "calls", from: selection.from, to: selection.to, project: selection.project, calls: selection.calls };
  }
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * The ids of a page's cards, step groups (their card in view), and links (an end in view) that the
 * canvas shows through `camera` at `size`; all of them before the canvas has a camera or a size.
 */
export function visibleItems(page: Page, rects: ReadonlyMap<string, Rect>, camera: Camera | null, size: Size | null): string[] {
  const area = camera && size ? { x: -camera.x / camera.zoom, y: -camera.y / camera.zoom, width: size.width / camera.zoom, height: size.height / camera.zoom } : null;
  const cards = new Set(
    page.cards.flatMap((card) => {
      const rect = rects.get(card.id);
      return !area || (rect && overlaps(rect, area)) ? [card.id] : [];
    }),
  );
  return [
    ...cards,
    ...page.groups.flatMap((group) => (cards.has(group.card) ? [group.id] : [])),
    ...page.links.flatMap((link) => (cards.has(link.from) || cards.has(link.to) ? [link.id] : [])),
  ];
}

/** The window's view now; `canvas` is the canvas's size on screen (null when none is shown). */
export function currentView(canvas: Size | null): ViewReport {
  const tabs = useTabs.getState().boards.map((ref) => ({ project: ref.project, id: ref.id }));
  const ref = getActiveBoard();
  const board = ref ? getOpenBoard(ref) : undefined;
  const page = board ? findPage(board.document, board.activePage) : undefined;
  if (!ref || !board || !page) {
    return { selection: { kind: "none" }, visible: [], tabs };
  }
  const view = getPageView({ board: ref, page: page.id });
  return {
    board: { project: ref.project, id: ref.id },
    page: page.id,
    selection: viewSelection(view.selection),
    lens: view.lens ?? page.defaultView,
    visible: visibleItems(page, displayedRects(board, page.id), view.camera, canvas),
    tabs,
  };
}

/** The board canvas's size on screen, or null when no board is shown. */
function canvasSize(): Size | null {
  const box = document.querySelector(".board-canvas")?.getBoundingClientRect();
  return box && box.width > 0 && box.height > 0 ? { width: box.width, height: box.height } : null;
}

/** Reports the window's view as it changes and when the window gains focus. Returns the uninstall function. */
export function installViewReports(): () => void {
  let sent = "";
  let timer: ReturnType<typeof setTimeout> | null = null;
  const send = () => {
    timer = null;
    const report = currentView(canvasSize());
    const text = JSON.stringify(report);
    if (text !== sent) {
      sent = text;
      windowCommand("report_view", report).catch((error: unknown) => {
        console.warn("Reporting the window's view failed:", error);
      });
    }
  };
  const schedule = () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = setTimeout(send, REPORT_DELAY_MS);
  };
  const onFocus = () => {
    sent = "";
    send();
  };
  const stops = [useTabs.subscribe(schedule), useBoards.subscribe(schedule)];
  window.addEventListener("focus", onFocus);
  window.addEventListener("resize", schedule);
  schedule();
  return () => {
    if (timer !== null) {
      clearTimeout(timer);
    }
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("resize", schedule);
    for (const stop of stops) {
      stop();
    }
  };
}
