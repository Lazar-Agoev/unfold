/**
 * What this window shows, reported to the main process (`report_view`) so agents work on
 * its view by default: the shown board and page, the selection, what the code panel shows (its block
 * and lines), the items in view, and the boards whose sessions the window
 * holds (`held`). Sent ~150 ms after any of them changes, and at once when the window gains focus.
 */

import type { QueryKey } from "@tanstack/react-query";
import type { CodePanelReport, ViewReport, ViewSelection } from "@unfold/desktop/agent-api";
import type { Rect, Size } from "../canvas/layout";
import { codeViewArgs } from "../code/view";
import { engineKey, queryClient, windowCommand, type CallBox, type CardView, type CodeView, type DrawnFlow, type Key, type Page, type Sequence } from "../ipc";
import { getCodeOpen, onCodePanelChange, type CodeOpen } from "../state/codePanel";
import { displayedRects, findPage, getOpenBoard, getPageView, useBoards, type Camera, type PageTarget, type Selection } from "../state/boards";
import { cardArgsOf, cardQuery, flowBlocks, overlayIds } from "../state/cards";
import { getShownBoard, useShownBoardStore } from "../state/shownBoard";

/** How long the window waits after a change before reporting its view. */
export const REPORT_DELAY_MS = 150;

/** The flows a drawn flow opens in place itself (not those they open in turn). */
function openedFlows(flow: DrawnFlow): DrawnFlow[] {
  const box = (call: CallBox): DrawnFlow[] => [call.inset, ...call.callbacks.map((callback) => callback.inset)].filter((inset) => inset !== undefined);
  const sequence = (steps: Sequence): DrawnFlow[] =>
    steps.steps.flatMap((step): DrawnFlow[] => {
      switch (step.kind) {
        case "call":
          return box(step);
        case "decision":
        case "loop":
        case "try":
        case "with":
          return [...step.headerCalls.flatMap(box), ...step.lanes.flatMap((lane) => [...(lane.test?.calls ?? []).flatMap(box), ...sequence(lane.steps)])];
        case "exit":
          return step.value?.kind === "call" ? box(step.value) : [];
        case "combined":
          return step.steps ? sequence(step.steps) : [];
        case "thin":
        case "proposed":
          return [];
      }
    });
  return sequence(flow.steps);
}

/** The definition whose drawn flow holds block `key` as its own: the flow's, or that of a call it opens in place. */
function stepOwner(flow: DrawnFlow, key: Key): Key | undefined {
  for (const inset of openedFlows(flow)) {
    const owner = stepOwner(inset, key);
    if (owner !== undefined) {
      return owner;
    }
  }
  return flowBlocks(flow).some((item) => item.key === key) ? flow.header.key : undefined;
}

/**
 * The selection as agents read it: a card selected at its own block (a click on its header), or at a
 * field of it, is the card; a step shown in a call the card opens in place names the definition that
 * owns it, from the card as `drawn` draws it.
 */
function viewSelection(page: Page, selection: Selection, drawn: (cardId: string) => CardView | undefined): ViewSelection {
  switch (selection.kind) {
    case "none":
      return { kind: "none" };
    case "card": {
      const ref = page.cards.find((card) => card.id === selection.cardId)?.ref;
      const own = ref !== undefined && "block" in ref && ref.block === selection.key;
      if (selection.key === null || own) {
        return { kind: "card", id: selection.cardId };
      }
      const view = drawn(selection.cardId);
      const flow = view?.type === "definition" ? view.flow : undefined;
      const owner = flow ? stepOwner(flow, selection.key) : undefined;
      return { kind: "step", card: selection.cardId, key: selection.key, ...(owner !== undefined && owner !== flow?.header.key ? { owner } : {}) };
    }
    case "field":
      return { kind: "card", id: selection.cardId };
    case "combined":
      return { kind: "combined", id: selection.combinedId };
    case "link":
      return { kind: "link", id: selection.linkId };
    case "frame":
      return { kind: "frame", id: selection.frameId };
    case "resource":
      return { kind: "resource", id: selection.resourceId };
    case "note":
      return { kind: "note", id: selection.noteId };
    case "line":
      return { kind: "arrow", from: selection.from, to: selection.to, calls: [selection.call] };
    case "calls":
      return { kind: "arrow", from: selection.from, to: selection.to, calls: selection.calls };
    case "proposal":
      return { kind: "proposal", id: selection.proposalId };
  }
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * The ids of a page's cards, combined and proposed steps (a card in view draws them, as `drawn` says),
 * connections (an end in view), frames, resource cards, and notes that the canvas shows through
 * `camera` at `size` (`rects` holds what is drawn: a card inside a collapsed frame isn't); all of them
 * before the canvas has a camera or a size.
 */
export function visibleItems(
  page: Page,
  rects: ReadonlyMap<string, Rect>,
  camera: Camera | null,
  size: Size | null,
  drawn: (cardId: string) => CardView | undefined,
): string[] {
  const area = camera && size ? { x: -camera.x / camera.zoom, y: -camera.y / camera.zoom, width: size.width / camera.zoom, height: size.height / camera.zoom } : null;
  const shown = (id: string) => {
    const rect = rects.get(id);
    return rect !== undefined && (!area || overlaps(rect, area));
  };
  const cards = new Set(page.cards.flatMap((card) => (shown(card.id) ? [card.id] : [])));
  const frames = (page.frames ?? []).flatMap((frame) => (shown(frame.id) ? [frame.id] : []));
  const resources = (page.resources ?? []).flatMap((resource) => (shown(resource.id) ? [resource.id] : []));
  const ends = new Set([...cards, ...frames, ...resources]);
  const overlay = new Set([...cards].flatMap((card) => {
    const view = drawn(card);
    return view ? overlayIds(view) : [];
  }));
  return [
    ...cards,
    ...page.combined.flatMap((combined) => (overlay.has(combined.id) ? [combined.id] : [])),
    ...(page.proposals ?? []).flatMap((proposal) => (overlay.has(proposal.id) ? [proposal.id] : [])),
    ...page.links.flatMap((link) => (ends.has(link.from) || ends.has(link.to) ? [link.id] : [])),
    ...frames,
    ...resources,
    ...(page.notes ?? []).flatMap((note) => (shown(note.id) ? [note.id] : [])),
  ];
}

/** A card of a page as the window's cache holds it now. */
function loadedCard(target: PageTarget, cardId: string): CardView | undefined {
  const args = cardArgsOf(target, cardId);
  return args ? queryClient.getQueryData<CardView>(cardQuery(args).queryKey) : undefined;
}

/** What the code panel shows: what it opened on, and, once its view has loaded, the file and lines on screen. */
export function codeReport(open: CodeOpen, view: CodeView | undefined): CodePanelReport {
  const { target } = open;
  const first = view?.lines[0]?.number;
  const last = view?.lines.at(-1)?.number;
  const file = view?.file ?? (target.kind === "lines" || target.kind === "file" ? target.path : undefined);
  return {
    ...(target.kind === "block" || target.kind === "field" || target.kind === "start" ? { key: target.key } : {}),
    ...(target.kind === "lines" ? { span: target.lines } : {}),
    ...(file !== undefined ? { file } : {}),
    ...(first !== undefined && last !== undefined ? { lines: [first, last] as const } : {}),
    widened: open.show !== null,
    ...(view ? { state: view.state } : {}),
  };
}

/** What the code panel shows now, from the window's cache; absent while it is closed. */
function loadedCode(): CodePanelReport | undefined {
  const open = getCodeOpen();
  return open ? codeReport(open, queryClient.getQueryData<CodeView>(engineKey("code_view", codeViewArgs(open)))) : undefined;
}

/** The window's view now; `canvas` is the canvas's size on screen (null when none is shown). */
export function currentView(canvas: Size | null): ViewReport {
  const held = Object.values(useBoards.getState().boards).map(({ ref }) => ({ project: ref.project, id: ref.id }));
  const ref = getShownBoard();
  const board = ref ? getOpenBoard(ref) : undefined;
  const page = board ? findPage(board.document, board.activePage) : undefined;
  if (!ref || !board || !page) {
    return { selection: { kind: "none" }, visible: [], held };
  }
  const target: PageTarget = { board: ref, page: page.id };
  const view = getPageView(target);
  const code = loadedCode();
  const drawn = (cardId: string) => loadedCard(target, cardId);
  return {
    board: { project: ref.project, id: ref.id },
    page: page.id,
    selection: viewSelection(page, view.selection, drawn),
    ...(code ? { code } : {}),
    visible: visibleItems(page, displayedRects(board, page.id), view.camera, canvas, drawn),
    held,
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
  const stops = [
    useShownBoardStore.subscribe(schedule),
    useBoards.subscribe(schedule),
    onCodePanelChange(schedule),
    // The code panel's lines arrive with its view.
    queryClient.getQueryCache().subscribe((event) => {
      if (event.type === "updated" && (event.query.queryKey as QueryKey)[1] === "code_view") {
        schedule();
      }
    }),
  ];
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
