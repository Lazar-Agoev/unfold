/**
 * Build a page (design §6.6): `add_cards`, `set_view`, `arrange_page`, and `remove_items`. Each is one
 * agent edit of one page (one undo step in the window that holds the board, saved at once); the
 * window places cards, lays out, checks calls to expand in place against the compiled models, and
 * applies the document's rules, and the result names what it did. A page opens in Diagram or Code;
 * in Diagram each card is compact or expanded, and a card's older `view` stays accepted (Diagram is
 * compact, Flow or Code expanded).
 */

import { z } from "zod";
import type { AgentCardSpec, AgentItemResult, AgentMove } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import { applyEdit, defined, editResult, editTarget, expectRevision, itemLine, itemsHeading, localIds, otherProject, pageArgs, type EditTarget } from "../edits";
import { ToolError } from "../results";
import { isLink } from "../target";
import { defineTool } from "../tool";

const VIEW = z.enum(["diagram", "flow", "code"]);

/** What a page opens in, by its default view. */
const OPENS_IN = {
  diagram: "The page opens in Diagram.",
  code: "The page opens in Code, every card's source; expansions show in Diagram.",
  flow: "The page opens in Diagram, its cards without their own setting expanded.",
} as const;

/** The most keys a call path holds, and the most paths a card expands (the board format's limits). */
const MAX_PATH_KEYS = 3;
const MAX_PATHS = 64;

const callPath = z
  .array(z.string().min(1))
  .min(1)
  .max(MAX_PATH_KEYS, "A call path holds 1 to 3 keys: expand at most three levels, and follow a deeper call to its own card (add_cards).")
  .describe("1 to 3 keys, outermost first: a step's main call or a callback (read_code marks which expand, with their keys)");

const cardSpec = z.object({
  key: z.string().min(1).describe("A function's, method's, class's, type's, or entrance's key"),
  project: z.string().min(1).optional().describe("The project the code is in, by name or id (default: the board's own)"),
  expanded: z.boolean().optional().describe("Shows its detail in Diagram: a function's written flow, an entrance's evidence and targets (default: the page's default)"),
  view: VIEW.optional().describe('Older input: "diagram" is compact, "flow" or "code" expanded (Code is a page-wide lens)'),
  frame: z.string().min(1).optional().describe("A frame's id: the cards this adds go into it"),
  near: z.string().min(1).optional().describe("An item's id to place the card beside"),
  follow: z
    .object({
      callees: z.number().int().min(0).max(3).optional(),
      callers: z.number().int().min(0).max(2).optional(),
      types: z.number().int().min(0).max(2).optional(),
    })
    .optional()
    .describe("Levels of calls, callers, and fields' types to add with it"),
});

/** A card spec with its project as the window takes it: by id, absent for the board's own project. */
async function resolvedCard(context: ToolContext, target: EditTarget, spec: z.output<typeof cardSpec>): Promise<AgentCardSpec> {
  const { project, ...rest } = defined(spec);
  const id = await otherProject(context, target, project);
  return id === undefined ? rest : { ...rest, project: id };
}

export const addCards = defineTool({
  name: "add_cards",
  title: "Add cards",
  description:
    'Puts compiled code on a page as cards: functions, methods, classes, types (interfaces, type aliases, enums), and entrances, by key (from search, list_entrances, read_code, or get_relations). Unfold places each card near the cards it relates to, the way following a call does; call arrange_page afterwards for a clean layout. Use follow to bring a call tree: {callees: 2} adds what each card calls, two levels deep (callees up to 3, callers up to 2). Following stops at external and unresolved calls, and the result lists where it stopped. On a type, {types: 1} adds the types its fields link to (up to 2), and Unfold draws the links. Cards already on the page are kept, not duplicated, and their ids are returned. Pass expanded: true for the one or two functions the page is about (their written flow); keep context cards compact; type cards always show their fields. view is older input ("flow" or "code" expands). frame puts the cards a spec adds into a frame of the page; near places a card beside an item. Read get_guide("diagrams") before building a page. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    cards: z.array(cardSpec).min(1).max(40).describe("The cards to add"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, cards, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const specs = await Promise.all(cards.map((card) => resolvedCard(context, target, card)));
    const edit = await applyEdit(context, target, { title: "Add cards", op: { op: "add_cards", cards: specs }, expectRevision: revision });
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("cards", edit.items), ...edit.items.map(itemLine)],
      "set_view to expand the cards the page is about (and calls in place), arrange_page for a clean layout, then check.",
    );
  },
});

export const setView = defineTool({
  name: "set_view",
  title: "Set view",
  description:
    'Sets how things on a page show. defaultView is the lens the page opens in: "diagram" (new pages) or "code" (every card\'s source, page-wide). In Diagram a function\'s or entrance\'s card is compact or expanded: expanded: true shows a function\'s written flow, an entrance\'s evidence and ordered targets; a class\'s card is its fields compact and its detail expanded; type declaration and table cards keep their fields and take no expansion. expandCalls expands calls in place inside a function\'s card: each path is 1 to 3 keys, outermost first, each a step\'s main call that resolved or a callback written on a step (read_code marks which expand, with their keys); each next key is checked in the Flow of the block the one before leads to, and a one-call callback such as () => sendEmail(job.data) leads to its callee while the step keeps its wrapper. External and unresolved calls, unknown keys, a block already shown above, more than 3 levels, and more than 64 paths a card are refused, saying what to pass instead. collapseCalls collapses a path and everything inside it. folded folds a card to its title; a step group or a frame takes collapsed. view ("diagram", "flow", "code") is older input: "diagram" is compact, the others expanded. Expand the one or two functions the page is about and keep context compact; collapse only what the person asked for, your brief said, or check finds too dense. Sizes change: arrange_page afterwards. get_guide("diagrams") explains. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    items: z
      .array(
        z.object({
          id: z.string().min(1).describe("A card's, step group's, or frame's id, or a link to it"),
          expanded: z.boolean().optional().describe("A function's, class's, or entrance's card: its detail in Diagram"),
          view: VIEW.optional().describe('Older input: "diagram" is compact, "flow" or "code" expanded'),
          expandCalls: z.array(callPath).max(MAX_PATHS).optional().describe("A function's card: calls to expand in place"),
          collapseCalls: z.array(callPath).max(MAX_PATHS).optional().describe("A function's card: expanded calls to collapse, with what they hold"),
          folded: z.boolean().optional(),
          collapsed: z.boolean().optional(),
        }),
      )
      .default([])
      .describe("How each item shows"),
    defaultView: VIEW.optional().describe('The lens the page opens in: "diagram" or "code" (a legacy "flow" opens in Diagram with cards expanded)'),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, items, defaultView, expectRevision: revision }, context) {
    if (items.length === 0 && defaultView === undefined) {
      throw new ToolError("Pass items (how each card, step group, or frame shows), defaultView, or both.");
    }
    const target = await editTarget(context, { board, page, project, item: items.map((item) => item.id).find(isLink) });
    const ids = await localIds(
      context,
      target,
      items.map((item) => item.id),
    );
    const edit = await applyEdit(context, target, {
      title: "Set view",
      op: { op: "set_view", items: defined(items.map((item, index) => ({ ...item, id: ids[index] ?? item.id }))), ...(defaultView !== undefined ? { defaultView } : {}) },
      expectRevision: revision,
    });
    const coded = items.filter((item) => item.view === "code" && item.expanded === undefined).length;
    return editResult(
      context,
      target,
      edit,
      [
        edit.items.length > 0 ? itemsHeading("items", edit.items) : null,
        ...edit.items.map(itemLine),
        coded > 0 ? `Code is a page-wide lens (defaultView "code"): view "code" expanded ${coded === 1 ? "that card" : "those cards"} in Diagram.` : null,
        defaultView !== undefined ? OPENS_IN[defaultView] : null,
      ],
      "arrange_page (expansion changes sizes), then check.",
    );
  },
});

const move = z
  .object({
    id: z.string().min(1).describe("A card's, frame's, resource card's, or note's id"),
    at: z.tuple([z.number(), z.number()]).optional().describe("A canvas position [x, y] (a frame's: its box's top-left)"),
    rightOf: z.string().min(1).optional().describe("An item's id to sit just right of"),
    below: z.string().min(1).optional().describe("An item's id to sit just below"),
  })
  .refine((spec) => [spec.at, spec.rightOf, spec.below].filter((place) => place !== undefined).length === 1, "Each move takes exactly one of at, rightOf, or below.");

function moveOf({ id, at, rightOf, below }: z.output<typeof move>): AgentMove {
  if (at !== undefined) {
    return { id, at };
  }
  if (rightOf !== undefined) {
    return { id, rightOf };
  }
  if (below !== undefined) {
    return { id, below };
  }
  throw new ToolError("Each move takes exactly one of at, rightOf, or below.");
}

/** A moved item's line: the item, and where it is now. */
function movedLine(item: AgentItemResult): string {
  return item.kind === "card" && item.at ? `${itemLine(item)} at [${item.at.join(", ")}]` : itemLine(item);
}

export const arrangePage = defineTool({
  name: "arrange_page",
  title: "Arrange page",
  description:
    'Lays out a page the way Arrange does, from the inside out: each frame\'s members by call depth and connections in the frame\'s direction, then the page with each frame as one block; type cards after the types that link to them; resource cards by data flow. Pass frame to arrange only that frame\'s inside (its box keeps its top-left corner); direction "right" or "down" sets that frame\'s direction, or, without frame, lays out the page\'s top level that way for this call. Then moves apply in order, to cards, frames, resource cards, and notes: at puts the item (a frame: its box\'s top-left) at [x, y]; rightOf and below place it just beside another item, clear of everything else. Arranging the same page twice gives the same layout. Requests read left to right, data top to bottom. Call it after adding or changing cards, then check. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    frame: z.string().min(1).optional().describe("A frame's id: arrange only its inside"),
    direction: z.enum(["right", "down"]).optional().describe("The frame's direction, or the page's top level for this call"),
    moves: z.array(move).optional().describe("Moves applied after arranging, in order"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, frame, direction, moves, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Arrange page",
      op: { op: "arrange", ...(frame !== undefined ? { frame } : {}), ...(direction !== undefined ? { direction } : {}), ...(moves !== undefined ? { moves: moves.map(moveOf) } : {}) },
      expectRevision: revision,
    });
    return editResult(
      context,
      target,
      edit,
      edit.items.length > 0 ? [itemsHeading("moved", edit.items), ...edit.items.map(movedLine)] : ["Everything was already in place."],
      "check, then get_screenshot to see the layout.",
    );
  },
});

export const removeItems = defineTool({
  name: "remove_items",
  title: "Remove items",
  description:
    "Takes items off a page by id: cards (with their step groups, proposed steps, and the links that end at them), step groups (the statements stay), connections and drawn links, frames, resource cards (with their links), notes, and proposed steps (never code: the intention goes, the code stays as it is). A removed frame's members stay, in the frame that held it, unless keepMembers is false, which removes them too; a frame left with no members goes as well, and a note attached to something removed stays, free. Removing text a person wrote (a description, label, title, note, or proposed step) is refused unless replacePeopleText is true: set it only when the person asked for that. Everything removed can be undone in Unfold. The result lists what was removed, the items asked for first, then what went with them. One undo step for the whole call.",
  input: z.object({
    ...pageArgs,
    ids: z.array(z.string().min(1)).min(1).describe("The items' ids, or links to them"),
    keepMembers: z.boolean().optional().describe("For a frame: keep what it holds (the default) or remove it too"),
    replacePeopleText: z.boolean().optional().describe("Only when the person asked to remove items with their words on them"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ board, page, project, ids, keepMembers, replacePeopleText, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project, item: ids.find(isLink) });
    const local = await localIds(context, target, ids);
    const edit = await applyEdit(context, target, {
      title: "Remove items",
      op: {
        op: "remove_items",
        ids: local,
        ...(keepMembers !== undefined ? { keepMembers } : {}),
        ...(replacePeopleText !== undefined ? { replacePeopleText } : {}),
      },
      expectRevision: revision,
    });
    return editResult(context, target, edit, [itemsHeading("items", edit.items), ...edit.items.map(itemLine)], "arrange_page if gaps are left, then check.");
  },
});
