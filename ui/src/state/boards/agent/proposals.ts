/**
 * `propose`: creates or updates proposed steps on one block card (docs/proposals/unified-code-diagrams.md
 * §7): an intended addition after a real step of the card's own Flow, with a title, an optional reason,
 * and its author. Never code, never a claim that code does it, and no status: a proposal is placed,
 * reworded, moved, or removed. A placement is checked against the card's compiled Flow by the rule a
 * person's Add proposed step after… also follows (`placementProblem`): a real step
 * (not Start or End) that something runs after (not a return, throw, break, continue, or opaque step),
 * and, for a step with several outgoing paths (a condition's yes and no, a loop's each and done), one
 * of those paths. A new `after` or `path` on an existing proposal is its explicit placement repair,
 * also for an anchor gone missing or held; it keeps the proposal's author. New words are the agent's,
 * and a person's words are changed only when the person asked (`replacePeopleText`).
 */

import type { AgentItemStatus, AgentProposalResult, AgentProposalSpec, ProposeOp } from "@unfold/desktop/agent-api";
import { isTypeKind, keyBlockKind } from "../../../code/typeKinds";
import type { FlowModel, FlowNode, Key, Page, Proposal, ProposalPath } from "../../../ipc/generated";
import { cardKey, isFreeId } from "../document";
import { addProposal, findProposal, placementProblem, proposalReason, proposalsOf, proposalTitle, PROPOSAL_REASON_MAX, PROPOSAL_TITLE_MAX, updateProposal, type ProposalChange } from "../proposals";
import { AgentRefusal, cardName, cardOf, cardProject, guardPeopleText, invalid, type OpContext } from "./context";

/** `“Place the order” (line 32)`: a step as a refusal names it. */
function stepText(node: FlowNode, label: string): string {
  return `“${label}”${node.startLine !== undefined ? ` (line ${node.startLine})` : ""}`;
}

/** `"yes" or "no"`. */
function choices(paths: readonly ProposalPath[]): string {
  return paths.map((path) => `"${path}"`).join(" or ");
}

/** A proposed step's result entry, with the step it goes after as the card's Flow names it. */
function proposalResult(proposal: Proposal, status: AgentItemStatus, flow: FlowModel | null, label: (key: Key, node: FlowNode) => string): AgentProposalResult {
  const node = flow?.nodes.find((item) => item.key === proposal.after);
  return {
    kind: "proposal",
    id: proposal.id,
    status,
    card: proposal.card,
    after: proposal.after,
    ...(proposal.path !== undefined ? { path: proposal.path } : {}),
    title: proposal.title,
    ...(proposal.reason !== undefined ? { reason: proposal.reason } : {}),
    ...(node ? { afterLabel: label(proposal.after, node) } : {}),
    ...(node?.startLine !== undefined ? { afterLine: node.startLine } : {}),
  };
}

/** A proposal's title as the file holds it, or a refusal that says what fits. */
function titleOf(text: string): string {
  const title = proposalTitle(text);
  if (title === null) {
    throw invalid(`A proposed step's title is one line of 1 to ${PROPOSAL_TITLE_MAX} characters that says what it would do ("Email a confirmation").`);
  }
  return title;
}

/** A proposal's reason as the file holds it (null clears it), or a refusal that says what fits. */
function reasonOf(text: string | null): string | null {
  if (text === null) {
    return null;
  }
  const reason = proposalReason(text);
  if (reason === null) {
    throw invalid(`A proposed step's reason is 1 to ${PROPOSAL_REASON_MAX} characters on why it's wanted; pass null to clear it.`);
  }
  return reason;
}

/** The proposal a spec without an id means: the card's proposal with the same placement and title, so the same call again changes nothing. */
function sameProposal(page: Page, card: string, spec: AgentProposalSpec): Proposal | undefined {
  const title = spec.title === undefined ? null : proposalTitle(spec.title);
  return proposalsOf(page, card).find((proposal) => proposal.after === spec.after && proposal.path === spec.path && proposal.title === title);
}

export function applyPropose(page: Page, op: ProposeOp, ctx: OpContext): AgentProposalResult[] {
  const card = cardOf(page, op.card);
  const name = cardName(ctx, card);
  const key = cardKey(card);
  if (!("block" in card.ref) || isTypeKind(keyBlockKind(key) ?? "")) {
    throw invalid(`${name} is ${"block" in card.ref ? "a type card" : "an entrance card"}: a proposed step goes after a real step of a function's card. add_cards the function it would change, then propose there.`);
  }
  const model = ctx.data.model(cardProject(ctx, card), key);
  const flow = model?.type === "block" && model.flow?.source === "current" ? model.flow : null;
  const project = card.ref.project;
  const label = (step: Key, node: FlowNode) => page.labels.find((entry) => entry.project === project && entry.key === step)?.text ?? node.label;

  /** The placement after `after` on `path`, checked against the card's Flow. */
  const placement = (after: Key, path: ProposalPath | undefined): { after: Key; path: ProposalPath | undefined } => {
    if (!flow) {
      throw new AgentRefusal(
        "code_unavailable",
        `${name}'s Flow isn't available right now (its file changed since the last compile, or its project isn't connected), so where a proposed step goes can't be checked. Compile the project, then try again.`,
      );
    }
    const problem = placementProblem(flow, after, path);
    switch (problem?.kind) {
      case undefined:
        return { after, path };
      case "notStep":
        throw invalid(`“${after}” isn't a step of ${name}'s Flow. A proposed step goes after one of its steps: read_code lists them with their keys.`);
      case "start":
        throw invalid(`“${after}” is ${name} itself (its Start). A proposed step goes after one of its real steps: read_code lists them with their keys.`);
      case "terminal":
        throw invalid(
          `${name}'s step ${stepText(problem.node, label(after, problem.node))} is a ${problem.node.kind}: nothing runs after it on its path, so a proposed step can't go there. Choose the step before it, or a step on another path.`,
        );
      case "needsPath":
        throw invalid(`${name}'s step ${stepText(problem.node, label(after, problem.node))} has ${problem.paths.length} paths: pass path ${choices(problem.paths)}.`);
      case "noSuchPath":
        throw invalid(`${name}'s step ${stepText(problem.node, label(after, problem.node))} has no "${path ?? ""}" path: pass path ${choices(problem.paths)}.`);
      case "oneWay":
        throw invalid(
          `${name}'s step ${stepText(problem.node, label(after, problem.node))} has one way on: leave path out (path picks a condition's "yes" or "no", or a loop's "each" or "done").`,
        );
    }
  };

  return op.proposals.map((spec) => {
    const existing = spec.id !== undefined ? findProposal(page, spec.id) : sameProposal(page, card.id, spec);
    if (existing && existing.card !== card.id) {
      throw invalid(`“${existing.id}” is a proposed step of card ${existing.card}, not ${card.id}: pass card ${existing.card} to change it.`);
    }
    if (!existing) {
      if (spec.id !== undefined && !isFreeId(page, spec.id)) {
        throw invalid(`“${spec.id}” can't name a new proposed step: another item has it, or it isn't letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`);
      }
      if (spec.after === undefined || spec.title === undefined) {
        throw invalid(`A new proposed step needs after (the key of the step of ${name} it goes after, from read_code) and title; pass id to change an existing one.`);
      }
      const place = placement(spec.after, spec.path);
      const title = titleOf(spec.title);
      const reason = spec.reason === undefined ? null : reasonOf(spec.reason);
      const id = addProposal(page, {
        ...(spec.id !== undefined ? { id: spec.id } : {}),
        card: card.id,
        after: place.after,
        ...(place.path !== undefined ? { path: place.path } : {}),
        title,
        ...(reason !== null ? { reason } : {}),
        author: ctx.agent.author,
      });
      const added = findProposal(page, id);
      if (!added) {
        throw invalid(`The proposed step “${title}” couldn't be added.`);
      }
      return proposalResult(added, "created", flow, label);
    }
    const change: { -readonly [K in keyof ProposalChange]: ProposalChange[K] } = {};
    if (spec.title !== undefined) {
      const title = titleOf(spec.title);
      if (title !== existing.title) {
        guardPeopleText(`the title of the proposed step “${existing.title}”`, existing.author, op.replacePeopleText);
      }
      change.title = title;
    }
    if (spec.reason !== undefined) {
      const reason = reasonOf(spec.reason);
      if ((reason ?? undefined) !== existing.reason) {
        guardPeopleText(`the reason of the proposed step “${existing.title}”`, existing.author, op.replacePeopleText);
      }
      change.reason = reason;
    }
    if (spec.after !== undefined || spec.path !== undefined) {
      const place = placement(spec.after ?? existing.after, spec.path);
      change.after = place.after;
      change.path = place.path ?? null;
    }
    const before = JSON.stringify(existing);
    updateProposal(page, existing.id, change, ctx.agent.author);
    return proposalResult(existing, JSON.stringify(existing) === before ? "kept" : "updated", flow, label);
  });
}
