/**
 * Read the code: `read_code` and `get_relations`.
 *
 * `read_code` prints a definition's compiled tree (`code_text`): each block with its short label and
 * full key beside the line it starts on, what it calls, binds, or tests, and the code between blocks;
 * a type's or class's members (`type_definition`), paged with the tree; and an app's or entrance's
 * code (`card`). `get_relations` pages callers and calls (`relations`) and a type's uses
 * (`type_uses`) with one cursor, and lists the board pages that show it (`board_refs`).
 */

import { z } from "zod";
import type { CallerView, CardView, LinkEnd, OutgoingView, StartCard, TypeDefinition, TypeMember, TypeUsePlace } from "../engine-types";
import { count, place, typeText } from "../format";
import { badCursor, cursorOffset, nextLine, pagedLines, textResult } from "../results";
import { fitting, linesLength, moreLine, section, type Section } from "../sections";
import { codeTarget } from "../target";
import { defineTool } from "../tool";

/** Lines of the compiled tree, then of a type's members, per page (fewer when they are long). */
const LINES_PER_PAGE = 150;

/** Where a call or a type name goes, in words. */
function endText(to: LinkEnd, name?: string): string {
  switch (to.outcome) {
    case "project":
      return to.key === undefined ? "a project definition" : `${name ?? to.key} (key ${to.key})`;
    case "library":
      return `the library ${to.note ?? to.module ?? "call"}`;
    case "builtin":
      return "a builtin";
    case "unknown":
      return `unknown: ${to.note ?? to.reason ?? "unresolved"}`;
  }
}

/** One call or link a definition makes, as `get_relations` lists it. */
function callLine(call: OutgoingView): string {
  return `- line ${call.line}: ${call.kind} ${call.text} → ${endText(call.to, call.targetName)}; key ${call.call}`;
}

/**
 * Where `get_relations` continues: the offset of every list's next stretch and, once callers were
 * shown, the engine's cursor of the last one, so the engine reads the next callers from there
 * (`20`, or `20:` and that cursor).
 */
interface RelationsCursor {
  readonly offset: number;
  readonly callersAfter?: string;
}

function readRelationsCursor(cursor: string | undefined): RelationsCursor {
  const split = cursor?.indexOf(":") ?? -1;
  if (cursor === undefined || split === -1) {
    return { offset: cursorOffset(cursor) };
  }
  if (!/^\d+$/.test(cursor.slice(0, split))) {
    throw badCursor(cursor);
  }
  return { offset: Number(cursor.slice(0, split)), callersAfter: cursor.slice(split + 1) };
}

function writeRelationsCursor(offset: number, callersAfter: string | undefined): string {
  return callersAfter === undefined ? String(offset) : `${offset}:${callersAfter}`;
}

function callerLine(caller: CallerView): string {
  return `- ${caller.ownerName} at ${place(caller.path, caller.line)}${caller.kind === "calls" ? "" : `, ${caller.kind}`} (key ${caller.owner}; call ${caller.call})`;
}

function useLine(use: TypeUsePlace): string {
  return `- ${use.label} at ${place(use.path, use.line)}${use.kind === "annotation" ? "" : `, ${use.kind}`} (key ${use.owner})`;
}

function memberLine(member: TypeMember): string {
  const name = `${member.readonly ? "readonly " : ""}${member.name ?? `(${member.kind} signature)`}${member.optional ? "?" : ""}`;
  if (member.kind === "value") {
    return `- ${name}${member.value !== undefined ? ` = ${member.value}` : ""}`;
  }
  const kind = member.kind === "property" ? "" : `${member.kind} `;
  return `- ${kind}${name}${member.type ? `: ${typeText(member.type)}` : ""}${member.default ? " (has a default)" : ""}`;
}

/** A type's or class's written members, inherited members by base, aliased type, and uses. */
function definitionLines(definition: TypeDefinition): string[] {
  return [
    definition.aliased ? `Aliased type: ${typeText(definition.aliased)}` : null,
    definition.members.length > 0 ? `Members (${definition.members.length}):` : definition.aliased ? null : "No members are written.",
    ...definition.members.map(memberLine),
    ...definition.inherited.flatMap((group) => [
      `Inherited from ${typeText(group.from)}${group.members.length === 0 ? ": no members Unfold can read" : ` (${group.members.length}):`}`,
      ...group.members.map(memberLine),
    ]),
    `Used in ${count(definition.uses, "place")} (get_relations lists them).`,
  ].filter((line): line is string => line !== null);
}

/** An app's or entrance's card: what starts it, and the code it starts at, each with its state. */
function startLines(card: StartCard): string[] {
  return [
    `${card.kind === "app" ? "App" : `Entrance of ${card.app}`} ${card.name}; key ${card.key}.`,
    card.on !== undefined ? `${card.kind === "app" ? "Run" : "On"}: ${card.on}` : null,
    card.about !== undefined ? `About: ${card.about}` : null,
    `Code, in the order it runs (${card.code.length}):`,
    ...card.code.map(
      (target, index) =>
        `${index + 1}. ${target.label ?? target.key} (key ${target.key}): ${target.state.replaceAll("_", " ")}${target.path !== undefined ? ` at ${place(target.path, target.line)}` : ""}`,
    ),
    "Next: add_cards with this key puts it on a page with its code (follow: {callees: 1}); create_board with trace makes a board from it.",
  ].filter((line): line is string => line !== null);
}

const cursorArg = z.string().optional().describe("The next cursor a result gave (absent: the first page)");

export const readCode = defineTool({
  name: "read_code",
  title: "Read code",
  description:
    "Reads a module, function, method, class, or type as Unfold compiled it, or an app or entrance, by key (from search, list_entrances, read_page, or get_relations) or a link to a card; without key, the card selected in the person's view. A definition: its compiled tree, one line per block with the line it starts on, its short label (try[0]/except[0]) and full key, and how it is drawn: ▸ a project call (a box, with its target's key), ? an unknown call (with the reason), builtin, binds, tests, and exit values; ⋯ marks code that isn't a block. shape_card takes the short labels or the keys; propose, connect, and read_call take the keys. A type or class: its members with written types and what each type name links to, inherited members by base, and how many places use it. An app or entrance: what starts it and the code it starts at, with each key's state. Long trees and member lists come in pages: pass cursor. Reading changes nothing.",
  input: z.strictObject({
    key: z.string().min(1).optional().describe("The code's key, or a link to a card (default: the selected card)"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    cursor: cursorArg,
  }),
  annotations: { readOnlyHint: true },
  async run({ key, project, cursor }, context) {
    const code = await codeTarget(context, { key, project });
    const id = code.project.id;
    const card: CardView = await context.engine("card", { project: id, key: code.key });
    const head = context.header({ project: code.project.name });
    if (card.type === "start") {
      return textResult(head, ...startLines(card));
    }
    const { header } = card.flow;
    const isType = header.kind === "type" || header.kind === "class";
    const [tree, definition, relations] = await Promise.all([
      context.engine("code_text", { project: id, key: code.key }),
      isType ? context.engine("type_definition", { project: id, key: code.key }) : null,
      header.kind === "type" ? null : context.engine("relations", { project: id, key: code.key, callersLimit: 1 }),
    ]);
    // A type's members continue the tree's lines, so one cursor pages both.
    const treeLines = tree.text.trimEnd().split("\n");
    const shown = pagedLines([...treeLines, ...(definition ? definitionLines(definition) : [])], cursor, LINES_PER_PAGE);
    const offset = cursorOffset(cursor);
    return textResult(
      head,
      `${header.form} ${header.label} at ${place(header.path, header.line)}; key ${header.key}.`,
      code.step !== undefined ? `Selected in the person's view: the block ${code.step}.` : null,
      offset < treeLines.length ? `Compiled tree:\n${shown.items.join("\n")}` : shown.items.join("\n"),
      nextLine(shown, "read_code"),
      relations && offset === 0 ? `Called from ${count(relations.callersTotal, "place")}.` : null,
      `Next: read_call with a call's key for what it sends; get_relations for callers${isType ? " and uses" : ""}; add_cards puts it on a page.`,
    );
  },
});

// ---------------------------------------------------------------------------------------------
// get_relations

export const getRelations = defineTool({
  name: "get_relations",
  title: "Get relations",
  description:
    "How a module, function, method, class, or type connects: who calls it (each caller's name, file:line, and the call's key), what it calls and links to (project, library, builtin, or unknown), which entrances reach it (at least: what the graph proves), and which board pages already show it. For a type or class: the places that use it (each use's owner, file:line, and key, with extends and implements marked). key comes from search, read_code, read_page, or a link to a card; without key, the card selected in the person's view. Callers, calls, and uses come in pages: limit is per list (default 20, at most 100; a page of long lines shows fewer), and the next cursor pages them all. Use it to choose what goes on a page, and before drawing a connection, to check whether Unfold already draws that call. Reading changes nothing.",
  input: z.strictObject({
    key: z.string().min(1).optional().describe("The code's key, or a link to a card (default: the selected card)"),
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    limit: z.number().int().min(1).max(100).default(20).describe("Callers, calls, and uses per page"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ key, project, limit, cursor }, context) {
    const code = await codeTarget(context, { key, project });
    const { offset, callersAfter } = readRelationsCursor(cursor);
    const id = code.project.id;
    const card = await context.engine("card", { project: id, key: code.key });
    const kind = card.type === "definition" ? card.flow.header.kind : null;
    const name = card.type === "definition" ? (card.flow.header.name ?? card.flow.header.label) : card.name;
    const isType = kind === "type" || kind === "class";
    const [relations, uses, boards] = await Promise.all([
      kind === null || kind === "type" ? null : context.engine("relations", { project: id, key: code.key, ...(callersAfter === undefined ? {} : { callersAfter }), callersLimit: limit }),
      isType ? context.engine("type_uses", { project: id, key: code.key, offset, limit }) : null,
      context.engine("board_refs", { project: id, key: code.key }),
    ]);
    const calls = relations ? relations.outgoing.toSorted((a, b) => a.line - b.line) : [];
    const head = [
      context.header({ project: code.project.name }),
      `${name}: key ${code.key}.`,
      card.type === "start" ? "An app or entrance: read_code lists its code; list_entrances, what else starts the project." : null,
      relations?.callersTotal === 0 && offset === 0 ? "No caller in the graph." : null,
      uses?.total === 0 && offset === 0 ? "No place uses it in the graph." : null,
    ];
    const tail = [
      relations && offset === 0
        ? relations.reachedFrom.length > 0
          ? `Reached at least from: ${relations.reachedFrom.map((from) => `${from.name} in ${from.app} (key ${from.key})`).join(", ")}.`
          : "No entrance reaches it in the graph."
        : null,
      boards.refs.length > 0
        ? [`On boards${boards.truncated ? " (the first 50)" : ""}:`, ...boards.refs.map((ref) => `- ${ref.title} › ${ref.pageTitle} (board id ${ref.board}, page id ${ref.page})`)].join("\n")
        : "No board shows it yet.",
    ];
    // Callers come from the engine a page at a time after the cursor's caller, uses from its offset;
    // calls all at once.
    const page = fitting(limit, linesLength([...head, ...tail]), (size) =>
      [
        relations ? section("Callers", relations.callers, relations.callersTotal, offset, size, callerLine) : null,
        section("Calls and links", calls.slice(offset), calls.length, offset, size, callLine),
        uses ? section("Uses", uses.places, uses.total, offset, size, useLine) : null,
      ].filter((item): item is Section => item !== null),
    );
    const lastCaller = relations?.callers.slice(0, page.limit).at(-1);
    const next = writeRelationsCursor(offset + page.limit, lastCaller?.cursor ?? callersAfter);
    return textResult(
      ...head,
      ...page.sections.flatMap((item) => item.lines),
      ...tail,
      moreLine(page.sections, "get_relations", next),
      "Next: read_code with a key; add_cards puts code on a page.",
    );
  },
});
