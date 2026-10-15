/**
 * `propose`: creates or updates proposed steps on a page: an intended addition after a block, or at
 * the start of a definition's or a part's body, with a title, an optional reason, and its author. It
 * applies page-wide, wherever its block is drawn. Never code, never a claim that code does it, and no
 * status: a proposal is placed, reworded, moved, or removed. A place is checked against what a card of
 * the page draws: the block's overlay actions say whether a proposed step can go after it or at the
 * start of its body. A new place on an existing proposal is its explicit placement repair, also for an
 * anchor gone missing; it keeps the proposal's author. New words are the agent's, and a person's words
 * are changed only when the person asked (`replacePeopleText`).
 */

import type { AgentItemStatus, AgentProposalResult, AgentProposalSpec, AgentProposalSpot, ProposeOp } from "@unfold/desktop/agent-api";
import type { DrawnBlock } from "@unfold/desktop/drawing";
import type { Page, Proposal } from "../../../ipc/generated";
import { findBlock } from "../../cards";
import { cardKey } from "../document";
import { addProposal, findProposal, moveProposal, updateProposal, type ProposalChange } from "../proposals";
import { AgentRefusal, guardPeopleText, invalid, type OpContext } from "./context";

/** Where a proposal goes, as the board and the result say it. */
function spotOf(proposal: AgentProposalSpot): AgentProposalSpot {
  return "after" in proposal ? { after: proposal.after } : { start: proposal.start };
}

/** A proposed step's result entry. */
export function proposalResult(proposal: Proposal, status: AgentItemStatus): AgentProposalResult {
  return {
    kind: "proposal",
    id: proposal.id,
    status,
    title: proposal.title,
    ...(proposal.reason !== undefined ? { reason: proposal.reason } : {}),
    ...spotOf(proposal),
  };
}

/** The spot a spec names, if it names one. */
function specSpot(spec: AgentProposalSpec): AgentProposalSpot | undefined {
  if ("after" in spec) {
    return { after: spec.after };
  }
  return "start" in spec ? { start: spec.start } : undefined;
}

function sameSpot(a: AgentProposalSpot, b: AgentProposalSpot): boolean {
  return "after" in a ? "after" in b && a.after === b.after : "start" in b && a.start === b.start;
}

/** The proposal a spec without an id means: the page's proposal with the same spot and title, so the same call again changes nothing. */
function sameProposal(page: Page, spec: AgentProposalSpec): Proposal | undefined {
  const spot = specSpot(spec);
  const title = spec.title?.trim();
  return spot ? (page.proposals ?? []).find((proposal) => sameSpot(proposal, spot) && proposal.title === title) : undefined;
}

/** The block a card of the page draws with key `key`, as its loaded card says. */
function drawnBlock(ctx: OpContext, page: Page, key: string): DrawnBlock | undefined {
  for (const card of page.cards) {
    const model = ctx.data.model(ctx.board.project, cardKey(card));
    const found = model?.type === "definition" ? findBlock(model.flow, key) : undefined;
    if (found) {
      return found;
    }
  }
  return undefined;
}

/** The spot checked against what the page's cards draw: the block's overlay actions allow a proposal there. */
function checkedSpot(ctx: OpContext, page: Page, spot: AgentProposalSpot): AgentProposalSpot {
  const key = "after" in spot ? spot.after : spot.start;
  const block = drawnBlock(ctx, page, key);
  if (!block) {
    throw new AgentRefusal(
      "code_unavailable",
      `No card on this page draws “${key}”, so where a proposed step goes can't be checked. add_cards the definition it would change, then propose there; read_code lists its blocks with their keys.`,
    );
  }
  const name = block.name ?? block.label;
  if ("after" in spot && block.actions?.after !== true) {
    throw invalid(`A proposed step can't go after “${name}”: nothing runs after it on its path, or it isn't a step. Choose the block before it, or one on another path.`);
  }
  if ("start" in spot && block.actions?.start !== true) {
    throw invalid(`A proposed step can't go at the start of “${name}”: start takes a definition or a part, whose body it begins.`);
  }
  return spot;
}

export function applyPropose(page: Page, op: ProposeOp, ctx: OpContext): AgentProposalResult[] {
  return op.proposals.map((spec) => {
    const existing = spec.id !== undefined ? findProposal(page, spec.id) : sameProposal(page, spec);
    const spot = specSpot(spec);
    if (!existing) {
      if (spot === undefined || spec.title === undefined) {
        throw invalid("A new proposed step needs after (the key of the block it goes after) or start (the key of the definition or part whose body it begins), and title; pass id to change an existing one.");
      }
      const place = checkedSpot(ctx, page, spot);
      const title = spec.title.trim();
      const reason = spec.reason?.trim() ?? "";
      const id = addProposal(page, {
        ...(spec.id !== undefined ? { id: spec.id } : {}),
        ...place,
        title,
        ...(reason.length > 0 ? { reason } : {}),
        author: ctx.agent.author,
      });
      const added = findProposal(page, id);
      if (!added) {
        throw invalid(`The proposed step “${title}” couldn't be added.`);
      }
      return proposalResult(added, "created");
    }
    const change: { -readonly [K in keyof ProposalChange]: ProposalChange[K] } = {};
    if (spec.title !== undefined) {
      const title = spec.title.trim();
      if (title !== existing.title) {
        guardPeopleText(`the title of the proposed step “${existing.title}”`, existing.author, op.replacePeopleText);
      }
      change.title = title;
    }
    if (spec.reason !== undefined) {
      const reason = spec.reason?.trim() ?? "";
      if (reason !== (existing.reason ?? "")) {
        guardPeopleText(`the reason of the proposed step “${existing.title}”`, existing.author, op.replacePeopleText);
      }
      change.reason = reason;
    }
    const before = JSON.stringify(existing);
    if (spot !== undefined && !sameSpot(existing, spot)) {
      moveProposal(page, existing.id, checkedSpot(ctx, page, spot));
    }
    updateProposal(page, existing.id, change, ctx.agent.author);
    const after = findProposal(page, existing.id) ?? existing;
    return proposalResult(after, JSON.stringify(after) === before ? "kept" : "updated");
  });
}
