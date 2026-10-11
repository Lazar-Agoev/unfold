/**
 * Pure edits of a page's connections (design §4.7): links with a `kind` that say how two things talk
 * when the graph can't show it (HTTP, RPC, a queue, an event, reads and writes, a runtime call, a
 * relationship between tables, or something else). A link without a kind is today's drawn link. A
 * connection is an authored claim: the next save signs it over its step and anchors. Each function is
 * an Immer recipe over a draft page; optional fields are deleted rather than set to `undefined`.
 */

import type { Anchor, Cardinality, Key, Link, LinkKind, Page, ProjectName } from "../../ipc/generated";
import { description, uniqueId } from "./document";

/** Every connection kind, in the order menus list them. */
export const CONNECTION_KINDS: readonly LinkKind[] = ["http", "rpc", "queue", "event", "writes", "reads", "calls", "relates", "other"];

/** What a connection says: its kind, its ends, where it leaves from, what travels, and what it cites. */
export interface ConnectionClaim {
  readonly kind: LinkKind;
  readonly from: string;
  readonly to: string;
  readonly step?: Key;
  readonly fromColumn?: string;
  readonly toColumn?: string;
  readonly cardinality?: Cardinality;
  readonly channel?: string;
  readonly carries?: Key;
  /** The carried type's project (absent: the board's own). */
  readonly carriesProject?: ProjectName;
  readonly anchors: readonly Anchor[];
}

/** Writes a claim onto `link`, leaving out what it doesn't have (columns and cardinality only on `relates`). */
function writeClaim(link: Link, claim: ConnectionClaim): void {
  const { kind, from, to, step, fromColumn, toColumn, cardinality, channel, carries, carriesProject, anchors } = claim;
  const relates = kind === "relates";
  delete link.step;
  delete link.fromColumn;
  delete link.toColumn;
  delete link.cardinality;
  delete link.channel;
  delete link.carries;
  delete link.carriesProject;
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
    ...(carries !== undefined ? { carries, ...(carriesProject !== undefined ? { carriesProject } : {}) } : {}),
    ...(anchors.length > 0 ? { anchors: anchors.map((anchor) => ({ ...anchor })) } : {}),
  });
}

/** What a link says, in the file's order of meaning: what `saysConnection` compares. */
function said(link: Link): string {
  return JSON.stringify([link.kind, link.from, link.to, link.step, link.fromColumn, link.toColumn, link.cardinality, link.channel, link.carries, link.carriesProject, link.anchors ?? []]);
}

/** Whether a link already says `claim`. */
export function saysConnection(link: Link, claim: ConnectionClaim): boolean {
  const next: Link = { ...link };
  writeClaim(next, claim);
  return said(next) === said(link);
}

/** Draws a connection and returns its id: `id` when given (a free one, `isFreeId`), else one from its ends and kind. */
export function addConnection(page: Page, claim: ConnectionClaim & { readonly id?: string; readonly author: string }): string {
  const id = claim.id ?? uniqueId(page, `${claim.from}-${claim.kind}-${claim.to}`);
  const link: Link = { id, from: claim.from, to: claim.to, author: claim.author };
  writeClaim(link, claim);
  page.links.push(link);
  return id;
}

/** Rewrites what a link says, which makes `author` the one who drew it; nothing changes when it already says it. */
export function setConnectionClaim(page: Page, linkId: string, claim: ConnectionClaim, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (link && !saysConnection(link, claim)) {
    writeClaim(link, claim);
    link.author = author;
  }
}

/**
 * Gives a link a kind (a drawn link becomes a connection), which makes `author` the one who drew it.
 * Its ends, step, channel, carried type, and anchors stay; columns and cardinality only on `relates`.
 */
export function setConnectionKind(page: Page, linkId: string, kind: LinkKind, author: string): void {
  const link = page.links.find((item) => item.id === linkId);
  if (!link || link.kind === kind) {
    return;
  }
  setConnectionClaim(page, linkId, { ...claimOf(link), kind }, author);
}

/** What a link says, as a claim to edit (its kind, when it has one, is the caller's to set). */
export function claimOf(link: Link): Omit<ConnectionClaim, "kind"> {
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
