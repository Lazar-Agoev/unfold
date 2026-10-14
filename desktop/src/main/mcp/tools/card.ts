/**
 * Shape a card (mcp-capabilities.md § Shape a card): `shape_card`, a card's whole shaping in one call.
 *
 * The agent names blocks by short label (`try[0]/except[0]`, `call:save_order`) or full key. Each
 * label maps to its key by lookup in the card's label map (`code_text`); a label in an open path's
 * later blocks maps in the label map of what the block before it opens (the callee drawn in place, as
 * `card` draws it). The window applies the shaping as one undo step, and the board's save checks every
 * key and the overlay rules. The result is the shaped card as `flow_text` prints it; a refusal says
 * what failed and lists the blocks the agent can choose from instead.
 */

import { z } from "zod";
import type { AgentCallPath, ShapeCardOp } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { Page, ProjectRow } from "../engine-types";
import { applyEdit, defined, editResult, editTarget, expectRevision, itemLine, itemsHeading, localIds, pageArgs, type AppliedEdit, type EditTarget } from "../edits";
import { choiceLines, Labels, openedFlow } from "../labels";
import { overlayArgs } from "../page-code";
import { cant, ToolError } from "../results";
import { isLink, readPage } from "../target";
import { defineTool } from "../tool";

/** The refusals that come from the shaping itself (a key, a rule), which the blocks to choose from answer. */
const SHAPING_REFUSALS: ReadonlySet<string | undefined> = new Set(["board_problem", "invalid_op"]);

/** A definition's label map, or none when it has no compiled tree to read (its keys are then taken as written). */
async function labelsOf(context: ToolContext, project: ProjectRow, key: string): Promise<Labels> {
  try {
    return new Labels((await context.engine("code_text", { project: project.id, key })).labels);
  } catch (error) {
    if (error instanceof ToolError) {
      return new Labels([]);
    }
    throw error;
  }
}

/** The definition that the last block of `path` opens in place on the card of `card`, as `card` draws it. */
async function openedDefinition(context: ToolContext, project: ProjectRow, card: string, path: readonly string[]): Promise<string | undefined> {
  const last = path.at(-1);
  try {
    const view = await context.engine("card", { project: project.id, key: card, open: [[...path]] });
    return view.type === "definition" && last !== undefined ? openedFlow(view.flow, last)?.header.key : undefined;
  } catch (error) {
    if (error instanceof ToolError) {
      return undefined;
    }
    throw error;
  }
}

/**
 * An open path's blocks as keys: the first in the card's labels, each next one in the labels of what
 * the block before it opens. When that can't be read, the rest go as written, and the save says why.
 */
async function openPath(context: ToolContext, project: ProjectRow, card: string, labels: Labels, path: readonly string[]): Promise<string[]> {
  const keys: string[] = [];
  let level = labels;
  for (const [index, block] of path.entries()) {
    keys.push(level.key(block));
    if (index === path.length - 1) {
      break;
    }
    const opened = await openedDefinition(context, project, card, keys);
    if (opened === undefined) {
      return [...keys, ...path.slice(index + 1)];
    }
    level = await labelsOf(context, project, opened);
  }
  return keys;
}

/** The blocks of the card an agent can choose from, for a refusal; nothing when its flow can't be read. */
async function choices(context: ToolContext, project: ProjectRow, key: string, labels: Labels): Promise<string[]> {
  try {
    const view = await context.engine("card", { project: project.id, key });
    return view.type === "definition" ? choiceLines(view.flow, labels) : [];
  } catch (error) {
    if (error instanceof ToolError) {
      return [];
    }
    throw error;
  }
}

/** The shaped card as the person now sees it: `flow_text` with the saved page's overlay and the card's open paths. */
async function shapedFlow(context: ToolContext, target: EditTarget, edit: AppliedEdit, card: string, key: string): Promise<string> {
  const page = edit.page as unknown as Page;
  const open = page.cards.find((item) => item.id === card)?.open ?? [];
  const flow = await context.engine("flow_text", { project: target.project.id, key, overlay: overlayArgs(page), open });
  return flow.text.trimEnd();
}

const block = z.string().min(1);

export const shapeCard = defineTool({
  name: "shape_card",
  title: "Shape card",
  description:
    'Shapes one expanded card in one call, so its code-shaped flow reads as the few named steps a person would draw: title (names the card), description (one sentence on what it does and why), names ({block: text} on any block: a call, a branch as a yes-or-no question, a loop, an except part, an exit, a callback; null takes a name off), combine ([{steps, title, collapsed}]: consecutive steps of one body, or one whole control block, under one title; never overlapping or nesting; collapsed by default, and it keeps its ways out and its unknown-call count), and open ([[blocks]]: the calls or callbacks it opens in place, each a path of 1 to 3, outermost first, the next a call of what the one before opens; at most 64; it replaces what the card opens, [] closes them). Name blocks by short label from read_code (try[0]/except[0], call:save_order) or full key. Name only what the code doesn\'t say; naming a library or unknown call lifts it into a step, and it stays marked ? (a name never resolves a call). Names and combined steps apply page-wide, wherever that code is drawn. Changing or removing a person\'s name, title, or description is refused unless replacePeopleText is true: set it only when the person asked. One undo step; the result is the shaped card as the person sees it. A refusal lists the blocks you can choose from. Read get_guide("diagrams") first.',
  input: z.object({
    ...pageArgs,
    card: z.string().min(1).describe("The card's id (read_page lists them), or a link to it"),
    title: z.string().trim().min(1).max(120).optional().describe("The card's title: the page's name for its definition"),
    description: z.string().trim().min(1).max(400).optional().describe("One sentence on what the card does and why"),
    names: z.record(block, z.string().trim().min(1).max(120).nullable()).optional().describe("Names by block (a short label or a key); null takes a name off"),
    combine: z
      .array(
        z.object({
          steps: z.array(block).min(1).describe("The blocks it combines, in written order (short labels or keys)"),
          title: z.string().trim().min(1).max(120),
          collapsed: z.boolean().optional().describe("Drawn as one step (default for a new one: true)"),
        }),
      )
      .optional()
      .describe("Combined steps; one with exactly these steps already on the page is updated"),
    open: z.array(z.array(block).min(1).max(3)).max(64).optional().describe("The paths of calls it opens in place, replacing what it opens now"),
    replacePeopleText: z.boolean().optional().describe("Only when the person asked you to change their words"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, card, title, description, names, combine, open, replacePeopleText, expectRevision: revision }, context) {
    if (title === undefined && description === undefined && names === undefined && combine === undefined && open === undefined) {
      throw cant("shape the card", "nothing to change was given", "Pass title, description, names, combine, or open.");
    }
    const target = await editTarget(context, { board, page, project, item: isLink(card) ? card : undefined });
    const [cardId = card] = await localIds(context, target, [card]);
    const read = await readPage(context, target.board, target.page);
    const found = read.page.cards.find((item) => item.id === cardId);
    if (!found) {
      throw cant("shape the card", `there is no card with id “${cardId}” on the page “${read.page.title}”`, "read_page lists the page's cards with their ids.");
    }
    if (!("block" in found.ref)) {
      throw cant("shape the card", `${cardId} is an app's or entrance's card, which draws no flow of its own`, "Shape the cards of its code; read_page lists them.");
    }
    const key = found.ref.block;
    const labels = await labelsOf(context, target.project, key);
    const paths: AgentCallPath[] | undefined = open === undefined ? undefined : await Promise.all(open.map((path) => openPath(context, target.project, key, labels, path)));
    const op: ShapeCardOp = {
      op: "shape_card",
      card: cardId,
      ...defined({ title, description, replacePeopleText }),
      ...(names !== undefined ? { names: Object.entries(names).map(([name, text]) => ({ key: labels.key(name), text })) } : {}),
      ...(combine !== undefined
        ? { combine: combine.map((step) => ({ members: step.steps.map((member) => labels.key(member)), title: step.title, ...defined({ collapsed: step.collapsed }) })) }
        : {}),
      ...(paths !== undefined ? { open: paths } : {}),
    };
    let edit: AppliedEdit;
    try {
      edit = await applyEdit(context, target, { title: "Shape card", op, expectRevision: revision });
    } catch (error) {
      if (error instanceof ToolError && SHAPING_REFUSALS.has(error.code)) {
        const lines = await choices(context, target.project, key, labels);
        throw new ToolError([error.message, ...lines].join("\n"), error.code);
      }
      throw error;
    }
    const flow = await shapedFlow(context, target, edit, cardId, key);
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("shaped", edit.items), ...edit.items.map(itemLine), "The card as the person sees it now:", flow],
      "shape_card the next card the page is about, then frame_cards, add_resources, and connect to compose the page; arrange_page and check.",
    );
  },
});
