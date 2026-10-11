/**
 * What agents write: `describe` (descriptions of cards, step groups, and frames, and labels of links) and
 * `label_steps` (Flow labels of a card's steps). Every text is signed with the agent's identity and
 * signed against the code at the next save. Replacing a person's text is refused unless the call says
 * the person asked for it; text that doesn't change is kept as it is, whoever wrote it.
 */

import type { AgentItemResult, AgentItemStatus, DescribeOp, LabelStepsOp } from "@unfold/desktop/agent-api";
import type { Page } from "../../../ipc/generated";
import { cardKey, findCard, setCardDescription, setLabel, setLinkLabel, updateGroup } from "../document";
import { findFrame, updateFrame } from "../frames";
import { AgentRefusal, cardName, cardOf, cardProject, cardResult, guardPeopleText, invalid, type OpContext } from "./context";
import { frameResult } from "./frames";

/** `created` for new text, `kept` for the same text, `updated` for new words. */
function statusOf(before: string | undefined, text: string): AgentItemStatus {
  return before === undefined ? "created" : before === text ? "kept" : "updated";
}

/** The text an agent writes, trimmed; blank text is refused (taking text off is a person's call). */
function textOf(text: string, what: string): string {
  const trimmed = text.trim();
  if (trimmed.length === 0) {
    throw invalid(`The text for ${what} is blank. Write a sentence, or leave it as it is.`);
  }
  return trimmed;
}

export function applyDescribe(page: Page, op: DescribeOp, ctx: OpContext): AgentItemResult[] {
  const author = ctx.agent.author;
  return op.items.map((item): AgentItemResult => {
    const text = textOf(item.text, `“${item.id}”`);
    const card = findCard(page, item.id);
    if (card) {
      const before = card.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of ${cardName(ctx, card)}`, before.author, op.replacePeopleText);
      }
      setCardDescription(page, card.id, text, author);
      return cardResult(ctx, card, statusOf(before?.text, text));
    }
    const group = page.groups.find((entry) => entry.id === item.id);
    if (group) {
      const before = group.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of the step group “${group.title}”`, before.author, op.replacePeopleText);
      }
      updateGroup(page, group.id, { description: { text, author } });
      return { kind: "group", id: group.id, status: statusOf(before?.text, text), card: group.card, name: group.title };
    }
    const frame = findFrame(page, item.id);
    if (frame) {
      const before = frame.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of the frame “${frame.title}”`, before.author, op.replacePeopleText);
      }
      updateFrame(page, frame.id, { description: { text, author } });
      return frameResult(ctx, page, frame, statusOf(before?.text, text));
    }
    const link = page.links.find((entry) => entry.id === item.id);
    if (link) {
      const before = link.label;
      if (before && before.text !== text) {
        guardPeopleText(`the label “${before.text}”`, before.author, op.replacePeopleText);
      }
      setLinkLabel(page, link.id, text, author);
      return { kind: "link", id: link.id, status: statusOf(before?.text, text), from: link.from, to: link.to, name: text };
    }
    throw invalid(`There is no card, step group, frame, or link with id “${item.id}” on this page. read_page lists the page's ids.`);
  });
}

export function applyLabelSteps(page: Page, op: LabelStepsOp, ctx: OpContext): AgentItemResult[] {
  const card = cardOf(page, op.card);
  const name = cardName(ctx, card);
  if (!("block" in card.ref)) {
    throw invalid(`${name} is an entrance card; Flow labels go on a function's steps.`);
  }
  const model = ctx.data.model(cardProject(ctx, card), cardKey(card));
  const flow = model?.type === "block" ? model.flow : null;
  if (!flow) {
    throw new AgentRefusal(
      "code_unavailable",
      `${name}'s Flow isn't available right now (its file changed since the last compile, or its project isn't connected). Compile the project, then try again.`,
    );
  }
  const steps = new Set(flow.nodes.flatMap((node) => (node.key !== undefined ? [node.key] : [])));
  const project = card.ref.project;
  return op.labels.map(({ step, text: written }): AgentItemResult => {
    const text = textOf(written, `the step “${step}”`);
    if (!steps.has(step)) {
      throw invalid(`“${step}” isn't a Flow step of ${name}. read_code lists its steps and their keys.`);
    }
    const before = page.labels.find((label) => label.project === project && label.key === step);
    if (before && before.text !== text) {
      guardPeopleText(`the Flow label “${before.text}”`, before.author, op.replacePeopleText);
    }
    setLabel(page, project, step, text, ctx.agent.author);
    return { kind: "label", status: statusOf(before?.text, text), card: card.id, step, name: text };
  });
}
