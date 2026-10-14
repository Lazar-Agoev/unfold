/**
 * Shape and explain (design §6.7): `frame_cards`, `add_resources`, `connect`, `add_notes`, `describe`,
 * and `propose` (proposed steps, on request only).
 * Each is one agent edit of one page (one undo step in the window that holds the board, saved at once).
 * The save checks what agents claim: resource cards and connections must cite code (anchors) whose
 * verified text holds their names, and a refusal says what failed and what to cite instead. A proposed
 * step is an intention placed after a block or at the start of a body, never code.
 */

import { z } from "zod";
import type { AgentProposalSpec } from "../../../shared/agent-api";
import { anchorArg, applyEdit, defined, editResult, editTarget, expectRevision, itemLine, itemsHeading, localIds, pageArgs } from "../edits";
import { isLink } from "../target";
import { defineTool } from "../tool";

/** What a tool without `replacePeopleText` says when the window refuses to replace a person's words. */
function keepWords(tool: string, what: string): string {
  return `${tool} keeps them: pass ${what} as they are, or ask the person to change them.`;
}

// ---------------------------------------------------------------------------------------------
// frame_cards

export const frameCards = defineTool({
  name: "frame_cards",
  title: "Frame cards",
  description:
    'Gathers items on a page into frames: named, described boxes that can nest and collapse. look "box" (the default) is for a unit of several functions ("Request checks") or a service ("Order API"); look "zone" is a tinted background for an area that holds services ("Backend"). members are ids of cards, resource cards, notes, or frames on this page. An item can be in one frame, and joins one from no frame, from the frame itself, or from its parent; frames nest at most three deep (zone > box > box). Pass parent to nest, direction to lay members out right or down, and an icon: api, worker, schedule, cli, web, library, or system. Titles are 1 to 4 words; a description says in one sentence what the frame does. Frames start expanded: collapse one only when the person asked, your brief said, or check finds the page too dense. Passing an existing id updates that frame: members it no longer lists go to the frame that holds it. Read get_guide("diagrams") first. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    frames: z
      .array(
        z.object({
          id: z.string().min(1).optional().describe("An existing frame's id updates it; absent: a new frame with an id from its title"),
          title: z.string().trim().min(1).max(80),
          description: z.string().trim().min(1).max(400).optional(),
          members: z.array(z.string().min(1)).min(1).describe("Ids of cards, resource cards, notes, or frames of the page"),
          look: z.enum(["box", "zone"]).optional(),
          icon: z.enum(["api", "worker", "schedule", "cli", "web", "library", "system"]).optional(),
          parent: z.string().min(1).nullable().optional().describe("The frame to nest it in; null takes it out of every frame"),
          direction: z.enum(["right", "down"]).optional(),
          collapsed: z.boolean().optional(),
        }),
      )
      .min(1),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, frames, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Frame cards",
      op: { op: "frame_cards", frames: defined(frames) },
      expectRevision: revision,
      peopleAdvice: keepWords("frame_cards", "a frame's title and description (you may still change its members, look, and collapse)"),
    });
    return editResult(context, target, edit, [itemsHeading("frames", edit.items), ...edit.items.map(itemLine)], "arrange_page, then check.");
  },
});

// ---------------------------------------------------------------------------------------------
// add_resources

const column = z.object({
  name: z.string().trim().min(1),
  type: z.string().trim().min(1).optional().describe("Its type as written (only on a table without type)"),
  key: z.enum(["primary", "foreign", "unique"]).optional(),
  references: z.string().trim().min(1).optional().describe("What a foreign key references (customers.id)"),
  optional: z.boolean().optional(),
});

export const addResources = defineTool({
  name: "add_resources",
  title: "Add resources",
  description:
    'Adds resource cards for what code talks to that isn\'t a function: kind table, queue, cache, storage, external (a service outside the project: another repository, or one such as Stripe), or actor (who starts things: a customer, a browser). name is the technical name ("orders"). Every kind but actor needs anchors that cite it: a block or entrance key ({key}: where the queue is created, the model class, the client) or an evidence file\'s lines ({file, lines: [from, to]}: a SQL migration, a Prisma schema). Unfold checks that the name, and every column a card lists itself, appear in the verified text of its anchors, and refuses what it can\'t find, saying what to cite. A table backed by a type (type: an interface\'s, dataclass\'s, or model class\'s key, from search or read_code) takes its columns from the type\'s fields, compiled; columns then only mark fields: {name, key: primary | foreign | unique, references}. Without type, list the columns yourself. If you can\'t cite it, don\'t add it: say what you found in the chat. Passing an existing id updates that card. frame puts it in a frame; near places it beside an item. Read get_guide("data-models") for tables. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    resources: z
      .array(
        z.object({
          id: z.string().min(1).optional().describe("An existing resource card's id updates it"),
          kind: z.enum(["table", "queue", "cache", "storage", "external", "actor"]),
          name: z.string().trim().min(1).max(120).describe("The technical name, as the code writes it"),
          type: z.string().min(1).optional().describe("A table's type: a type declaration's or class's key"),
          description: z.string().trim().min(1).max(400).optional(),
          columns: z.array(column).optional(),
          anchors: z.array(anchorArg).optional().describe("What it cites"),
          frame: z.string().min(1).optional().describe("A frame's id to put it in"),
          near: z.string().min(1).optional().describe("An item's id to place it beside"),
        }),
      )
      .min(1),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, resources, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Add resources",
      op: { op: "add_resources", resources: defined(resources) },
      expectRevision: revision,
      peopleAdvice: keepWords("add_resources", "a resource card's name and description"),
    });
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("resource cards", edit.items), ...edit.items.map(itemLine)],
      "connect the code that reads, writes, or sends to them, then arrange_page and check.",
    );
  },
});

// ---------------------------------------------------------------------------------------------
// connect

export const connect = defineTool({
  name: "connect",
  title: "Connect",
  description:
    'Draws typed connections for what the compiled graph can\'t show: how code talks through HTTP, queues, and data. A connection that repeats a compiled call (a call Unfold already draws as an arrow between the two cards) is refused: get_relations says what Unfold draws. kind: http, queue, reads, writes, calls (where an unresolved call goes at runtime), relates (between two table columns), or other (needs a label). from is the side that acts; step is the call it leaves from (a call\'s key from read_code), so the line starts where the code talks. anchors cite the receiving side: the worker entrance, the handler\'s registration, the route, the queue\'s creation. If you can\'t cite both sides, don\'t draw it: say what you found in the chat. When to is an entrance, Unfold fills in the channel from it. carries names the type that travels (a type key written in a type annotation on either side, such as src/types.ts#OrderJob). description, the evidence in one sentence, is required. relates takes fromColumn, toColumn, and a cardinality; calls takes the unresolved call itself as its step. Read get_guide("connections") first. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    connections: z
      .array(
        z.object({
          id: z.string().min(1).optional().describe("An existing connection's id updates it"),
          kind: z.enum(["http", "queue", "reads", "writes", "calls", "relates", "other"]),
          from: z.string().min(1).describe("The id of the side that acts: a card, resource card, or frame"),
          step: z.string().min(1).optional().describe("The key of the call it leaves from, inside from's code"),
          to: z.string().min(1).describe("The id of the other side"),
          fromColumn: z.string().min(1).optional().describe("relates: from's column"),
          toColumn: z.string().min(1).optional().describe("relates: to's column"),
          cardinality: z.enum(["one-to-one", "one-to-many", "many-to-one", "many-to-many"]).optional(),
          channel: z.string().trim().min(1).optional().describe("What travels: a route, a queue, an event (filled in when to is an entrance)"),
          carries: z.string().min(1).optional().describe("The key of the type it carries"),
          label: z.string().trim().min(1).max(120).optional().describe('What travels, in words ("place-order job")'),
          description: z.string().trim().min(1).max(400).describe("The evidence, in one sentence"),
          anchors: z.array(anchorArg).optional().describe("What it cites on the receiving side"),
        }),
      )
      .min(1),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, connections, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Connect",
      op: { op: "connect", connections: defined(connections) },
      expectRevision: revision,
      peopleAdvice: keepWords("connect", "a connection's label and description"),
    });
    return editResult(context, target, edit, [itemsHeading("connections", edit.items), ...edit.items.map(itemLine)], "arrange_page, then check.");
  },
});

// ---------------------------------------------------------------------------------------------
// add_notes

export const addNotes = defineTool({
  name: "add_notes",
  title: "Add notes",
  description:
    'Adds notes: short plain text on a page for what a reader would otherwise miss, such as a caveat, a reading order, or an open question ("Queued orders are placed by the worker. The client gets a job id, not the order."). Two sentences at most, 400 characters. attach names a card, frame, resource card, or connection the note moves with; without it the note is free, placed beside near or right of everything. Notes aren\'t checked against code and have no health, so never use one to claim what code does: give the card a description with shape_card, or connect with anchors. Add notes sparingly; get_guide("writing") says what makes a good one. Passing an existing id updates that note. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    notes: z
      .array(
        z.object({
          id: z.string().min(1).optional().describe("An existing note's id updates it"),
          text: z.string().trim().min(1).max(400),
          attach: z.string().min(1).optional().describe("The id of the card, frame, resource card, or connection it moves with"),
          near: z.string().min(1).optional().describe("An item's id to place a free note beside"),
        }),
      )
      .min(1),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, notes, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Add notes",
      op: { op: "add_notes", notes: defined(notes) },
      expectRevision: revision,
      peopleAdvice: keepWords("add_notes", "a person's note"),
    });
    return editResult(context, target, edit, [itemsHeading("notes", edit.items), ...edit.items.map(itemLine)], "check, then get_screenshot.");
  },
});

// ---------------------------------------------------------------------------------------------
// describe

export const describe = defineTool({
  name: "describe",
  title: "Describe",
  description:
    'Writes descriptions of what isn\'t a card: one sentence on what an item does and why, not how ("Checks the order before anything is saved."), never a paragraph that restates the code. items name combined steps, frames, resource cards, and connections by id (or a link). A card\'s description is shape_card\'s. A note\'s text is what it says: change it with add_notes. Unfold signs each text against the code it describes, so it goes to review when that code changes. Replacing a person\'s text is refused unless replacePeopleText is true: set it only when the person asked for that change. Writing the same text again keeps it. Read get_guide("writing") first. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    items: z
      .array(
        z.object({
          id: z.string().min(1).describe("The id of a combined step, frame, resource card, or connection, or a link to it"),
          text: z.string().trim().min(1).max(400),
        }),
      )
      .min(1),
    replacePeopleText: z.boolean().optional().describe("Only when the person asked you to replace their words"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, items, replacePeopleText, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project, item: items.map((item) => item.id).find(isLink) });
    const ids = await localIds(
      context,
      target,
      items.map((item) => item.id),
    );
    const edit = await applyEdit(context, target, {
      title: "Describe",
      op: { op: "describe", items: items.map((item, index) => ({ id: ids[index] ?? item.id, text: item.text })), ...(replacePeopleText !== undefined ? { replacePeopleText } : {}) },
      expectRevision: revision,
    });
    return editResult(context, target, edit, [itemsHeading("described", edit.items), ...edit.items.map(itemLine)], "check, then get_screenshot.");
  },
});

// ---------------------------------------------------------------------------------------------
// propose

const proposalArg = z
  .object({
    id: z.string().min(1).optional().describe("An existing proposed step's id updates it; absent: a new one (or the page's one with the same spot and title)"),
    after: z.string().min(1).optional().describe("The key of the block it goes after (read_code lists the keys)"),
    start: z.string().min(1).optional().describe("The key of a definition or a part: it goes at the start of its body"),
    title: z.string().trim().min(1).max(120).optional().describe("What it would do, 1 to 120 characters; needed for a new one"),
    reason: z.string().trim().min(1).max(400).nullable().optional().describe("Why it's wanted, 1 to 400 characters; null clears it"),
  })
  .refine((spec) => spec.after === undefined || spec.start === undefined, "A proposed step goes after a block or at the start of a body: pass after or start, not both.");

function proposalOf({ after, start, ...words }: z.output<typeof proposalArg>): AgentProposalSpec {
  if (after !== undefined) {
    return { ...defined(words), after };
  }
  return start !== undefined ? { ...defined(words), start } : defined(words);
}

export const propose = defineTool({
  name: "propose",
  title: "Propose steps",
  description:
    'Adds or changes proposed steps on a page, only when the person asks for an intended change ("send a confirmation email after placing the order"); explaining existing code never adds proposals, and you never invent missing behavior. A proposed step is a title, an optional reason, and where it would go: after a block (after: its key from read_code), or at the start of the body of a definition or a part (start: its key). It applies page-wide, wherever that block is drawn. Unfold draws it dashed green as PROPOSED beside the compiled path, which stays as it is: it is not code, and nothing marks it built. Title: the intended behavior in 1 to 120 characters ("Email a confirmation"); reason: why, in a sentence. No status, completion, or "would call": once the code exists, remove_items takes the proposal off (that never edits code). Passing an existing id updates it: a new after or start moves it, keeping its author (the explicit repair when check finds its anchor missing or held); reason: null clears the reason. Changing a person\'s title or reason is refused unless replacePeopleText is true. One undo step for the whole call.',
  input: z.object({
    ...pageArgs,
    proposals: z.array(proposalArg).min(1).max(50),
    replacePeopleText: z.boolean().optional().describe("Only when the person asked you to change their proposed step's words"),
    expectRevision,
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, project, proposals, replacePeopleText, expectRevision: revision }, context) {
    const target = await editTarget(context, { board, page, project });
    const edit = await applyEdit(context, target, {
      title: "Propose steps",
      op: { op: "propose", proposals: proposals.map(proposalOf), ...(replacePeopleText !== undefined ? { replacePeopleText } : {}) },
      expectRevision: revision,
    });
    return editResult(
      context,
      target,
      edit,
      [itemsHeading("proposed steps", edit.items), ...edit.items.map(itemLine), "Proposed steps are intentions, not code: the compiled path stays as it is."],
      "get_screenshot of the page, then tell the person in chat what you proposed and why.",
    );
  },
});
