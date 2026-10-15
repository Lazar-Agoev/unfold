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

/** A page's proposed steps, as the queries read them. */
interface Proposed {
  readonly proposals?: readonly Proposal[] | undefined | null;
}

export function findProposal(page: Proposed, proposalId: string): Proposal | undefined {
  return (page.proposals ?? []).find((proposal) => proposal.id === proposalId);
}

/** Where a proposed step goes: after a block, or at the start of a definition's or a part's body. */
export type ProposalPlace = { readonly after: Key } | { readonly start: Key };

/** What a new proposed step is: where it goes, its words, and who proposed it. */
export type NewProposal = ProposalPlace & {
  /** An id of its own choosing (the save refuses one the page has); absent: one of its own. */
  readonly id?: string;
  readonly title: string;
  readonly reason?: string;
  readonly author: string;
};

/** Adds a proposed step and returns its id. The board's save checks its words. */
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

/**
 * Rewords a proposed step, trimmed, which makes `author` its author; the board's save checks the words. A
 * blank title changes nothing (a proposed step says what it would do), and a blank reason clears it.
 */
export function updateProposal(page: Page, proposalId: string, change: ProposalChange, author: string): void {
  const proposal = (page.proposals ?? []).find((item) => item.id === proposalId);
  if (!proposal) {
    return;
  }
  let reworded = false;
  const title = change.title?.trim() ?? "";
  if (title.length > 0 && title !== proposal.title) {
    proposal.title = title;
    reworded = true;
  }
  if (change.reason !== undefined) {
    const reason = change.reason?.trim() ?? "";
    if (reason !== (proposal.reason ?? "")) {
      if (reason.length === 0) {
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
