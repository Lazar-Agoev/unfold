/**
 * Pure edits of a page's resource cards (design §4.5–4.6): what the code talks to that isn't a
 * function (a table, a queue, a cache, storage, a service outside the workspace, or who starts
 * things), each with its name, its anchors, and, for a table, the type that declares its columns or
 * its own columns. Resource cards are authored claims: the next save signs them over what they cite.
 * Each function is an Immer recipe over a draft page; optional fields are deleted rather than set to
 * `undefined`, and an empty list is deleted, as the file omits them.
 */

import type { Anchor, Column, Key, Page, Resource, ResourceKind } from "../../ipc/generated";
import { description, uniqueId } from "./document";

/** A page's resource cards, as the queries read them. */
interface Resourced {
  readonly resources?: readonly Resource[] | undefined;
}

export function findResource(page: Resourced, resourceId: string): Resource | undefined {
  return (page.resources ?? []).find((resource) => resource.id === resourceId);
}

/** What a resource card says: its kind, name, what it cites, and (a table) its type or columns. */
export interface ResourceClaim {
  readonly kind: ResourceKind;
  readonly name: string;
  /** A table's type key. */
  readonly type?: Key;
  readonly columns?: readonly Column[];
  readonly anchors: readonly Anchor[];
}

/** What a new resource card is: its claim, where it goes, and who drew it. */
export interface NewResource extends ResourceClaim {
  /** A free id (`isFreeId`); absent: one from the name. */
  readonly id?: string;
  readonly at: readonly [number, number];
  readonly author: string;
}

/** Writes a claim onto `resource`, leaving out what it doesn't have (only a table has a type or columns). */
function writeClaim(resource: Resource, claim: ResourceClaim): void {
  resource.kind = claim.kind;
  resource.name = claim.name;
  resource.anchors = claim.anchors.map((anchor) => ({ ...anchor }));
  delete resource.type;
  delete resource.columns;
  if (claim.kind !== "table") {
    return;
  }
  if (claim.type !== undefined) {
    resource.type = claim.type;
  }
  if (claim.columns !== undefined && claim.columns.length > 0) {
    resource.columns = claim.columns.map((column) => ({ ...column }));
  }
}

/** Adds a resource card and returns its id. Positions are rounded: the file holds finite numbers. */
export function addResource(page: Page, spec: NewResource): string {
  const id = spec.id ?? uniqueId(page, spec.name);
  const resource: Resource = { id, kind: spec.kind, name: spec.name, anchors: [], at: [Math.round(spec.at[0]), Math.round(spec.at[1])], author: spec.author };
  writeClaim(resource, spec);
  page.resources ??= [];
  page.resources.push(resource);
  return id;
}

/** Whether a resource card already says `claim` (its kind, name, type, columns, and anchors). */
export function saysClaim(resource: Resource, claim: ResourceClaim): boolean {
  const next: Resource = { ...resource, anchors: [] };
  writeClaim(next, claim);
  const said = (item: Resource) => JSON.stringify([item.kind, item.name, item.type, item.columns ?? [], item.anchors]);
  return said(next) === said(resource);
}

/** Rewrites what a resource card says, which makes `author` the one who drew it; nothing changes when it already says it. */
export function setResourceClaim(page: Page, resourceId: string, claim: ResourceClaim, author: string): void {
  const resource = findResource(page, resourceId);
  if (resource && !saysClaim(resource, claim)) {
    writeClaim(resource, claim);
    resource.author = author;
  }
}

/** Sets or clears (blank text) a resource card's description; the next save signs it. */
export function setResourceDescription(page: Page, resourceId: string, text: string, author: string): void {
  const resource = findResource(page, resourceId);
  if (!resource) {
    return;
  }
  if (text.trim().length === 0) {
    delete resource.description;
  } else if (resource.description?.text !== text) {
    resource.description = description(text, author);
  }
}
