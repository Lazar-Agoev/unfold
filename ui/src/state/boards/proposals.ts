/**
 * Pure edits of a page's proposed steps (docs/proposals/unified-code-diagrams.md §7): an intended
 * addition after a real step of a block card, with a title, an optional reason, and its author. A
 * proposal is not a compiler fact: it has no source of its own and is never signed; its anchor is
 * guarded like any reference. Each function is an Immer recipe over a draft page; an empty list is
 * deleted, as the file omits it.
 */

import type { FlowModel, FlowNode, Key, Page, Proposal, ProposalPath, RouteKind } from "../../ipc/generated";
import { removeItems, uniqueId } from "./document";

/** The most characters a proposed step's title holds (one line). */
export const PROPOSAL_TITLE_MAX = 120;
/** The most characters a proposed step's reason holds. */
export const PROPOSAL_REASON_MAX = 400;

/** A page's proposed steps, as the queries read them. */
interface Proposed {
  readonly proposals?: readonly Proposal[] | undefined | null;
}

export function findProposal(page: Proposed, proposalId: string): Proposal | undefined {
  return (page.proposals ?? []).find((proposal) => proposal.id === proposalId);
}

/** The proposed steps of a card, in the page's order. */
export function proposalsOf(page: Proposed, cardId: string): Proposal[] {
  return (page.proposals ?? []).filter((proposal) => proposal.card === cardId);
}

/** How long a text is, as the file counts it: in Unicode scalar values (a surrogate pair is one). */
function scalarLength(text: string): number {
  return text.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, "_").length;
}

/** A title as the file holds it: trimmed, one line, or null when it is blank, too long, or holds a control character. */
export function proposalTitle(text: string): string | null {
  const trimmed = text.trim();
  // eslint-disable-next-line no-control-regex -- control characters are what the format refuses.
  return trimmed.length > 0 && scalarLength(trimmed) <= PROPOSAL_TITLE_MAX && !/[\u0000-\u001f\u007f]/.test(trimmed) ? trimmed : null;
}

/** A reason as the file holds it: trimmed, or null when it is blank or too long. */
export function proposalReason(text: string): string | null {
  const trimmed = text.trim();
  return trimmed.length > 0 && scalarLength(trimmed) <= PROPOSAL_REASON_MAX ? trimmed : null;
}

/** What a new proposed step is: its card, the real step it goes after (and which path), its words, and who proposed it. */
export interface NewProposal {
  /** A free id (`isFreeId`); absent: one of its own. */
  readonly id?: string;
  readonly card: string;
  readonly after: Key;
  readonly path?: ProposalPath;
  readonly title: string;
  readonly reason?: string;
  readonly author: string;
}

/** Adds a proposed step and returns its id. The title and reason must already be valid (`proposalTitle`, `proposalReason`). */
export function addProposal(page: Page, proposal: NewProposal): string {
  const id = proposal.id ?? uniqueId(page, "proposal");
  page.proposals ??= [];
  page.proposals.push({
    id,
    card: proposal.card,
    after: proposal.after,
    ...(proposal.path !== undefined ? { path: proposal.path } : {}),
    title: proposal.title,
    ...(proposal.reason !== undefined ? { reason: proposal.reason } : {}),
    author: proposal.author,
  });
  return id;
}

/** A change to a proposed step: its words (`reason: null` clears it), or where it goes (`path: null` clears it). */
export interface ProposalChange {
  readonly title?: string;
  readonly reason?: string | null;
  readonly after?: Key;
  readonly path?: ProposalPath | null;
}

/**
 * Changes a proposed step. New words make `author` its author; moving it to another real step (an
 * explicit placement repair) keeps its author. Invalid words change nothing.
 */
export function updateProposal(page: Page, proposalId: string, change: ProposalChange, author: string): void {
  const proposal = (page.proposals ?? []).find((item) => item.id === proposalId);
  if (!proposal) {
    return;
  }
  let reworded = false;
  if (change.title !== undefined) {
    const title = proposalTitle(change.title);
    if (title !== null && title !== proposal.title) {
      proposal.title = title;
      reworded = true;
    }
  }
  if (change.reason !== undefined) {
    const reason = change.reason === null ? null : proposalReason(change.reason);
    if (reason === null && change.reason !== null && change.reason.trim().length > 0) {
      // Too long: keep the old reason.
    } else if ((reason ?? undefined) !== proposal.reason) {
      if (reason === null) {
        delete proposal.reason;
      } else {
        proposal.reason = reason;
      }
      reworded = true;
    }
  }
  if (change.after !== undefined) {
    proposal.after = change.after;
  }
  if (change.path !== undefined) {
    if (change.path === null) {
      delete proposal.path;
    } else {
      proposal.path = change.path;
    }
  }
  if (reworded) {
    proposal.author = author;
  }
}

/** Removes a proposed step; it never edits code. */
export function removeProposal(page: Page, proposalId: string): void {
  removeItems(page, [proposalId]);
}

/** The proposal path each route kind out of a decision is: an `else if` is the condition's "no", a do-while's `again` its "each". */
const ROUTE_PATHS: Readonly<Partial<Record<RouteKind, ProposalPath>>> = { yes: "yes", no: "no", elseIf: "no", each: "each", again: "each", done: "done" };

/**
 * The outgoing paths of Flow node `node` a proposed step after it chooses between, in the order its
 * routes were derived: a condition's yes and no, a loop's each and done. Fewer than two means it has
 * one way on, so a proposal after it takes no path.
 */
export function stepPaths(flow: Pick<FlowModel, "routes">, node: number): ProposalPath[] {
  const paths = flow.routes.flatMap((route) => {
    const path = route.from === node ? ROUTE_PATHS[route.kind] : undefined;
    return path === undefined ? [] : [path];
  });
  return [...new Set(paths)];
}

/** The proposal path a route out of a decision is on (`elseIf` is "no", `again` is "each"); none for any other route. */
export function routePath(kind: RouteKind): ProposalPath | undefined {
  return ROUTE_PATHS[kind];
}

/** Steps nothing runs after on their path: a proposed step can't go after one. */
const TERMINAL: ReadonlySet<FlowNode["kind"]> = new Set(["return", "throw", "break", "continue", "opaque"]);

/**
 * Why a proposed step can't go after key `after` (on `path`) in a Flow: not a step of it (or its End),
 * its Start, a step nothing runs after (`terminal`), a step with several paths given none or one it
 * doesn't have, or a step with one way on given a path.
 */
export type PlacementProblem =
  | { readonly kind: "notStep" | "start" }
  | { readonly kind: "terminal" | "oneWay"; readonly node: FlowNode }
  | { readonly kind: "needsPath" | "noSuchPath"; readonly node: FlowNode; readonly paths: readonly ProposalPath[] };

/**
 * Where a proposed step can go after Flow step `after`: the step and the paths to choose between
 * (empty when it has one way on), or why none can go after it. The one placement rule, shared by the
 * person's Add proposed step after… and the agent's `propose`.
 */
export function placesAfter(
  flow: Pick<FlowModel, "nodes" | "routes">,
  after: Key,
): { readonly node: FlowNode; readonly paths: readonly ProposalPath[] } | { readonly problem: PlacementProblem } {
  const node = flow.nodes.find((item) => item.key === after);
  if (!node || node.kind === "end") {
    return { problem: { kind: "notStep" } };
  }
  if (node.kind === "start") {
    return { problem: { kind: "start" } };
  }
  if (TERMINAL.has(node.kind)) {
    return { problem: { kind: "terminal", node } };
  }
  const paths = stepPaths(flow, node.id);
  return { node, paths: paths.length > 1 ? paths : [] };
}

/** Why a proposed step can't go after `after` on `path` (absent: no path) in a Flow, or null when it can. */
export function placementProblem(flow: Pick<FlowModel, "nodes" | "routes">, after: Key, path: ProposalPath | undefined): PlacementProblem | null {
  const places = placesAfter(flow, after);
  if ("problem" in places) {
    return places.problem;
  }
  const { node, paths } = places;
  if (paths.length > 1 && path === undefined) {
    return { kind: "needsPath", node, paths };
  }
  if (paths.length > 1 && path !== undefined && !paths.includes(path)) {
    return { kind: "noSuchPath", node, paths };
  }
  if (paths.length <= 1 && path !== undefined) {
    return { kind: "oneWay", node };
  }
  return null;
}
