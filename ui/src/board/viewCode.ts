/**
 * View code: where the actual source of what is selected
 * on a page is, as a peek sheet target with the lines (and, for a step, the exact expression) to
 * highlight. Every View code entry point resolves through `selectionSource`: the `</>` buttons, the
 * card and step menus, Selected's location links, and the sheet following the selection.
 *
 * - A function or type card opens its definition at its first line; a step, its recorded focus (a
 *   condition, a call's expression, else the statement) in the file of the block whose Flow shows it,
 *   so a step inside a call expanded in place opens the callee's file; a combined block, the span of
 *   its statements; a Shift-selected run, its lines.
 * - An entrance opens its declaration, registration, or configuration evidence; a resolved target of
 *   it opens that startup's or handler's definition instead (`entranceTargetSource`).
 * - A resource card or a connection opens the first source it cites.
 * - A proposed step has no code of its own: it opens its placement, the step it goes after (that step's
 *   focus), which Selected labels as placement code.
 *
 * It reads only the page and the card models already loaded. What it can't prove (an external or
 * unresolved target, a missing reference, a model not loaded) has no source, never a guessed one.
 * `sourceOf` and `combineOf` answer for an open page from the window's query cache; `viewCode` opens
 * the answer in the sheet, which then follows that page's selection (`useFollowSelection`). The sheet
 * keeps how to find the source again, so a recompile that moves the item moves its highlight too.
 */

import { useEffect } from "react";
import { cardQuery } from "../canvas/cardData";
import { shortPath } from "../code/text";
import { queryClient } from "../ipc/client";
import type { Anchor, BlockHeader, CardView, EvidenceView, Key, Page, ProjectId, ProjectName, TargetView } from "../ipc/generated";
import { cardKey, findCard, findPage, findProposal, findResource, getOpenBoard, getPageView, sameBoard, usePageView, type PageTarget, type Selection } from "../state/boards";
import { getPeekTarget, openPeek, type PeekTarget } from "../state/overlays";
import { getProjectIndex, resolveRefProject } from "../state/workspace";
import { combineChoice, stepRun, type CombineChoice } from "./combine";

type BlockView = Extract<CardView, { type: "block" }>;

/** What the resolver reads besides the page: projects as the board resolves them, and loaded card models. */
export interface SourceContext {
  /** The project a ref's project name (absent: the board's own) resolves to; null when it isn't connected. */
  readonly project: (name: ProjectName | undefined) => ProjectId | null;
  /** A card model, when it is loaded. */
  readonly model: (project: ProjectId, key: Key) => CardView | undefined;
}

/** Where something's source is, or why it has none. */
export type SourceResult = { readonly target: PeekTarget; readonly reason?: undefined } | { readonly target: null; readonly reason: string };

function found(target: PeekTarget): SourceResult {
  return { target };
}

function none(reason: string): SourceResult {
  return { target: null, reason };
}

/** Lines `startLine`–`endLine` of a file, opened at the first. */
function linesOf(project: ProjectId, path: string, startLine: number, endLine: number): PeekTarget {
  return { project, path, line: startLine, focus: { startLine, endLine: Math.max(startLine, endLine) } };
}

/** A definition's source: its file, focused on its first line. */
function definitionSource(project: ProjectId, header: Pick<BlockHeader, "path" | "startLine">): PeekTarget {
  return linesOf(project, header.path, header.startLine, header.startLine);
}

/** An entrance's declaration, registration, or configuration evidence: its file, at its lines when it has them. */
function evidenceSource(project: ProjectId, evidence: EvidenceView): PeekTarget {
  return evidence.startLine === null ? { project, path: evidence.path } : linesOf(project, evidence.path, evidence.startLine, evidence.endLine ?? evidence.startLine);
}

/** A resolved target of an entrance: that startup's or handler's definition, apart from the entrance's evidence. */
export function entranceTargetSource(project: ProjectId, target: TargetView): SourceResult {
  if (target.resolution === "external") {
    return none("An external target has no source in this project.");
  }
  if (target.resolution !== "resolved" || target.target === null) {
    return none("Unfold couldn't resolve this target, so it has no source to show.");
  }
  if (target.path === null) {
    return none("This target's source isn't available.");
  }
  return found(target.line !== null ? linesOf(project, target.path, target.line, target.line) : { project, path: target.path });
}

/** A step of `model`'s Flow (or, in Code, one of its statements): its recorded focus, else its lines. */
function stepSource(project: ProjectId, model: BlockView, key: Key): SourceResult {
  const { path } = model.header;
  const node = model.flow?.nodes.find((item) => item.key === key);
  if (node?.focus) {
    return found({ project, path, line: node.focus.startLine, focus: { ...node.focus } });
  }
  if (node?.startLine !== undefined) {
    return found(linesOf(project, path, node.startLine, node.endLine ?? node.startLine));
  }
  const item = model.code?.source.state === "current" ? model.code.source.outline.find((entry) => entry.key === key) : undefined;
  return item ? found(linesOf(project, path, item.startLine, item.endLine)) : none("This step has no recorded source.");
}

/** What a cited anchor opens: an evidence range at its lines, a key at its definition (or an entrance's evidence). */
export function anchorSource(project: ProjectId, anchor: Anchor, model: CardView | undefined): SourceResult {
  if ("file" in anchor) {
    return found(linesOf(project, anchor.file, anchor.lines[0], anchor.lines[1]));
  }
  if (model?.type === "block") {
    return found(definitionSource(project, model.header));
  }
  if (model?.type === "entrance") {
    return found(evidenceSource(project, model.evidence));
  }
  return none("What it cites isn't loaded.");
}

/** The first source an authored claim cites. */
function citedSource(anchors: readonly Anchor[] | undefined, context: SourceContext): SourceResult {
  const anchor = anchors?.[0];
  if (!anchor) {
    return none("It cites no source.");
  }
  const project = context.project(anchor.project);
  if (project === null) {
    return none("What it cites is in a project that isn't connected.");
  }
  return anchorSource(project, anchor, "key" in anchor ? context.model(project, anchor.key) : undefined);
}

function cardSource(page: Page, selection: Extract<Selection, { kind: "card" }>, context: SourceContext): SourceResult {
  const card = findCard(page, selection.cardId);
  if (!card) {
    return none("The card isn't on the page.");
  }
  const project = context.project(card.ref.project);
  if (project === null) {
    return none("Its project isn't connected.");
  }
  const key = cardKey(card);
  const model = context.model(project, key);
  if (!model) {
    return none("The card isn't loaded yet.");
  }
  if (model.type === "entrance") {
    return found(evidenceSource(project, model.evidence));
  }
  // A step shown in a call expanded in place: the callee's own source (calls never cross projects).
  if (selection.owner !== undefined) {
    const owner = context.model(project, selection.owner);
    if (owner?.type !== "block") {
      return none("The called function isn't loaded.");
    }
    return selection.key === null || selection.key === selection.owner ? found(definitionSource(project, owner.header)) : stepSource(project, owner, selection.key);
  }
  if (selection.key === null || selection.key === key) {
    return found(definitionSource(project, model.header));
  }
  if (selection.last !== undefined) {
    const run = stepRun(
      model,
      selection.key,
      selection.last,
      page.groups.filter((group) => group.card === card.id),
    );
    if (run) {
      return found(linesOf(project, model.header.path, run.startLine, run.endLine));
    }
  }
  return stepSource(project, model, selection.key);
}

/** A combined block (step group): the span of its statements in its function's file. */
function groupSource(page: Page, groupId: string, context: SourceContext): SourceResult {
  const group = page.groups.find((item) => item.id === groupId);
  const card = group ? findCard(page, group.card) : undefined;
  if (!group || !card) {
    return none("The combined block isn't on the page.");
  }
  const project = context.project(card.ref.project);
  const model = project !== null ? context.model(project, cardKey(card)) : undefined;
  const outline = model?.type === "block" && model.code?.source.state === "current" ? model.code.source.outline : null;
  const items = outline?.filter((item) => group.members.includes(item.key)) ?? [];
  if (project === null || model?.type !== "block" || items.length === 0) {
    return none("This combined block's code isn't available right now.");
  }
  return found(linesOf(project, model.header.path, Math.min(...items.map((item) => item.startLine)), Math.max(...items.map((item) => item.endLine))));
}

/** A proposed step's placement: the source of the step it goes after (it has no code of its own). */
function placementSource(page: Page, proposalId: string, context: SourceContext): SourceResult {
  const proposal = findProposal(page, proposalId);
  const card = proposal ? findCard(page, proposal.card) : undefined;
  if (!proposal || !card) {
    return none("The proposed step isn't on the page.");
  }
  const project = context.project(card.ref.project);
  if (project === null) {
    return none("Its project isn't connected.");
  }
  const model = context.model(project, cardKey(card));
  if (model?.type !== "block") {
    return none("The card isn't loaded yet.");
  }
  const result = stepSource(project, model, proposal.after);
  return result.target ? result : none("The step it goes after isn't in the code now. A proposed step has no code of its own.");
}

/** Where the source of `selection` on `page` is, or why it has none. */
export function selectionSource(page: Page, selection: Selection, context: SourceContext): SourceResult {
  switch (selection.kind) {
    case "card":
      return cardSource(page, selection, context);
    case "group":
      return groupSource(page, selection.groupId, context);
    case "resource":
      return citedSource(findResource(page, selection.resourceId)?.anchors, context);
    case "link":
      return citedSource(page.links.find((link) => link.id === selection.linkId)?.anchors, context);
    case "proposal":
      return placementSource(page, selection.proposalId, context);
    default:
      return none("Select a card, a step, or a combined block to view its code.");
  }
}

/**
 * The resolver's context for a board's pages now: the workspace's projects and the window's loaded card
 * models. `settled` leaves out a model that is refreshing (its data may be from an earlier compile).
 */
export function loadedContext(boardProject: ProjectId, settled = false): SourceContext {
  const index = getProjectIndex();
  return {
    project: (name) => resolveRefProject(index, boardProject, name),
    model: (project, key) => {
      const state = queryClient.getQueryState<CardView>(cardQuery(project, key).queryKey);
      return settled && (state?.isInvalidated === true || state?.fetchStatus === "fetching") ? undefined : state?.data;
    },
  };
}

function openPage(target: PageTarget): Page | undefined {
  const board = getOpenBoard(target.board);
  return board ? findPage(board.document, target.page) : undefined;
}

/** Where the source of `selection` (default: the page's selection) is, from what is loaded now. */
export function sourceOf(target: PageTarget, selection: Selection = getPageView(target).selection): SourceResult {
  const page = openPage(target);
  return page ? selectionSource(page, selection, loadedContext(target.board.project)) : none("The page isn't open.");
}

/** What Combine… does with `selection` (default: the page's selection), from what is loaded now. */
export function combineOf(target: PageTarget, selection: Selection = getPageView(target).selection): CombineChoice {
  const page = openPage(target);
  return page ? combineChoice(page, selection, loadedContext(target.board.project)) : { reason: "The page isn't open." };
}

/** Where the source of `selection` is on the settled card models, each time it's asked (`PeekTarget.resolve`). */
function resolverOf(target: PageTarget, selection: Selection): () => PeekTarget | null {
  return () => {
    const page = openPage(target);
    return page ? selectionSource(page, selection, loadedContext(target.board.project, true)).target : null;
  };
}

/** Shows a View code target from a page, found again by `resolve`; the sheet then follows that page's selection. */
function showSource(target: PageTarget, source: PeekTarget, resolve: () => PeekTarget | null): void {
  openPeek({ ...source, follow: target, resolve });
}

/** View code: opens the source of `selection` (default: the page's) in the peek sheet. Returns whether it had one. */
export function viewCode(target: PageTarget, selection: Selection = getPageView(target).selection): boolean {
  const result = sourceOf(target, selection);
  if (result.target) {
    showSource(target, result.target, resolverOf(target, selection));
  }
  return result.target !== null;
}

/** View code on an entrance's resolved target at `position`: that startup's or handler's definition. */
export function viewEntranceTarget(target: PageTarget, project: ProjectId, entrance: Key, position: number): void {
  const find = (settled: boolean) => {
    const model = loadedContext(target.board.project, settled).model(project, entrance);
    const handler = model?.type === "entrance" ? model.targets.find((item) => item.position === position) : undefined;
    return handler ? entranceTargetSource(project, handler).target : null;
  };
  const source = find(false);
  if (source) {
    showSource(target, source, () => find(true));
  }
}

/** Where a source target is, as Selected links it: `orders.ts:27–31` (or the file alone). */
export function sourceLines(source: PeekTarget): string {
  const { focus } = source;
  const lines = focus ? `:${focus.startLine}${focus.endLine !== focus.startLine ? `–${focus.endLine}` : ""}` : "";
  return `${shortPath(source.path)}${lines}`;
}

/** Whether two targets show the same lines of the same file. */
function sameSource(a: PeekTarget, b: PeekTarget): boolean {
  return a.project === b.project && a.path === b.path && a.line === b.line && JSON.stringify(a.focus ?? null) === JSON.stringify(b.focus ?? null);
}

/**
 * While the peek sheet shows View code for this page, selecting another item with a source moves the
 * sheet there. The page's selection, expansions, and camera stay as they are; an item without a source
 * leaves the sheet where it is.
 */
export function useFollowSelection(target: PageTarget): void {
  const selection = usePageView(target, (view) => view.selection);
  useEffect(() => {
    const peek = getPeekTarget();
    if (!peek || peek.follow?.page !== target.page || !sameBoard(peek.follow.board, target.board)) {
      return;
    }
    const result = sourceOf(target, selection);
    if (result.target && !sameSource(result.target, peek)) {
      showSource(target, result.target, resolverOf(target, selection));
    }
  }, [target, selection]);
}
