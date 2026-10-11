/**
 * `group_steps`: gathers consecutive statements of one card into step groups, by line range or by
 * step keys, or updates groups by id, or (without one) the card's group of the same statements, so
 * the same call twice changes nothing. The reader's rule decides what a range holds (`groupSelection`,
 * as the gutter and Make a group from lines use it): whole statements of one branch, never the card's
 * header or a nested definition, never overlapping another group. The result names the statements
 * each group took; a refusal says why a range breaks the rule and what is there.
 */

import type { AgentItemResult, AgentStatement, AgentStepGroupSpec, GroupStepsOp } from "@unfold/desktop/agent-api";
import { groupConflict, groupSelection } from "../../../code/outline";
import type { Card, Key, OutlineItem, Page } from "../../../ipc/generated";
import { addGroup, cardKey, isFreeId, updateGroup } from "../document";
import { AgentRefusal, cardName, cardOf, cardProject, guardPeopleText, invalid, type OpContext } from "./context";

/** `27–31`, or `27` for one line. */
function linesText(from: number, to: number): string {
  return from === to ? `line ${from}` : `lines ${from}–${to}`;
}

/** The outermost statements a range touches, as a refusal lists them so the agent can choose again. */
function statementsThere(outline: readonly OutlineItem[], from: number, to: number): string {
  const touches = (item: OutlineItem) => item.startLine <= to && item.endLine >= from;
  const outer = outline.filter((item) => touches(item) && (item.parent === undefined || !touches(outline[item.parent] ?? item)));
  if (outer.length === 0) {
    return "No statement starts there.";
  }
  return `The statements there: ${outer.map((item) => `${item.kind}${item.role !== undefined && item.role !== "body" ? ` (${item.role})` : ""}, ${linesText(item.startLine, item.endLine)}`).join("; ")}.`;
}

/** Whether two lists hold the same statements. */
function sameSteps(a: readonly Key[], b: readonly Key[]): boolean {
  return [...a].sort().join("\u0000") === [...b].sort().join("\u0000");
}

/** The statements a group spec takes, or a refusal that says why the range breaks the rule. */
function chooseStatements(outline: readonly OutlineItem[], spec: AgentStepGroupSpec, name: string): OutlineItem[] {
  const byKey = new Map(outline.map((item) => [item.key, item]));
  if ((spec.lines === undefined) === (spec.steps === undefined)) {
    throw invalid(`Give the step group “${spec.title}” either lines or steps.`);
  }
  let from: number;
  let to: number;
  if (spec.lines) {
    from = Math.min(spec.lines[0], spec.lines[1]);
    to = Math.max(spec.lines[0], spec.lines[1]);
  } else {
    const items = (spec.steps ?? []).map((key) => {
      const item = byKey.get(key);
      if (!item) {
        throw invalid(`“${key}” isn't a statement of ${name}. read_code lists its statements and their keys.`);
      }
      return item;
    });
    from = Math.min(...items.map((item) => item.startLine));
    to = Math.max(...items.map((item) => item.endLine));
  }
  const selection = groupSelection(outline, from, to);
  if ("error" in selection) {
    throw invalid(
      `Can't make the step group “${spec.title}” from ${linesText(from, to)} of ${name}: ${selection.error} A step group is consecutive statements in one branch. ${statementsThere(outline, from, to)}`,
    );
  }
  if (spec.steps && !sameSteps(selection.members, spec.steps)) {
    throw invalid(
      `The steps for “${spec.title}” aren't consecutive statements of one branch of ${name}: ${linesText(from, to)} hold ${selection.members.length} statements. ${statementsThere(outline, from, to)}`,
    );
  }
  return selection.members.flatMap((key) => byKey.get(key) ?? []);
}

/** A card's statement outline, or a refusal when its code isn't available. */
function outlineOf(ctx: OpContext, card: Card, name: string): readonly OutlineItem[] {
  const model = ctx.data.model(cardProject(ctx, card), cardKey(card));
  const code = model?.type === "block" && model.code?.source.state === "current" ? model.code.source : null;
  if (!code) {
    throw new AgentRefusal(
      "code_unavailable",
      `${name}'s code isn't available right now (its file changed since the last compile, or its project isn't connected). Compile the project, then try again.`,
    );
  }
  return code.outline;
}

function statement(item: OutlineItem): AgentStatement {
  return { key: item.key, kind: item.kind, lines: [item.startLine, item.endLine] };
}

export function applyGroupSteps(page: Page, op: GroupStepsOp, ctx: OpContext): AgentItemResult[] {
  const card = cardOf(page, op.card);
  const name = cardName(ctx, card);
  if (!("block" in card.ref)) {
    throw invalid(`${name} is an entrance card; step groups go in a function's card.`);
  }
  const outline = outlineOf(ctx, card, name);
  const author = ctx.agent.author;
  return op.groups.map((spec): AgentItemResult => {
    const title = spec.title.trim();
    if (title.length === 0) {
      throw invalid("A step group needs a title of a few words.");
    }
    const items = chooseStatements(outline, spec, name);
    const members: Key[] = items.map((item) => item.key);
    // By id; else the card's group of the same statements, so the same call again changes nothing.
    const existing =
      spec.id !== undefined ? page.groups.find((group) => group.id === spec.id) : page.groups.find((group) => group.card === card.id && sameSteps(group.members, members));
    if (existing && existing.card !== card.id) {
      throw invalid(`“${existing.id}” is a step group of another card.`);
    }
    if (!existing && spec.id !== undefined && !isFreeId(page, spec.id)) {
      throw invalid(`“${spec.id}” can't name a new step group: another item has it, or it isn't letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`);
    }
    const others = page.groups.filter((group) => group.card === card.id && group.id !== existing?.id);
    const conflict = groupConflict(outline, members, others);
    if (conflict !== null) {
      throw invalid(`Can't make the step group “${title}” in ${name}: ${conflict}`);
    }
    const description = spec.description?.trim();
    const describe = description !== undefined && description.length > 0 ? { description: { text: description, author } } : {};
    let id: string;
    let status: AgentItemResult["status"];
    if (existing) {
      if (existing.title !== title) {
        guardPeopleText(`the title of the step group “${existing.title}”`, existing.author, op.replacePeopleText);
      }
      if (existing.description && description !== undefined && existing.description.text !== description) {
        guardPeopleText(`the description of the step group “${existing.title}”`, existing.description.author, op.replacePeopleText);
      }
      const before = JSON.stringify(existing);
      updateGroup(page, existing.id, { title, author, members, ...describe, ...(spec.collapsed !== undefined ? { collapsed: spec.collapsed } : {}) });
      id = existing.id;
      status = JSON.stringify(existing) === before ? "kept" : "updated";
    } else {
      id = addGroup(page, card.id, members, title, { author, ...(spec.id !== undefined ? { id: spec.id } : {}), ...(spec.collapsed !== undefined ? { collapsed: spec.collapsed } : {}) });
      updateGroup(page, id, describe);
      status = "created";
    }
    const from = Math.min(...items.map((item) => item.startLine));
    const to = Math.max(...items.map((item) => item.endLine));
    if (spec.lines && (Math.min(...spec.lines) !== from || Math.max(...spec.lines) !== to)) {
      ctx.warnings.push(`“${title}” took whole statements: ${linesText(from, to)} of ${name}, not ${linesText(Math.min(...spec.lines), Math.max(...spec.lines))}.`);
    }
    return { kind: "group", id, status, card: card.id, name: title, lines: [from, to], statements: items.map(statement) };
  });
}
