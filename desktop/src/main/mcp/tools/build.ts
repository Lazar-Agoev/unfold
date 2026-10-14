/**
 * Build a page (design §6.6): `add_cards`, `set_view`, `arrange_page`, and `remove_items`. Each is one
 * agent edit of one page (one undo step in the window that holds the board, saved at once); the
 * window places cards, lays out, checks calls to expand in place against the compiled models, and
 * applies the document's rules, and the result names what it did. Each card is compact or expanded.
 */

import { z } from "zod";
import type { AgentItemResult, AgentMove } from "../../../shared/agent-api";
import { applyEdit, cardRef, defined, editResult, editTarget, expectRevision, itemLine, itemsHeading, localIds, pageArgs, startKeys } from "../edits";
import { ToolError } from "../results";
import { isLink } from "../target";
import { defineTool } from "../tool";

const VIEW = z.enum(["compact", "expanded"]);

const cardSpec = z.object({
  key: z.string().min(1).describe("A module's, function's, method's, class's, or type's key, or an app's or entrance's"),
  view: VIEW.optional().describe("compact, or expanded: a function's drawn flow, an entrance's code (default: the page's default)"),
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

export const addCards = defineTool({
  name: "add_cards",
  title: "Add cards",
  description:
    'Puts compiled code on a page as cards: functions, methods, classes, types (interfaces, type aliases, enums), and entrances, by key (from search, list_entrances, read_code, or get_relations). Unfold places each card near the cards it relates to, the way following a call does; call arrange_page afterwards for a clean layout. Use follow to bring a call tree: {callees: 2} adds what each card calls, two levels deep (callees up to 3, callers up to 2). Following stops at external and unresolved calls, and the result lists where it stopped. On a type, {types: 1} adds the types its fields link to (up to 2), and Unfold draws the links. Cards already on the page are kept, not duplicated, and their ids are returned. Pass view "expanded" for the one or two functions the page is about (their drawn flow); keep context cards compact; type cards always show their fields. frame puts the cards a spec adds into a frame of the page; near places a card beside an item. Read get_guide("diagrams") before building a page. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    cards: z.array(cardSpec).min(1).max(40).describe("The cards to add"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, cards, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const starts = await startKeys(context, target.project.id);
    const specs = cards.map(({ key, ...spec }) => ({ ref: cardRef(starts, key), ...defined(spec) }));
    const edit = await applyEdit(context, target, { title: "Add cards", op: { op: "add_cards", cards: specs }, expectRevision: revision });
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("cards", edit.items), ...edit.items.map(itemLine)],
      "set_view to expand the cards the page is about, arrange_page for a clean layout, then check.",
    );
  },
});

export const setView = defineTool({
  name: "set_view",
  title: "Set view",
  description:
    'Sets how things on a page show. A card is compact or expanded: view "expanded" shows a function\'s drawn flow, an app\'s or entrance\'s code; a class\'s card is its fields compact and its class-level code expanded; type and table cards keep their fields and take no view. A combined step or a frame takes collapsed. Expand the one or two functions the page is about and keep context compact; collapse only what the person asked for, your brief said, or check finds too dense. Sizes change: arrange_page afterwards. get_guide("diagrams") explains. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    items: z
      .array(
        z.object({
          id: z.string().min(1).describe("A card's, combined step's, or frame's id, or a link to it"),
          view: VIEW.optional().describe("A card: compact or expanded"),
          collapsed: z.boolean().optional().describe("A combined step or a frame: collapsed or open"),
        }),
      )
      .min(1)
      .describe("How each item shows"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, items, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project, item: items.map((item) => item.id).find(isLink) });
    const ids = await localIds(
      context,
      target,
      items.map((item) => item.id),
    );
    const edit = await applyEdit(context, target, {
      title: "Set view",
      op: { op: "set_view", items: defined(items.map((item, index) => ({ ...item, id: ids[index] ?? item.id }))) },
      expectRevision: revision,
    });
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("items", edit.items), ...edit.items.map(itemLine)],
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
    "Takes items off a page by id: cards (with the connections that end at them), combined steps (the blocks stay), connections, frames, resource cards (with their connections), notes, and proposed steps (never code: the intention goes, the code stays as it is). A removed frame's members stay, in the frame that held it, unless keepMembers is false, which removes them too; a frame left with no members goes as well, and a note attached to something removed stays, free. Removing text a person wrote (a description, title, note, or proposed step) is refused unless replacePeopleText is true: set it only when the person asked for that. Everything removed can be undone in Unfold. The result lists what was removed, the items asked for first, then what went with them. One undo step for the whole call.",
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
