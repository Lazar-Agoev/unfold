/**
 * Pure edits of a page's connections: how two things talk when the graph can't show it (HTTP, a queue,
 * reads and writes, a runtime call, a relationship between tables, or something else, which carries a
 * label). A connection is an authored claim: the next save signs it over its step and anchors. Each
 * function is an Immer recipe over a draft page; optional fields are deleted rather than set to
 * `undefined`.
 */

import type { Anchor, Cardinality, Connection, ConnectionKind, Key, Page } from "../../ipc/generated";
import { description, uniqueId } from "./document";

/** Every connection kind, in the order menus list them. */
export const CONNECTION_KINDS: readonly ConnectionKind[] = ["http", "queue", "reads", "writes", "calls", "relates", "other"];

/** Each kind in a word, as menus and Selected name it. */
export const CONNECTION_WORDS: Readonly<Record<ConnectionKind, string>> = {
  http: "HTTP",
  queue: "Queue",
  reads: "Reads",
  writes: "Writes",
  calls: "Calls",
  relates: "Relates",
  other: "Other",
};

/** What a connection says: its kind, its ends, where it leaves from, what travels, and what it cites. */
export interface ConnectionClaim {
  readonly kind: ConnectionKind;
  readonly from: string;
  readonly to: string;
  readonly step?: Key;
  readonly fromColumn?: string;
  readonly toColumn?: string;
  readonly cardinality?: Cardinality;
  readonly channel?: string;
  readonly carries?: Key;
  readonly anchors: readonly Anchor[];
}

/** Writes a claim onto connection `link`, leaving out what it doesn't have (columns and cardinality only on `relates`). */
function writeClaim(link: Connection, claim: ConnectionClaim): void {
  const { kind, from, to, step, fromColumn, toColumn, cardinality, channel, carries, anchors } = claim;
  const relates = kind === "relates";
  delete link.step;
  delete link.fromColumn;
  delete link.toColumn;
  delete link.cardinality;
  delete link.channel;
  delete link.carries;
  delete link.anchors;
  Object.assign(link, {
    kind,
    from,
    to,
    ...(step !== undefined ? { step } : {}),
    ...(relates && fromColumn !== undefined ? { fromColumn } : {}),
    ...(relates && toColumn !== undefined ? { toColumn } : {}),
    ...(relates && cardinality !== undefined ? { cardinality } : {}),
    ...(channel !== undefined ? { channel } : {}),
    ...(carries !== undefined ? { carries } : {}),
    ...(anchors.length > 0 ? { anchors: anchors.map((anchor) => ({ ...anchor })) } : {}),
  });
}

/** What a connection says, in the file's order of meaning: what `saysConnection` compares. */
function said(link: Connection): string {
  return JSON.stringify([link.kind, link.from, link.to, link.step, link.fromColumn, link.toColumn, link.cardinality, link.channel, link.carries, link.anchors ?? []]);
}

/** Whether a connection already says `claim`. */
export function saysConnection(link: Connection, claim: ConnectionClaim): boolean {
  const next: Connection = { ...link };
  writeClaim(next, claim);
  return said(next) === said(link);
}

/** Draws a connection and returns its id: `id` when given (a free one, `isFreeId`), else one from its ends and kind. */
export function addConnection(page: Page, claim: ConnectionClaim & { readonly id?: string; readonly author: string }): string {
  const id = claim.id ?? uniqueId(page, `${claim.from}-${claim.kind}-${claim.to}`);
  const link: Connection = { id, from: claim.from, to: claim.to, kind: claim.kind, author: claim.author };
  writeClaim(link, claim);
  page.links.push(link);
  return id;
}

/** Rewrites what a connection says, which makes `author` the one who drew it; nothing changes when it already says it. */
export function setConnectionClaim(page: Page, linkId: string, claim: ConnectionClaim, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (link && !saysConnection(link, claim)) {
    writeClaim(link, claim);
    link.author = author;
  }
}

/**
 * Gives a connection another kind, which makes `author` the one who drew it. Its ends, step, channel,
 * carried type, and anchors stay; columns and cardinality only on `relates`.
 */
export function setConnectionKind(page: Page, linkId: string, kind: ConnectionKind, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (!link || link.kind === kind) {
    return;
  }
  setConnectionClaim(page, linkId, { ...claimOf(link), kind }, author);
}

/** What a connection says, as a claim to edit (its kind is the caller's to set). */
export function claimOf(link: Connection): Omit<ConnectionClaim, "kind"> {
  const { id: _id, label: _label, description: _description, author: _author, signature: _signature, kind: _kind, anchors = [], ...said } = link;
  return { ...said, anchors };
}

/** Sets or clears (blank text) a connection's description; the next save signs it. */
export function setLinkDescription(page: Page, linkId: string, text: string, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (!link) {
    return;
  }
  if (text.trim().length === 0) {
    delete link.description;
  } else if (link.description?.text !== text) {
    link.description = description(text, author);
  }
}

/**
 * Sets or clears (blank text) a connection's label. An `other` connection says what it is in its label,
 * so blank text keeps the label it has.
 */
export function setLinkLabel(page: Page, linkId: string, text: string, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (!link) {
    return;
  }
  if (text.trim().length === 0) {
    if (link.kind !== "other") {
      delete link.label;
    }
  } else if (link.label?.text !== text) {
    link.label = description(text, author);
  }
}
