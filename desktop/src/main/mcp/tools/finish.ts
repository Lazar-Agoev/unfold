/**
 * Repair and finish (design §6.8): `repair` and `finish_work`.
 *
 * `repair` finds the health item (`board_health`, as the window that holds the board), refuses to
 * re-sign or accept what a person wrote (D6: that sign-off is theirs), and has the holding window
 * apply the repair the way Review does: saved at once, one undo step. `finish_work` clears the agent's
 * working marks on the boards it changed and shows its summary as a toast in the window.
 */

import { z } from "zod";
import type { AgentBoardRef } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { AnchorSpec, HealthItem, Page, ProjectRow } from "../engine-types";
import { anchorArg } from "../edits";
import { authorText } from "../format";
import { personWords, repairNames, repairSpec, REPAIRS, targetId, targetText, type RepairName } from "../health";
import { cardKey } from "../page-code";
import { cant, textResult, ToolError, windowRefusal } from "../results";
import { currentView, readPage, resolveBoard } from "../target";
import { defineTool } from "../tool";

const reconnectTo = z.union([z.string().min(1), anchorArg]);

/** What a repair did, in words, before the item's name. */
const DONE: Readonly<Record<RepairName, string>> = {
  reconnect: "Reconnected",
  remove_card: "Removed, with its step groups and links,",
  remove_label: "Removed (the step shows its name from the code again)",
  ungroup: "Ungrouped (its statements stay)",
  remove_link: "Removed",
  remove_resource: "Removed, with the links to it,",
  remove_description: "Removed the description of",
  resign: "Re-signed against the current code",
  accept: "Accepted the current code for",
};

/** Repairs that take their item off the page: nothing is left to outline. */
const REMOVALS: ReadonlySet<RepairName> = new Set(["remove_card", "remove_label", "ungroup", "remove_link", "remove_resource"]);

/** A reconnect's target as `repair_board` takes it: projects by shared name, absent for the board's own. */
async function reconnectSpec(context: ToolContext, own: ProjectRow, to: z.output<typeof reconnectTo> | undefined): Promise<string | AnchorSpec | undefined> {
  if (to === undefined || typeof to === "string") {
    return to;
  }
  const { project, ...anchor } = to;
  if (project === undefined) {
    return anchor;
  }
  const row = await context.project(project);
  return row.id === own.id ? anchor : { ...anchor, project: row.name };
}

/** The page items a repair changed, to outline: the item, or a Flow label's card. */
function outlined(page: Page, item: HealthItem, repair: RepairName): string[] {
  if (REMOVALS.has(repair)) {
    return [];
  }
  const { target } = item;
  if (!("label" in target)) {
    return [targetId(target)];
  }
  const card = page.cards.find((entry) => entry.ref.project === target.label.project && target.label.key.startsWith(`${cardKey(entry)}/`));
  return card ? [card.id] : [];
}

/** The one health item of `item` on `page` that offers `repair` (an anchor's, when `anchor` is given). */
function chosenItem(page: Page, items: readonly HealthItem[], item: string, repair: RepairName, anchor: number | undefined): HealthItem {
  const own = items.filter((entry) => entry.page === page.id && targetId(entry.target) === item);
  const [first] = own;
  if (!first) {
    throw cant(`repair “${item}”`, `nothing on the page “${page.title}” with that id has a health problem`, "check lists the page's problems, each with its item and repairs.");
  }
  const what = targetText(page, first.target);
  const offered = own.filter((entry) => repairNames(entry).includes(repair) && (anchor === undefined || entry.anchor === anchor));
  const [chosen] = offered;
  if (!chosen) {
    const names = [...new Set(own.flatMap(repairNames))];
    throw cant(`${repair.replaceAll("_", " ")} ${what}`, `its problems${anchor !== undefined ? ` with anchor ${anchor}` : ""} offer ${names.join(", ")}`, "Pass one of those; check shows each problem.");
  }
  const anchors = [...new Set(offered.flatMap((entry) => (entry.anchor !== undefined ? [entry.anchor] : [])))];
  if (anchors.length > 1) {
    throw cant(
      `${repair.replaceAll("_", " ")} ${what}`,
      `${anchors.length} of its anchors have problems`,
      `Pass anchor, one of: ${offered.map((entry) => `${entry.anchor ?? "none"} (${entry.message})`).join("; ")}.`,
    );
  }
  return chosen;
}

export const repair = defineTool({
  name: "repair",
  title: "Repair",
  description:
    "Applies one repair that a health problem offers (check lists each problem with its repair calls). repair is reconnect (to: the code's new key after a rename or move, from search or get_relations; for an anchor, a key or an evidence file's lines), remove_card, remove_label, ungroup, remove_link, remove_resource, remove_description, resign (keeps the text and signs it against the current code), or accept (accepts the current code for a reference held for review). item is the problem's item: a card's, step group's, link's, resource card's, or frame's id, or a Flow label's step key; anchor picks one of an item's anchors when several have problems. Reconnecting a card carries the steps of the connections that leave it. Re-sign or accept only after reading the new code (read_code), and only what an agent wrote: what a person wrote is theirs to sign off, so tell them in the chat instead. One undo step in Unfold, saved at once.",
  input: z.object({
    board: z.string().min(1).describe("The board's id or a link"),
    page: z.string().min(1).describe("The page's id or a link"),
    item: z.string().min(1).describe("The problem's item: an id, a Flow label's step key, or a link"),
    repair: z.enum(REPAIRS),
    to: reconnectTo.optional().describe("reconnect: the new key, or an anchor {key} or {file, lines: [from, to]}"),
    anchor: z.number().int().min(0).optional().describe("Which of the item's anchors, when several have problems"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ board, page, item, repair: name, to, anchor, project }, context) {
    const target = await resolveBoard(context, { board, page, project, item });
    if (target.page === undefined || target.item === undefined) {
      throw cant("tell what to repair", "the link names a board or a page, not an item", "Pass board, page, and item as check lists them.");
    }
    const health = await context.held(target.board, "board_health", { project: target.board.project, id: target.board.id });
    const read = await readPage(context, target.board, target.page);
    const chosen = chosenItem(read.page, health.health.items, target.item, name, anchor);
    const what = targetText(read.page, chosen.target);
    if (name === "resign" || name === "accept") {
      const words = personWords(read.page, chosen, name);
      if (words) {
        throw new ToolError(
          `Can't ${name === "resign" ? "re-sign" : "accept the current code for"} ${what}: ${authorText(words.author, context.agent)} wrote ${words.what}, so signing off on it is theirs to do. Tell them in the chat what changed in the code, and whether the text still reads right to you.`,
          "people_text",
        );
      }
    }
    const spec = repairSpec(read.page, chosen, name, await reconnectSpec(context, target.project, to));
    const repaired = await context.bridge.repair({ board: target.board, page: target.page, agent: context.agent, repair: spec, items: outlined(read.page, chosen, name) });
    if (!repaired.ok) {
      throw windowRefusal(repaired);
    }
    return textResult(
      context.header({ project: target.project.name, board: { title: repaired.board.title, draft: repaired.board.place === "draft" }, page: repaired.page.title, revision: repaired.revision }),
      `${DONE[name]} ${what}${name === "reconnect" ? ` to ${typeof to === "string" ? to : JSON.stringify(to)}` : ""}.`,
      `Ids: board ${target.board.id}, page ${target.page}. One undo step in Unfold, saved.`,
      "Next: check again to see what's left.",
    );
  },
});

/** A board's title, from its file (its id when the file can't be read). */
async function boardTitle(context: ToolContext, board: AgentBoardRef): Promise<string> {
  try {
    return (await context.engine("read_board", { project: board.project, id: board.id })).document.title;
  } catch {
    return board.id;
  }
}

export const finishWork = defineTool({
  name: "finish_work",
  title: "Finish work",
  description:
    'Ends a piece of work: clears "Claude is working…" and the outlines on what you changed, and shows your one-line summary as a toast in the window ("Framed the order flow in three services"). It finishes every board you edited or repaired since you last called it (board names one instead; default, with none edited, the board in view). Call it once the work is reviewed (check and get_screenshot), then tell the person in the chat what you made, and which parts go beyond what Unfold compiled.',
  input: z.object({
    summary: z.string().trim().min(1).max(200).describe("One line: what you made"),
    board: z.string().min(1).optional().describe("A board's id or a link (default: every board you changed)"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ summary, board, project }, context) {
    const worked = board !== undefined ? [(await resolveBoard(context, { board, project })).board] : context.bridge.workedOn(context.agent.author);
    const inView = currentView(context)?.board;
    const boards = worked.length > 0 ? worked : inView ? [inView] : [];
    if (boards.length === 0) {
      return textResult(context.header(), "Nothing to finish: you haven't changed a board since you last finished, and no board is in view.", "Next: tell the person in the chat what you found.");
    }
    const titles = await Promise.all(boards.map((ref) => boardTitle(context, ref)));
    for (const ref of boards) {
      context.bridge.finish({ board: ref, agent: context.agent, summary });
    }
    return textResult(
      context.header(),
      ...boards.map((ref, index) => `Finished on “${titles[index] ?? ref.id}” (board id ${ref.id}): its working marks are cleared, and “${context.agent.name} · ${summary}” shows in its window.`),
      "Next: tell the person in the chat what you made, and which parts go beyond what Unfold compiled.",
    );
  },
});
