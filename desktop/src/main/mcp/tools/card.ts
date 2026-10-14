/**
 * Shape a card: `shape_card`, a card's whole shaping in one call.
 *
 * The agent names blocks by short label (`try[0]/except[0]`, `call:save_order`) or full key. A short
 * label names one of the card's own blocks, by lookup in the card's label map (`code_text`); a block of
 * a call the card opens in place is named by its full key, except in an open path, whose later blocks
 * map in the label map of what the block before it opens (the callee drawn in place, as `card` draws
 * it). An app's or entrance's card, or a type card, draws no flow: it takes only a description. The
 * window applies the shaping as one undo step, and the board's save checks every key and the overlay
 * rules. The result is the shaped card as `flow_text` prints it; a refusal of a key or a rule lists the
 * blocks the agent can choose from instead.
 */

import { z } from "zod";
import type { AgentCallPath, ShapeCardOp } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { CardView, Page, ProjectRow } from "../engine-types";
import { applyEdit, defined, editResult, editTarget, expectRevision, itemLine, itemsHeading, localIds, pageArgs, type AppliedEdit, type EditTarget } from "../edits";
import { choiceLines, Labels, openedDefinitions, openedFlow } from "../labels";
import { cardKey, overlayArgs } from "../page-code";
import { cant, ToolError } from "../results";
import { isLink, readPage } from "../target";
import { defineTool } from "../tool";

/** The refusal that comes from the shaping itself (the save's check of a key or an overlay rule), which the blocks to choose from answer. */
const SHAPING_REFUSAL = "board_problem";

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

/** The card of `card` as `card` draws it with the calls `open` opens in place; none when it can't be read. */
async function drawnCard(context: ToolContext, project: ProjectRow, card: string, open: readonly AgentCallPath[] = []): Promise<CardView | undefined> {
  try {
    return await context.engine("card", { project: project.id, key: card, ...(open.length > 0 ? { open: open.map((path) => [...path]) } : {}) });
  } catch (error) {
    if (error instanceof ToolError) {
      return undefined;
    }
    throw error;
  }
}

/** The definition that the last block of `path` opens in place on the card of `card`, as `card` draws it. */
async function openedDefinition(context: ToolContext, project: ProjectRow, card: string, path: readonly string[]): Promise<string | undefined> {
  const last = path.at(-1);
  const view = await drawnCard(context, project, card, [path]);
  return view?.type === "definition" && last !== undefined ? openedFlow(view.flow, last)?.header.key : undefined;
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

/**
 * Refuses a short label that isn't one of the card's own but is one of a definition the card opens in
 * place (`open`, as it will be saved): short labels name the card's own blocks, and a block of an
 * opened call goes by its full key.
 */
async function refuseOpenedLabels(context: ToolContext, project: ProjectRow, card: string, labels: Labels, blocks: readonly string[], open: readonly AgentCallPath[]): Promise<void> {
  const foreign = blocks.filter((block) => !labels.has(block));
  if (foreign.length === 0 || open.length === 0) {
    return;
  }
  const view = await drawnCard(context, project, card, open);
  const opened = view?.type === "definition" ? openedDefinitions(view.flow) : [];
  for (const definition of opened) {
    const theirs = await labelsOf(context, project, definition);
    const block = foreign.find((label) => theirs.has(label));
    if (block !== undefined) {
      throw cant(
        "shape the card",
        `“${block}” is a short label of ${definition}, a call the card opens in place, not of the card's own code`,
        `Short labels name the card's own blocks; name a block of an opened call by its full key: ${theirs.key(block)}.`,
      );
    }
  }
}

/** A card that draws no flow, which takes only a description: why, and what to shape instead. */
interface Flowless {
  readonly why: string;
  readonly instead: string;
}

/** Why a card takes only a description (an app's or entrance's card, a type card); undefined for a card that draws a flow. */
function flowless(card: Page["cards"][number], view: CardView | undefined): Flowless | undefined {
  if (!("block" in card.ref)) {
    return { why: `${card.id} is an app's or entrance's card, which draws no flow of its own`, instead: "On it, shape_card writes only description; shape the cards of its code for the rest." };
  }
  return view?.type === "definition" && view.flow.header.kind === "type"
    ? { why: `${card.id} is a type card, which shows fields, not a flow`, instead: "On it, shape_card writes only description; shape a function's, a class's, or a module's card for the rest." }
    : undefined;
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
    'Shapes one expanded card in one call, so its code-shaped flow reads as the few named steps a person would draw: title (names the card), description (one sentence on what it does and why), names ({block: text} on any block: a call, a branch as a yes-or-no question, a loop, an except part, an exit, a callback; null takes a name off), combine ([{steps, title, collapsed}]: consecutive steps of one body, or one whole control block, under one title; never overlapping or nesting; collapsed by default, and it keeps its ways out and its unknown-call count), and open ([[blocks]]: the calls or callbacks it opens in place, each a path of 1 to 3, outermost first, the next a call of what the one before opens; at most 64; it replaces what the card opens, [] closes them). Short labels from read_code (try[0]/except[0], call:save_order) name the card\'s own blocks; name a block of a call it opens in place by its full key (in open, each next block may be a short label of what the one before opens). Name only what the code doesn\'t say; naming a library or unknown call lifts it into a step, and it stays marked ? (a name never resolves a call). Names and combined steps apply page-wide, wherever that code is drawn. Changing or removing a person\'s name, title, or description is refused unless replacePeopleText is true: set it only when the person asked. An app\'s or entrance\'s card, or a type card, draws no flow: on it, shape_card writes only description. One undo step; the result is the shaped card as the person sees it. A refusal of a key or an overlay rule lists the blocks you can choose from. Read get_guide("diagrams") first.',
  input: z.object({
    ...pageArgs,
    card: z.string().min(1).describe("The card's id (read_page lists them), or a link to it"),
    title: z.string().trim().min(1).max(120).optional().describe("The card's title: the page's name for its definition"),
    description: z.string().trim().min(1).max(400).optional().describe("One sentence on what the card does and why (the only field an app's, entrance's, or type card takes)"),
    names: z.record(block, z.string().trim().min(1).max(120).nullable()).optional().describe("Names by block (a short label of the card's own code, or a key); null takes a name off"),
    combine: z
      .array(
        z.object({
          steps: z.array(block).min(1).describe("The blocks it combines, in written order (short labels of the card's own code, or keys)"),
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
    const key = cardKey(found);
    const drawn = await drawnCard(context, target.project, key);
    const onlyDescribed = flowless(found, drawn);
    if (onlyDescribed !== undefined) {
      if (title !== undefined || names !== undefined || combine !== undefined || open !== undefined) {
        throw cant("shape the card", onlyDescribed.why, onlyDescribed.instead);
      }
      const edit = await applyEdit(context, target, { title: "Describe card", op: { op: "shape_card", card: cardId, ...defined({ description, replacePeopleText }) }, expectRevision: revision });
      return editResult(context, target, edit, [itemsHeading("shaped", edit.items), ...edit.items.map(itemLine)], "shape_card the next card the page is about, then compose the page; arrange_page and check.");
    }
    const labels = await labelsOf(context, target.project, key);
    const paths: AgentCallPath[] | undefined = open === undefined ? undefined : await Promise.all(open.map((path) => openPath(context, target.project, key, labels, path)));
    const named = [...Object.keys(names ?? {}), ...(combine ?? []).flatMap((step) => step.steps)];
    await refuseOpenedLabels(context, target.project, key, labels, named, paths ?? found.open ?? []);
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
      if (error instanceof ToolError && error.code === SHAPING_REFUSAL) {
        const lines = drawn?.type === "definition" ? choiceLines(drawn.flow, labels) : [];
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
