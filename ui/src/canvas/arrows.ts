/**
 * Arrows between cards (docs/reader.md#boards): one per opened call occurrence, that is, every
 * occurrence shown in a card whose resolved target has a card on the board. Each leaves from its own
 * port (the end of the calling line, or the Flow node), even when several calls go to one card.
 * Imports and passed references are dashed. An entrance points at each handler on the page. The
 * graph has no edges between projects, so arrows join only cards that resolve to the same project.
 * The board file holds no arrows; they are derived from the cards and their models.
 *
 * A call expanded in place shows the callee's (or a callback's) own Flow inside the card: the calls
 * that Flow shows leave from its steps (`insetPort`), each inset's apart, so several calls of one
 * function keep their own arrows. A callback's own Flow takes its calls from the card's.
 *
 * A card shown as a Diagram node or a type card's fields has no rows to leave from: its arrows leave
 * from the node's edge, and all its calls to one card merge into one arrow. A type card's fields draw
 * type arrows (design §4.2): from a field's row to each card on the page its written type names, one
 * per field and card, labeled with the written type.
 */

import type { CardView, CodeView, EdgeKind, Key, ProjectId, TypeDefinition, TypeMember, TypeText } from "../ipc/generated";
import { isWithin } from "../code/outline";
import type { CardBody } from "../code/typeKinds";
import { cardRefKey } from "../state/boards/keys";

export interface ArrowCard {
  readonly id: string;
  readonly key: Key;
  /** The project the card resolves to; null for a card of a project that isn't connected. */
  readonly project: ProjectId | null;
  readonly entrance: boolean;
  /** What its body shows (`cardBody`): a node's and a type card's arrows leave from the node's edge. */
  readonly body: CardBody;
  /** The calls it shows expanded in place, outermost first (only while its Flow shows them). */
  readonly insets?: readonly ArrowInset[];
}

/** A call expanded in place in a card, drawn: its saved path, and the block whose Flow it shows. */
export interface ArrowInset {
  readonly path: readonly Key[];
  readonly owner: Key;
}

/** One call an arrow draws: its occurrence, its edge, and its calling line. */
interface ArrowCall {
  readonly source: Key;
  readonly seq: number;
  readonly line: number;
  readonly kind: EdgeKind;
}

export interface Arrow {
  /** Stable: source card, port, target card. */
  readonly id: string;
  readonly source: string;
  /**
   * The source card's port: the occurrence key, `target:<position>` for an entrance handler, `node` for
   * a Diagram node or a type card, or `field:<row>` for a type card's field.
   */
  readonly sourceHandle: string;
  readonly target: string;
  /** The project both cards resolve to: the line's labels and contract are asked of it. */
  readonly project: ProjectId;
  /** The occurrence it draws (with `seq`, the graph edge); a handler's or a type arrow's port. */
  readonly occurrence: string;
  /** With the occurrence key, names the graph edge: 0 for the call itself, more for a passed reference. */
  readonly seq: number;
  readonly kind: EdgeKind | "handler" | "type";
  readonly dashed: boolean;
  /** Source order inside the calling card. */
  readonly order: number;
  /** The calling line, when the occurrence has one. */
  readonly line: number | null;
  /** Every call it stands for, in source order, when it leaves a node and joins several calls to one card. */
  readonly calls?: readonly ArrowCall[];
  /** A type arrow's label: the field's written type. */
  readonly type?: TypeText;
  /**
   * When it joins several arrows at a collapsed frame (`routeArrows`): how many calls (handlers
   * included) and how many fields' type links it stands for.
   */
  readonly merged?: { readonly calls: number; readonly fields: number };
  /** The cards it joins, when an end attaches to a collapsed frame instead (`routeArrows`). */
  readonly cards?: { readonly from: string; readonly to: string };
}

/** The port a Diagram node's (or a type card's) arrows leave from: its header's right edge. */
export const NODE_PORT = "node";

/** The handle id of an entrance's target row. */
export function targetHandle(position: number): string {
  return `target:${position}`;
}

/** The port of a type card's field in row `row` (`0`, `from1/2`, `=`: see `fieldRows`). */
export function fieldPort(row: string): string {
  return `field:${row}`;
}

const INSET_PORT = "inset:";

/** The port of `port` (a call's occurrence key) shown in the call expanded in place at `path`. */
export function insetPort(path: readonly Key[], port: string): string {
  return `${INSET_PORT}${JSON.stringify([path, port])}`;
}

/** The inset path and inner port of an `insetPort`, if `id` is one. */
export function insetPortOf(id: string): { readonly path: readonly Key[]; readonly port: string } | undefined {
  if (!id.startsWith(INSET_PORT)) {
    return undefined;
  }
  const [path, port] = JSON.parse(id.slice(INSET_PORT.length)) as [Key[], string];
  return { path, port };
}

/** Whether `path` is one key longer than `prefix` and starts with it. */
function isChildPath(path: readonly Key[], prefix: readonly Key[]): boolean {
  return path.length === prefix.length + 1 && prefix.every((key, index) => path[index] === key);
}

/** Whether a card's arrows leave from its node's edge (a Diagram node, or a type card's fields). */
function leavesFromEdge(body: CardBody): boolean {
  return body === "node" || body === "fields";
}

interface Occurrence extends ArrowCall {
  readonly target: string;
}

/** The resolved occurrences a block card shows: its Code links (or the last verified ones), else its Flow chips. */
export function occurrencesOf(view: Extract<CardView, { type: "block" }>, stale: CodeView | undefined): Occurrence[] {
  const code = view.code?.source.state === "current" ? view.code : stale;
  if (code?.source.state === "current") {
    return code.source.links.flatMap((link) =>
      link.resolution === "resolved" && link.target !== undefined ? [{ source: link.source, seq: 0, target: link.target, kind: link.kind, line: link.line }] : [],
    );
  }
  return (view.flow?.nodes ?? []).flatMap((node) =>
    (node.chips ?? []).flatMap((chip) =>
      chip.resolution === "resolved" && chip.target !== undefined ? [{ source: chip.source, seq: chip.seq, target: chip.target, kind: chip.kind, line: chip.line }] : [],
    ),
  );
}

/** One row of a type card's fields: its id, the member it shows, and its written type. */
export interface FieldRow {
  /** `=` for an alias's aliased type, `<index>` for a member, `from<g>/<index>` for a member inherited from base g. */
  readonly row: string;
  /** The member; null for an alias's aliased type. */
  readonly member: TypeMember | null;
  readonly type: TypeText | undefined;
  /** For an inherited member, the index of the base it comes from. */
  readonly base?: number;
}

/**
 * The rows a type card lists, in order: an alias's aliased type, its own members, then the members it
 * inherits, base by base.
 */
export function fieldRows(definition: TypeDefinition): FieldRow[] {
  return [
    ...(definition.aliased ? [{ row: "=", member: null, type: definition.aliased }] : []),
    ...definition.members.map((member, index) => ({ row: String(index), member, type: member.type })),
    ...definition.inherited.flatMap((group, base) =>
      group.members.map((member, index) => ({ row: `from${base}/${index}`, member, type: member.type, base })),
    ),
  ];
}

/**
 * The calls of `card` as arrows: one per occurrence, or, leaving a node's edge, one per card reached.
 * `port` names the port each occurrence leaves from (itself in the card's own Flow); `first` is the
 * source order of the first.
 */
function callArrows(
  card: ArrowCard,
  project: ProjectId,
  occurrences: readonly Occurrence[],
  cardOf: (key: Key) => string | undefined,
  port: (occurrence: Key) => string = (occurrence) => occurrence,
  first = 0,
): Arrow[] {
  const byTarget = new Map<string, { order: number; calls: ArrowCall[] }>();
  const arrows: Arrow[] = [];
  const seen = new Set<string>();
  occurrences.forEach((occurrence, index) => {
    const order = first + index;
    const targetCard = cardOf(occurrence.target);
    if (targetCard === undefined || targetCard === card.id) {
      return;
    }
    const dashed = occurrence.kind === "imports" || occurrence.kind === "references";
    if (leavesFromEdge(card.body)) {
      const merged = byTarget.get(targetCard) ?? { order, calls: [] };
      if (!merged.calls.some((call) => call.source === occurrence.source && call.seq === occurrence.seq)) {
        merged.calls.push({ source: occurrence.source, seq: occurrence.seq, line: occurrence.line, kind: occurrence.kind });
      }
      byTarget.set(targetCard, merged);
      return;
    }
    const handle = port(occurrence.source);
    const id = `${card.id}|${handle}|${targetCard}`;
    if (!seen.has(id)) {
      seen.add(id);
      arrows.push({
        id,
        source: card.id,
        sourceHandle: handle,
        target: targetCard,
        project,
        occurrence: occurrence.source,
        seq: occurrence.seq,
        kind: occurrence.kind,
        dashed,
        order,
        line: occurrence.line,
      });
    }
  });
  for (const [targetCard, { order, calls }] of byTarget) {
    const first = calls.find((call) => call.kind === "calls" || call.kind === "constructs") ?? calls[0];
    if (!first) {
      continue;
    }
    arrows.push({
      id: `${card.id}|${NODE_PORT}|${targetCard}`,
      source: card.id,
      sourceHandle: NODE_PORT,
      target: targetCard,
      project,
      occurrence: first.source,
      seq: first.seq,
      kind: first.kind,
      dashed: calls.every((call) => call.kind === "imports" || call.kind === "references"),
      order,
      line: first.line,
      ...(calls.length > 1 ? { calls } : {}),
    });
  }
  return arrows;
}

/** A type card's type arrows: from each field's row to each other card of the project its written type names. */
function typeArrows(card: ArrowCard, project: ProjectId, definition: TypeDefinition, cardOf: (key: Key) => string | undefined): Arrow[] {
  // A base on the page lists its own fields there, with their arrows.
  const shownBases = new Set(
    definition.inherited.flatMap((group, base) => {
      const span = group.from.spans[0];
      return span?.style === "declared" && span.target !== undefined && cardOf(span.target) !== undefined ? [base] : [];
    }),
  );
  const arrows: Arrow[] = [];
  fieldRows(definition).forEach(({ row, type, base }, order) => {
    if (!type || (base !== undefined && shownBases.has(base))) {
      return;
    }
    const reached = new Set<string>();
    for (const span of type.spans) {
      const targetCard = span.style === "declared" && span.target !== undefined ? cardOf(span.target) : undefined;
      if (targetCard === undefined || targetCard === card.id || reached.has(targetCard)) {
        continue;
      }
      reached.add(targetCard);
      const port = fieldPort(row);
      arrows.push({
        id: `${card.id}|${port}|${targetCard}`,
        source: card.id,
        sourceHandle: port,
        target: targetCard,
        project,
        occurrence: port,
        seq: 0,
        kind: "type",
        dashed: false,
        order,
        line: null,
        type,
      });
    }
  });
  return arrows;
}

/**
 * The calls of a block card as arrows: its own Flow's, then each call expanded in place's from its
 * steps. A callback's own Flow (a block inside the one above it) takes its calls from that Flow.
 */
function insetCallArrows(
  card: ArrowCard,
  project: ProjectId,
  models: ReadonlyMap<string, CardView>,
  stale: (project: ProjectId, key: Key) => CodeView | undefined,
  cardOf: (key: Key) => string | undefined,
): Arrow[] {
  const insets = card.insets ?? [];
  const arrows: Arrow[] = [];
  let order = 0;
  for (const { path, owner } of [{ path: [], owner: card.key }, ...insets]) {
    const model = models.get(cardRefKey(project, owner));
    if (model?.type !== "block") {
      continue;
    }
    const inner = insets.filter((inset) => isChildPath(inset.path, path) && isWithin(inset.owner, owner));
    const occurrences = occurrencesOf(model, stale(project, owner)).filter((occurrence) => !inner.some((inset) => isWithin(occurrence.source, inset.owner)));
    arrows.push(...callArrows(card, project, occurrences, cardOf, path.length === 0 ? undefined : (occurrence) => insetPort(path, occurrence), order));
    order += occurrences.length;
  }
  return arrows;
}

/**
 * Every arrow on a page. `models` maps `cardRefKey(project, key)` to loaded card models (missing ones
 * draw no arrows yet); `stale` gives the last verified Code of a block whose source changed;
 * `definitions` maps the same keys to the loaded definitions of the type cards that show their fields.
 */
export function computeArrows(
  cards: readonly ArrowCard[],
  models: ReadonlyMap<string, CardView>,
  stale: (project: ProjectId, key: Key) => CodeView | undefined = () => undefined,
  definitions: ReadonlyMap<string, TypeDefinition> = new Map(),
): Arrow[] {
  const cardOfRef = new Map<string, string>();
  for (const card of cards) {
    const ref = card.project !== null ? cardRefKey(card.project, card.key) : null;
    if (!card.entrance && ref !== null && !cardOfRef.has(ref)) {
      cardOfRef.set(ref, card.id);
    }
  }
  const arrows: Arrow[] = [];
  for (const card of cards) {
    const { project } = card;
    const model = project !== null ? models.get(cardRefKey(project, card.key)) : undefined;
    if (!model || project === null) {
      continue;
    }
    const cardOf = (key: Key) => cardOfRef.get(cardRefKey(project, key));
    if (model.type === "entrance") {
      const seen = new Set<string>();
      for (const target of model.targets) {
        const targetCard = target.resolution === "resolved" && target.target !== null ? cardOf(target.target) : undefined;
        if (targetCard === undefined || targetCard === card.id) {
          continue;
        }
        const handle = leavesFromEdge(card.body) ? NODE_PORT : targetHandle(target.position);
        const id = `${card.id}|${handle}|${targetCard}`;
        if (!seen.has(id)) {
          seen.add(id);
          arrows.push({
            id,
            source: card.id,
            sourceHandle: handle,
            target: targetCard,
            project,
            occurrence: targetHandle(target.position),
            seq: 0,
            kind: "handler",
            dashed: false,
            order: target.position,
            line: null,
          });
        }
      }
      continue;
    }
    arrows.push(...insetCallArrows(card, project, models, stale, cardOf));
    const definition = card.body === "fields" ? definitions.get(cardRefKey(project, card.key)) : undefined;
    if (definition) {
      arrows.push(...typeArrows(card, project, definition, cardOf));
    }
  }
  return arrows;
}

/** Each card's ports (handle ids with an arrow), sorted, for the cards to draw. */
export function portsByCard(arrows: readonly Arrow[]): Map<string, string[]> {
  const ports = new Map<string, string[]>();
  for (const arrow of arrows) {
    const list = ports.get(arrow.source) ?? [];
    if (!list.includes(arrow.sourceHandle)) {
      list.push(arrow.sourceHandle);
    }
    ports.set(arrow.source, list);
  }
  for (const list of ports.values()) {
    list.sort();
  }
  return ports;
}
