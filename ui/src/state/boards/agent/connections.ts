/**
 * `connect`: draws connections between cards, resource cards, and frames of a page, or
 * updates them by id: a kind, the step the line leaves from, the columns a relationship joins and its
 * cardinality, the channel and the type that travel, a label, a description, and anchors. The save
 * checks the connection against the board's rules (its ends, kind, step, columns, and label) and what it
 * cites, fills in the channel when `to` is an entrance, and grounds an agent's connection, refusing with
 * what to do instead. The window checks only what it draws: that a step is drawn on the from card, and
 * that the connection doesn't repeat a compiled call. What a person drew and wrote is theirs unless the
 * call says the person asked.
 */

import type { AgentConnectionSpec, AgentItemResult, AgentItemStatus, AgentLinkResult, ConnectOp } from "@unfold/desktop/agent-api";
import { projectCalls } from "@unfold/desktop/drawing";
import type { Connection, Page } from "../../../ipc/generated";
import { findBlock } from "../../cards";
import { cardKey, findCard } from "../document";
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

/** An end's name for a refusal: a card's, a resource card's, a frame's, or else its id. */
function endName(ctx: OpContext, page: Page, id: string): string {
  const card = findCard(page, id);
  if (card) {
    return cardName(ctx, card);
  }
  const resource = findResource(page, id);
  if (resource) {
    return resourceName(resource);
  }
  const frame = findFrame(page, id);
  return frame ? `the frame “${frame.title}”` : `“${id}”`;
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

/** What a spec says, with its step checked against what the from card draws and the connection against the compiled calls. */
function claimOf(ctx: OpContext, page: Page, spec: AgentConnectionSpec): ConnectionClaim {
  const what = `the ${spec.kind} connection from ${endName(ctx, page, spec.from)} to ${endName(ctx, page, spec.to)}`;
  const card = findCard(page, spec.from);
  const model = card ? ctx.data.model(ctx.board.project, cardKey(card)) : undefined;
  // The step must be drawn on the from card; that it is a call is the save's to check, as is a card not loaded.
  if (card && spec.step !== undefined && model?.type === "definition" && findBlock(model.flow, spec.step) === undefined) {
    throw invalid(`The step “${spec.step}” isn't drawn on ${cardName(ctx, card)}: a step is a call of the from card's block (read_code lists them with their keys).`);
  }
  checkNotCompiled(ctx, page, spec, what);
  const channel = spec.channel?.trim();
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
  if (spec.id !== undefined) {
    return page.links.find((item) => item.id === spec.id);
  }
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

/** Draws or updates one connection; returns its result. */
function applyConnectionSpec(ctx: OpContext, page: Page, spec: AgentConnectionSpec, replacePeopleText: boolean | undefined): AgentLinkResult {
  const claim = claimOf(ctx, page, spec);
  const existing = existingFor(page, spec);
  const label = spec.label?.trim();
  const text = spec.description?.trim();
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
