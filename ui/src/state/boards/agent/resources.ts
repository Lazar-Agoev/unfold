/**
 * `add_resources` (design §6.7): adds resource cards to a page, or updates them by id: a table (backed
 * by a type, whose fields are its columns, or listing its own), a queue, a cache, storage, a service
 * outside the workspace, or an actor, each with what it cites. The window checks the card's shape and
 * places it; the save checks what it cites and grounds an agent's card (its name, and the columns a
 * table lists itself, must appear in what it cites), refusing with what to do instead. With `frame`, a
 * card goes into that frame from no frame or the frame around it, as `frame_cards` takes members. What
 * a person drew and wrote is theirs unless the call says the person asked.
 */

import type { AddResourcesOp, AgentAnchor, AgentItemResult, AgentItemStatus, AgentResourceResult, AgentResourceSpec } from "@unfold/desktop/agent-api";
import { RESOURCE_WORDS } from "../../../canvas/resources/kinds";
import { columnTypeText, type TableColumn } from "../../../canvas/resources/columns";
import type { Anchor, Column, Page, Resource } from "../../../ipc/generated";
import { estimatedResourceSize } from "../../../canvas/frames/geometry";
import { isFreeId, isItemId, moveItems } from "../document";
import { addToFrame, findFrame, frameOf } from "../frames";
import { addResource, findResource, saysClaim, setResourceClaim, setResourceDescription, type ResourceClaim } from "../resources";
import { guardPeopleText, invalid, itemSize, resourceColumns, resourceName, type ApplyOp, type OpContext } from "./context";
import { resourceSpot } from "./place";

/** A table's columns as a result lists them. */
function columnResults(columns: readonly TableColumn[]): NonNullable<AgentResourceResult["columns"]> {
  return columns.map((column) => {
    const type = columnTypeText(column);
    return {
      name: column.name,
      ...(type.length > 0 ? { type } : {}),
      ...(column.key !== undefined ? { key: column.key } : {}),
      ...(column.references !== undefined ? { references: column.references } : {}),
      ...(column.optional ? { optional: true } : {}),
    };
  });
}

/** A resource card's result entry. */
export function resourceResult(ctx: OpContext, page: Page, resource: Resource, status: AgentItemStatus, extra: Partial<AgentResourceResult> = {}): AgentResourceResult {
  const frame = frameOf(page, resource.id);
  return {
    kind: "resource",
    id: resource.id,
    status,
    resource: resource.kind,
    name: resource.name,
    ...(resource.type !== undefined ? { type: resource.type } : {}),
    ...(resource.kind === "table" ? { columns: columnResults(resourceColumns(ctx, resource)) } : {}),
    ...(frame ? { frame: frame.id } : {}),
    ...extra,
  };
}

/** An anchor as the board file writes it, lines checked. */
export function fileAnchor(anchor: AgentAnchor, what: string): Anchor {
  if ("key" in anchor) {
    return { key: anchor.key };
  }
  const [first, last] = anchor.lines;
  if (!Number.isInteger(first) || !Number.isInteger(last) || first < 1 || last < first) {
    throw invalid(`An anchor of ${what} cites lines ${first}–${last} of ${anchor.file}: lines are whole numbers from 1, the last not before the first.`);
  }
  return { file: anchor.file, lines: [first, last] };
}

/** The columns a table writes, checked: distinct names, marks only on a table backed by a type, `references` only on a foreign key. */
function fileColumns(spec: AgentResourceSpec, what: string): Column[] {
  const columns = spec.columns ?? [];
  const seen = new Set<string>();
  return columns.map((column) => {
    const name = column.name.trim();
    if (name.length === 0 || seen.has(name)) {
      throw invalid(`${what} lists ${name.length === 0 ? "a column without a name" : `the column “${name}” twice`}: each column has its own name.`);
    }
    seen.add(name);
    if (spec.type !== undefined && (column.type !== undefined || column.optional !== undefined)) {
      throw invalid(`${what} is backed by a type, so its columns, their types, and whether they are optional come from the type's fields: mark them with name, key, and references only.`);
    }
    if (column.references !== undefined && column.key !== "foreign") {
      throw invalid(`The column “${name}” of ${what} has references but isn't a foreign key: give it key "foreign", or leave references out.`);
    }
    return {
      name,
      ...(column.type !== undefined ? { type: column.type } : {}),
      ...(column.key !== undefined ? { key: column.key } : {}),
      ...(column.references !== undefined ? { references: column.references } : {}),
      ...(column.optional === true ? { optional: true } : {}),
    };
  });
}

/** What a spec says, over what an existing card says: `type` and `columns` go together, absent anchors are kept. */
function claimOf(spec: AgentResourceSpec, name: string, existing: Resource | undefined): ResourceClaim {
  const what = resourceName({ kind: spec.kind, name });
  if (spec.kind !== "table" && (spec.type !== undefined || spec.columns !== undefined)) {
    throw invalid(`Only a table has a type or columns; ${what} is a ${RESOURCE_WORDS[spec.kind]}. Leave type and columns out.`);
  }
  const shaped = spec.type !== undefined || spec.columns !== undefined;
  const anchors = spec.anchors ? spec.anchors.map((anchor) => fileAnchor(anchor, what)) : (existing?.anchors ?? []);
  const kept = !shaped && existing?.kind === "table" && spec.kind === "table" ? existing : undefined;
  const type = shaped ? spec.type : kept?.type;
  const columns = shaped ? fileColumns(spec, what) : kept?.columns;
  return {
    kind: spec.kind,
    name,
    anchors,
    ...(type !== undefined ? { type } : {}),
    ...(columns !== undefined ? { columns } : {}),
  };
}

/** The card a spec updates: by id, else the card of the same kind and name; undefined for a new one. */
function existingFor(page: Page, spec: AgentResourceSpec, name: string): Resource | undefined {
  if (spec.id === undefined) {
    return (page.resources ?? []).find((resource) => resource.kind === spec.kind && resource.name === name);
  }
  const resource = findResource(page, spec.id);
  if (!resource && !isFreeId(page, spec.id)) {
    throw invalid(
      isItemId(spec.id)
        ? `“${spec.id}” is the id of another item on this page (not a resource card). Give the resource card another id, or leave id out.`
        : `“${spec.id}” can't name a resource card: an id is letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`,
    );
  }
  return resource;
}

/**
 * Refuses to put a resource card in frame `frameId` from another frame: an item is in one frame, so it
 * comes from no frame, the frame itself, or the frame that holds it (as `frame_cards` takes members).
 */
function checkFrameMove(page: Page, resource: Resource, frameId: string): void {
  const now = frameOf(page, resource.id);
  const target = findFrame(page, frameId);
  if (now && target && now.id !== target.id && now.id !== frameOf(page, target.id)?.id) {
    throw invalid(
      `Can't put ${resourceName(resource)} (id ${resource.id}) in the frame “${target.title}”: it is in the frame “${now.title}” (id ${now.id}), and an item is in one frame. Leave frame out to keep it there, or take it out of “${now.title}” first (frame_cards with its members without it).`,
    );
  }
}

/** Creates or updates one resource card; returns its result. */
function applyResourceSpec(ctx: OpContext, page: Page, spec: AgentResourceSpec, replacePeopleText: boolean | undefined): AgentResourceResult {
  const name = spec.name.trim();
  if (name.length === 0) {
    throw invalid(`A ${RESOURCE_WORDS[spec.kind]} card needs a name: the technical name the code uses, such as “orders”.`);
  }
  if (spec.frame !== undefined && !findFrame(page, spec.frame)) {
    throw invalid(`There is no frame with id “${spec.frame}” on this page to put ${resourceName({ kind: spec.kind, name })} in. read_page lists the page's frames; frame_cards makes one.`);
  }
  const existing = existingFor(page, spec, name);
  if (existing && spec.frame !== undefined) {
    checkFrameMove(page, existing, spec.frame);
  }
  const claim = claimOf(spec, name, existing);
  const author = ctx.agent.author;
  const text = spec.description?.trim();
  const before = existing ? JSON.stringify([existing, frameOf(page, existing.id)?.id]) : null;
  if (existing && !saysClaim(existing, claim)) {
    guardPeopleText(resourceName(existing), existing.author, replacePeopleText);
  }
  if (existing?.description && text !== undefined && existing.description.text !== text) {
    guardPeopleText(`the description of ${resourceName(existing)}`, existing.description.author, replacePeopleText);
  }
  let at: [number, number] | undefined;
  let id: string;
  if (existing) {
    setResourceClaim(page, existing.id, claim, author);
    id = existing.id;
    if (spec.near !== undefined) {
      at = resourceSpot(ctx, page, itemSize(ctx, page, id), spec.near, spec.frame);
      moveItems(page, { [id]: at });
    }
  } else {
    const { columns, ...said } = claim;
    const draft: Resource = { ...said, anchors: [...claim.anchors], ...(columns ? { columns: [...columns] } : {}), id: "", at: [0, 0], author };
    at = resourceSpot(ctx, page, estimatedResourceSize(draft, resourceColumns(ctx, draft)), spec.near, spec.frame);
    id = addResource(page, { ...claim, at, author, ...(spec.id !== undefined ? { id: spec.id } : {}) });
  }
  if (text !== undefined && text.length > 0) {
    setResourceDescription(page, id, text, author);
  }
  if (spec.frame !== undefined) {
    addToFrame(page, spec.frame, [id]);
  }
  const resource = findResource(page, id);
  if (!resource) {
    throw invalid(`${resourceName({ kind: spec.kind, name })} couldn't be added.`);
  }
  const status: AgentItemStatus = before === null ? "created" : JSON.stringify([resource, frameOf(page, id)?.id]) === before ? "kept" : "updated";
  return resourceResult(ctx, page, resource, status, at ? { at } : {});
}

/** Loads the types tables are backed by (their columns and sizes), refusing a type the project doesn't have; returns how to apply the operation. */
export async function prepareAddResources(op: AddResourcesOp, ctx: OpContext): Promise<ApplyOp> {
  for (const spec of op.resources) {
    if (spec.type === undefined || spec.kind !== "table") {
      continue;
    }
    if (!(await ctx.data.loadDefinition(ctx.board.project, spec.type))) {
      throw invalid(
        `The board's project has no type declaration or class with the key “${spec.type}” to back the table “${spec.name}”. Keys come from search (kinds: types) or read_code; or list the table's own columns instead.`,
      );
    }
  }
  return (page, applyCtx) => op.resources.map((spec): AgentItemResult => applyResourceSpec(applyCtx, page, spec, op.replacePeopleText));
}
