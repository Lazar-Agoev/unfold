/**
 * Pure edits of a page's proposed steps: an intended addition after a block, or at the start of a
 * definition's or a part's body, with a title, an optional reason, and its author. It applies page-wide,
 * on every card that draws that block. A proposal is not a compiler fact: it has no source of its own
 * and is never signed; its anchor is guarded like any reference. Where one can go comes from the drawn
 * flow's overlay actions. Each function is an Immer recipe over a draft page; an empty list is deleted,
 * as the file omits it.
 */

import type { Key, Page, Proposal } from "../../ipc/generated";
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

/** Where a proposed step goes: after a block, or at the start of a definition's or a part's body. */
export type ProposalPlace = { readonly after: Key } | { readonly start: Key };

/** What a new proposed step is: where it goes, its words, and who proposed it. */
export type NewProposal = ProposalPlace & {
  /** A free id (`isFreeId`); absent: one of its own. */
  readonly id?: string;
  readonly title: string;
  readonly reason?: string;
  readonly author: string;
};

/** Adds a proposed step and returns its id. The title and reason must already be valid (`proposalTitle`, `proposalReason`). */
export function addProposal(page: Page, proposal: NewProposal): string {
  const id = proposal.id ?? uniqueId(page, "proposal");
  page.proposals ??= [];
  page.proposals.push({
    id,
    ...("after" in proposal ? { after: proposal.after } : { start: proposal.start }),
    title: proposal.title,
    ...(proposal.reason !== undefined ? { reason: proposal.reason } : {}),
    author: proposal.author,
  });
  return id;
}

/** A change to a proposed step's words (`reason: null` clears it). */
export interface ProposalChange {
  readonly title?: string;
  readonly reason?: string | null;
}

/** Rewords a proposed step, which makes `author` its author. Invalid words change nothing. */
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
  if (reworded) {
    proposal.author = author;
  }
}

/** Moves a proposed step to another place (an explicit placement repair); it keeps its words and author. */
export function moveProposal(page: Page, proposalId: string, place: ProposalPlace): void {
  const proposals = page.proposals ?? [];
  const index = proposals.findIndex((item) => item.id === proposalId);
  const proposal = proposals[index];
  if (!proposal) {
    return;
  }
  const { id, title, reason, author } = proposal;
  proposals[index] = { id, ...("after" in place ? { after: place.after } : { start: place.start }), title, ...(reason !== undefined ? { reason } : {}), author };
}

/** Removes a proposed step; it never edits code. */
export function removeProposal(page: Page, proposalId: string): void {
  removeItems(page, [proposalId]);
}
