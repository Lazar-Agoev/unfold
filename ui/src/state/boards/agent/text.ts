/**
 * What agents write with `describe`: descriptions of cards, combined steps, frames, resource cards, and
 * connections. Every text is signed with the agent's identity and signed against the code at the next
 * save. Replacing a person's text is refused unless the call says the person asked for it; text that
 * doesn't change is kept as it is, whoever wrote it.
 */

import type { AgentItemResult, AgentItemStatus, DescribeOp } from "@unfold/desktop/agent-api";
import type { Page } from "../../../ipc/generated";
import { findCard, findCombined, setCardDescription, updateCombined } from "../document";
import { setLinkDescription } from "../connections";
import { findFrame, updateFrame } from "../frames";
import { findNote } from "../notes";
import { findResource, setResourceDescription } from "../resources";
import { cardName, cardResult, combinedResult, guardPeopleText, invalid, resourceName, type OpContext } from "./context";
import { linkResult } from "./connections";
import { frameResult } from "./frames";
import { resourceResult } from "./resources";

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
    const combined = findCombined(page, item.id);
    if (combined) {
      const before = combined.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of the combined step “${combined.title}”`, before.author, op.replacePeopleText);
      }
      updateCombined(page, combined.id, { description: { text, author } });
      return combinedResult(combined, statusOf(before?.text, text));
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
    const resource = findResource(page, item.id);
    if (resource) {
      const before = resource.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of ${resourceName(resource)}`, before.author, op.replacePeopleText);
      }
      setResourceDescription(page, resource.id, text, author);
      return resourceResult(ctx, page, resource, statusOf(before?.text, text));
    }
    const link = page.links.find((entry) => entry.id === item.id);
    if (link) {
      // A connection's description gives its evidence in words; its label says what travels (connect).
      const before = link.description;
      if (before && before.text !== text) {
        guardPeopleText(`the description of the ${link.kind} connection`, before.author, op.replacePeopleText);
      }
      setLinkDescription(page, link.id, text, author);
      return linkResult(link, statusOf(before?.text, text));
    }
    if (findNote(page, item.id)) {
      throw invalid(`“${item.id}” is a note: its text is what it says. add_notes with its id rewrites it.`);
    }
    throw invalid(`There is no card, combined step, frame, resource card, or connection with id “${item.id}” on this page. read_page lists the page's ids.`);
  });
}
