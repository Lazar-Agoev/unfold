/**
 * `connect`: draws connections between cards, resource cards, and frames of a page, or
 * updates them by id: a kind, the step the line leaves from, the columns a relationship joins and its
 * cardinality, the channel and the type that travel, a label, a description, and anchors. The window
 * checks the connection's shape; the save checks what it cites, fills in the channel when `to` is an
 * entrance, and grounds an agent's connection, refusing with what to do instead. What a person drew
 * and wrote is theirs unless the call says the person asked.
 */

import type { AgentConnectionSpec, AgentItemResult, AgentItemStatus, AgentLinkResult, ConnectOp } from "@unfold/desktop/agent-api";
import { projectCalls } from "@unfold/desktop/drawing";
import { tableColumns } from "../../../canvas/resources/columns";
import type { Connection, Page } from "../../../ipc/generated";
import { findBlock } from "../../cards";
import { cardKey, findCard, findCombined, isFreeId, isItemId } from "../document";
import { findFrame } from "../frames";
import { findResource } from "../resources";
import { addConnection, saysConnection, setConnectionClaim, setLinkDescription, setLinkLabel, type ConnectionClaim } from "../connections";
import { cardName, guardPeopleText, invalid, lineWords, resourceName, type OpContext } from "./context";
import { fileAnchor } from "./resources";

/** A connection's result entry: its ends, its label, its kind, step, channel, and carried type. */
export function linkResult(link: Connection, status: AgentItemStatus): AgentLinkResult {
  return {
    kind: "link",
    id: link.id,
    status,
    from: link.from,
    to: link.to,
    ...(link.label ? { name: link.label.text } : {}),
    connection: link.kind,
    ...(link.step !== undefined ? { step: link.step } : {}),
    ...(link.channel !== undefined ? { channel: link.channel } : {}),
    ...(link.carries !== undefined ? { carries: link.carries } : {}),
  };
}

/** An end's name for a refusal: a card's, a resource card's, or a frame's. */
function endName(ctx: OpContext, page: Page, id: string): string {
  const card = findCard(page, id);
  if (card) {
    return cardName(ctx, card);
  }
  const resource = findResource(page, id);
  return resource ? resourceName(resource) : `the frame “${findFrame(page, id)?.title ?? id}”`;
}

/** Refuses an end that isn't a card, a resource card, or a frame of the page. */
function checkEnd(page: Page, id: string, side: "from" | "to"): void {
  if (findCard(page, id) || findResource(page, id) || findFrame(page, id)) {
    return;
  }
  const other = findCombined(page, id) ? "a combined step" : page.links.some((item) => item.id === id) ? "a connection" : (page.notes ?? []).some((item) => item.id === id) ? "a note" : null;
  throw invalid(
    other !== null
      ? `“${id}” (${side}) is ${other}: a connection joins cards, resource cards, and frames.`
      : `There is no card, resource card, or frame with id “${id}” (${side}) on this page. read_page lists the page's ids; add_cards and add_resources put them there.`,
  );
}

/** The columns a table end has, when they are known (its own, or its type's once loaded); null when they aren't. */
function knownColumns(ctx: OpContext, page: Page, id: string): readonly string[] | null {
  const resource = findResource(page, id);
  if (!resource) {
    return null;
  }
  const definition = resource.type !== undefined ? ctx.data.definition(ctx.board.project, resource.type) : undefined;
  return resource.type !== undefined && !definition ? null : tableColumns(resource, definition).map((column) => column.name);
}

/** Refuses a relationship that doesn't join two tables, or names a column its table doesn't have. */
function checkRelates(ctx: OpContext, page: Page, spec: AgentConnectionSpec): void {
  for (const [id, column] of [
    [spec.from, spec.fromColumn],
    [spec.to, spec.toColumn],
  ] as const) {
    if (findResource(page, id)?.kind !== "table") {
      throw invalid(`A relates connection joins two tables, and ${endName(ctx, page, id)} isn't one. Use writes or reads between code and a table.`);
    }
    const columns = knownColumns(ctx, page, id);
    if (column !== undefined && columns !== null && !columns.includes(column)) {
      throw invalid(`“${column}” isn't a column of ${endName(ctx, page, id)}. Its columns: ${columns.join(", ") || "none listed"}.`);
    }
  }
}

/**
 * Refuses a connection that repeats a compiled call: a project call the from card draws (its step, or
 * any call without one; held by a collapsed combined step or opened in place too) to the to card's
 * definition, which Unfold already draws as an arrow. A `calls` connection is the save's to check: it
 * must start at an unknown call.
 */
function checkNotCompiled(ctx: OpContext, page: Page, spec: AgentConnectionSpec, what: string): void {
  const from = findCard(page, spec.from);
  const to = findCard(page, spec.to);
  const model = from ? ctx.data.model(ctx.board.project, cardKey(from)) : undefined;
  if (!from || !to || spec.kind === "calls" || model?.type !== "definition") {
    return;
  }
  const target = cardKey(to);
  const call = projectCalls(model.flow, true).find((item) => item.to === target && (spec.step === undefined || item.call === spec.step));
  if (call) {
    throw invalid(
      `${what} repeats a compiled call: ${cardName(ctx, from)} calls ${cardName(ctx, to)} (${call.callee}), and Unfold already draws that arrow. Connections are for what the graph can't show: HTTP, queues, data, and where an unknown call goes. get_relations says what Unfold draws.`,
    );
  }
}

/** What a spec says, checked against the page: its ends, step, columns, channel, carried type, and anchors. */
function claimOf(ctx: OpContext, page: Page, spec: AgentConnectionSpec): ConnectionClaim {
  checkEnd(page, spec.from, "from");
  checkEnd(page, spec.to, "to");
  if (spec.from === spec.to) {
    throw invalid(`A connection joins two different items; “${spec.from}” is both ends.`);
  }
  const what = `the ${spec.kind} connection from ${endName(ctx, page, spec.from)} to ${endName(ctx, page, spec.to)}`;
  if (spec.step !== undefined) {
    const card = findCard(page, spec.from);
    if (!card || !("block" in card.ref)) {
      throw invalid(`${what} has a step, but its from end isn't a definition's card: a step is a call inside the from card's block.`);
    }
    // The call must be one the from card draws; a card not loaded is checked by the save.
    const model = ctx.data.model(ctx.board.project, cardKey(card));
    const call = model?.type === "definition" ? findBlock(model.flow, spec.step) : undefined;
    if (model?.type === "definition" && (call === undefined || (call.form !== "call" && call.form !== "new"))) {
      throw invalid(`The step “${spec.step}” isn't a call of ${cardName(ctx, card)}: a step is a call of the from card's block (read_code lists them with their keys).`);
    }
  }
  checkNotCompiled(ctx, page, spec, what);
  if (spec.kind === "relates") {
    checkRelates(ctx, page, spec);
  } else if (spec.fromColumn !== undefined || spec.toColumn !== undefined || spec.cardinality !== undefined) {
    throw invalid(`Columns and cardinality are for a relates connection between two tables; ${what} is ${spec.kind}. Leave them out.`);
  }
  const channel = spec.channel?.trim();
  if (channel?.length === 0) {
    throw invalid(`The channel of ${what} is blank: name what travels (the route, the queue, the event), or leave channel out.`);
  }
  return {
    kind: spec.kind,
    from: spec.from,
    to: spec.to,
    anchors: (spec.anchors ?? []).map((anchor) => fileAnchor(anchor, what)),
    ...(spec.step !== undefined ? { step: spec.step } : {}),
    ...(spec.fromColumn !== undefined ? { fromColumn: spec.fromColumn } : {}),
    ...(spec.toColumn !== undefined ? { toColumn: spec.toColumn } : {}),
    ...(spec.cardinality !== undefined ? { cardinality: spec.cardinality } : {}),
    ...(channel !== undefined ? { channel } : {}),
    ...(spec.carries !== undefined ? { carries: spec.carries } : {}),
  };
}

/** The connection a spec updates: by id, else the one of the same kind, ends, step, and columns; undefined for a new one. */
function existingFor(page: Page, spec: AgentConnectionSpec): Connection | undefined {
  if (spec.id === undefined) {
    return page.links.find(
      (link) =>
        link.kind === spec.kind &&
        link.from === spec.from &&
        link.to === spec.to &&
        link.step === spec.step &&
        link.fromColumn === spec.fromColumn &&
        link.toColumn === spec.toColumn,
    );
  }
  const link = page.links.find((item) => item.id === spec.id);
  if (!link && !isFreeId(page, spec.id)) {
    throw invalid(
      isItemId(spec.id)
        ? `“${spec.id}” is the id of another item on this page (not a connection). Give the connection another id, or leave id out.`
        : `“${spec.id}” can't name a connection: an id is letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`,
    );
  }
  return link;
}

/** Draws or updates one connection; returns its result. */
function applyConnectionSpec(ctx: OpContext, page: Page, spec: AgentConnectionSpec, replacePeopleText: boolean | undefined): AgentLinkResult {
  const claim = claimOf(ctx, page, spec);
  const existing = existingFor(page, spec);
  const label = spec.label?.trim();
  const text = spec.description?.trim();
  if (spec.kind === "other" && (label === undefined || label.length === 0) && !existing?.label) {
    throw invalid("An other connection needs a label: what travels along it, in a few words (“place-order job”). Or choose the kind that fits.");
  }
  const author = ctx.agent.author;
  const before = existing ? JSON.stringify(existing) : null;
  if (existing && !saysConnection(existing, claim)) {
    guardPeopleText(...lineWords(existing), replacePeopleText);
  }
  if (existing?.label && label !== undefined && existing.label.text !== label) {
    guardPeopleText(`the label “${existing.label.text}”`, existing.label.author, replacePeopleText);
  }
  if (existing?.description && text !== undefined && existing.description.text !== text) {
    guardPeopleText(`the description of ${lineWords(existing)[0]}`, existing.description.author, replacePeopleText);
  }
  let id: string;
  if (existing) {
    setConnectionClaim(page, existing.id, claim, author);
    id = existing.id;
  } else {
    id = addConnection(page, { ...claim, author, ...(spec.id !== undefined ? { id: spec.id } : {}) });
  }
  if (label !== undefined && label.length > 0) {
    setLinkLabel(page, id, label, author);
  }
  if (text !== undefined && text.length > 0) {
    setLinkDescription(page, id, text, author);
  }
  const link = page.links.find((item) => item.id === id);
  if (!link) {
    throw invalid(`The ${spec.kind} connection couldn't be drawn.`);
  }
  return linkResult(link, before === null ? "created" : JSON.stringify(link) === before ? "kept" : "updated");
}

export function applyConnect(page: Page, op: ConnectOp, ctx: OpContext): AgentItemResult[] {
  return op.connections.map((spec): AgentItemResult => applyConnectionSpec(ctx, page, spec, op.replacePeopleText));
}
