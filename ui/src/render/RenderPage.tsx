/**
 * One page drawn for the main process: the page's canvas exactly as the board view draws it (the same
 * canvas, cards, arrows, and labels), with nothing around it and nothing to edit. Once every drawn card
 * is measured, every query has loaded, the fonts are in, and nothing has moved for a moment, it
 * answers: the bounds an image captures, or the page checks. For an image, it then takes the camera the
 * main process framed it with, in the window sized for it, and says when that is drawn.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useReactFlow, useStoreApi } from "@xyflow/react";
import type { RenderFrame, RenderJob } from "@unfold/desktop/render-api";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { pageChecks } from "../board/checks";
import { BoardCanvas, useBoardArrows } from "../canvas";
import { collapsedPlaces } from "../canvas/frames/geometry";
import type { Card, Page } from "../ipc/generated";
import { cardKey, cardRefKey, findPage, useOpenBoard } from "../state/boards";
import { resolveRefProject, useProjectIndex } from "../state/workspace";
import { drawnGeometry, drawnNodeIds, unlabeledSteps } from "./geometry";
import { answer, useRenderJob } from "./jobs";
import { whenQuiet } from "./quiet";
import { imageBounds } from "./target";

/** How long a page may take to load and settle. */
const READY_TIMEOUT_MS = 20_000;
/** How long nothing may move before the page counts as drawn. */
const QUIET_MS = 250;
/** How long the window may take to reach the size the main process gave it. */
const RESIZE_MS = 2_000;
/** How long the framed page may take to settle. */
const FRAME_TIMEOUT_MS = 8_000;

const EMPTY_CARDS: readonly Card[] = [];

/** Whether the bundled fonts have loaded (a test's DOM has no font set). */
function fontsLoaded(): boolean {
  const fonts = document.fonts as FontFaceSet | undefined;
  return fonts === undefined || fonts.status === "loaded";
}

function timedOut(what: string): { readonly ok: false; readonly code: string; readonly message: string } {
  return { ok: false, code: "render_timeout", message: `The page didn't finish drawing ${what}.` };
}

export function RenderPage({ job }: { readonly job: RenderJob }) {
  const container = useRef<HTMLDivElement>(null);
  const store = useStoreApi();
  const flow = useReactFlow();
  const client = useQueryClient();
  const target = useMemo(() => ({ board: { project: job.board.project, id: job.board.id }, page: job.page.id }), [job]);
  const cards = useOpenBoard(target.board, (open) => findPage(open.document, target.page)?.cards) ?? EMPTY_CARDS;
  const frames = useOpenBoard(target.board, (open) => findPage(open.document, target.page)?.frames);
  const resources = useOpenBoard(target.board, (open) => findPage(open.document, target.page)?.resources);
  const notes = useOpenBoard(target.board, (open) => findPage(open.document, target.page)?.notes);
  const links = useOpenBoard(target.board, (open) => findPage(open.document, target.page)?.links);
  const boardArrows = useBoardArrows(target, cards);
  const frame = useRenderJob((state) => state.frame);
  const index = useProjectIndex();

  // Read once the page is drawn, by functions that must stay the same while it loads.
  const settled = useRef(boardArrows.settled);
  const models = useRef(boardArrows.models);
  const projects = useRef(index);
  useLayoutEffect(() => {
    settled.current = boardArrows.settled;
    models.current = boardArrows.models;
    projects.current = index;
  });

  // The cards drawn as themselves: a collapsed frame draws the ones it holds as its own node.
  const shownCards = useMemo(() => {
    const page = job.page as unknown as Page;
    const hidden = collapsedPlaces(page);
    return page.cards.filter((card) => !hidden.has(card.id)).length;
  }, [job]);

  /** What is drawn now, as one string; null while anything still loads or isn't measured. */
  const probe = useCallback((): string | null => {
    const element = container.current;
    if (!element || !settled.current || client.isFetching() > 0 || !fontsLoaded()) {
      return null;
    }
    const { nodeLookup, transform, width, height, edges } = store.getState();
    if (nodeLookup.size < shownCards) {
      return null;
    }
    const drawn = drawnNodeIds(element);
    const parts: string[] = [];
    for (const node of nodeLookup.values()) {
      if (node.hidden === true || !drawn.has(node.id)) {
        continue;
      }
      const { width: nodeWidth, height: nodeHeight } = node.measured;
      if (!nodeWidth || !nodeHeight) {
        return null;
      }
      const { x, y } = node.internals.positionAbsolute;
      parts.push(`${node.id}@${x},${y},${nodeWidth},${nodeHeight}`);
    }
    parts.push(transform.join(","), `${width}x${height}`, String(edges.length), String(element.querySelectorAll(".react-flow__edge").length));
    return parts.join("|");
  }, [client, store, shownCards]);

  // The page drawn: answer with what to capture, or with the checks.
  useEffect(() => {
    const controller = new AbortController();
    void whenQuiet(probe, { quietMs: QUIET_MS, timeoutMs: READY_TIMEOUT_MS, signal: controller.signal }).then((quiet) => {
      const element = container.current;
      if (controller.signal.aborted || !element) {
        return;
      }
      if (!quiet) {
        answer(job.requestId, timedOut(`within ${READY_TIMEOUT_MS / 1000} s`));
        return;
      }
      const page = job.page as unknown as Page;
      const unlabeledOf = (id: string) => {
        const card = page.cards.find((item) => item.id === id);
        const project = card ? resolveRefProject(projects.current, job.board.project, card.ref.project) : null;
        return card && project !== null ? unlabeledSteps(page, card, models.current.get(cardRefKey(project, cardKey(card)))) : 0;
      };
      const geometry = drawnGeometry(element, store.getState(), unlabeledOf);
      answer(job.requestId, job.purpose === "check" ? { ok: true, checks: pageChecks(page, geometry) } : imageBounds(page, geometry, job.item));
    });
    return () => {
      controller.abort();
    };
  }, [job, probe, store]);

  // Framed for its image: once the window is the size asked for, take the camera and settle again.
  useEffect(() => {
    if (!frame) {
      return undefined;
    }
    const controller = new AbortController();
    const { signal } = controller;
    const sized = (asked: RenderFrame) => {
      const { width, height } = store.getState();
      return Math.abs(width - asked.width) < 1 && Math.abs(height - asked.height) < 1 ? "sized" : null;
    };
    void (async () => {
      await whenQuiet(() => sized(frame), { quietMs: 0, timeoutMs: RESIZE_MS, signal });
      await flow.setViewport(frame.camera);
      const drawn = await whenQuiet(probe, { quietMs: QUIET_MS, timeoutMs: FRAME_TIMEOUT_MS, signal });
      if (!signal.aborted) {
        answer(frame.requestId, drawn ? { ok: true } : timedOut("once framed"));
      }
    })();
    return () => {
      controller.abort();
    };
  }, [frame, flow, probe, store]);

  return (
    <div ref={container} className="render-stage">
      <BoardCanvas target={target} cards={cards} frames={frames} resources={resources} notes={notes} links={links} boardArrows={boardArrows} />
    </div>
  );
}
