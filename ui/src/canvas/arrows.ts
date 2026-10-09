/**
 * Arrows between cards (architecture §10.2): one per opened call occurrence, that is, every
 * occurrence shown in a card whose resolved target has a card on the board. Each leaves from its own
 * port (the end of the calling line, or the Flow node), even when several calls go to one card.
 * Imports and passed references are dashed. An entrance points at each handler on the page. The
 * graph has no edges between projects, so arrows join only cards that resolve to the same project.
 * The board file holds no arrows; they are derived from the cards and their models.
 */

import type { CardView, CodeView, EdgeKind, Key, ProjectId } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";

export interface ArrowCard {
  readonly id: string;
  readonly key: Key;
  /** The project the card resolves to; null for a card of a project that isn't connected. */
  readonly project: ProjectId | null;
  readonly entrance: boolean;
}

export interface Arrow {
  /** Stable: source card, occurrence, target card. */
  readonly id: string;
  readonly source: string;
  /** The source card's port: the occurrence key, or `target:<position>` for an entrance handler. */
  readonly sourceHandle: string;
  readonly target: string;
  readonly kind: EdgeKind | "handler";
  readonly dashed: boolean;
  /** Source order inside the calling card. */
  readonly order: number;
  /** The calling line, when the occurrence has one. */
  readonly line: number | null;
}

/** The handle id of an entrance's target row. */
export function targetHandle(position: number): string {
  return `target:${position}`;
}

interface Occurrence {
  readonly source: string;
  readonly target: string;
  readonly kind: EdgeKind;
  readonly line: number;
}

/** The resolved occurrences a block card shows: its Code links (or the last verified ones), else its Flow chips. */
function occurrencesOf(view: Extract<CardView, { type: "block" }>, stale: CodeView | undefined): Occurrence[] {
  const code = view.code?.source.state === "current" ? view.code : stale;
  if (code?.source.state === "current") {
    return code.source.links.flatMap((link) =>
      link.resolution === "resolved" && link.target !== undefined ? [{ source: link.source, target: link.target, kind: link.kind, line: link.line }] : [],
    );
  }
  return (view.flow?.nodes ?? []).flatMap((node) =>
    (node.chips ?? []).flatMap((chip) =>
      chip.resolution === "resolved" && chip.target !== undefined ? [{ source: chip.source, target: chip.target, kind: chip.kind, line: chip.line }] : [],
    ),
  );
}

/**
 * Every arrow on a page. `models` maps `cardRefKey(project, key)` to loaded card models (missing ones
 * draw no arrows yet); `stale` gives the last verified Code of a block whose source changed.
 */
export function computeArrows(
  cards: readonly ArrowCard[],
  models: ReadonlyMap<string, CardView>,
  stale: (project: ProjectId, key: Key) => CodeView | undefined = () => undefined,
): Arrow[] {
  const cardOf = new Map<string, string>();
  for (const card of cards) {
    const ref = card.project !== null ? cardRefKey(card.project, card.key) : null;
    if (!card.entrance && ref !== null && !cardOf.has(ref)) {
      cardOf.set(ref, card.id);
    }
  }
  const arrows: Arrow[] = [];
  const seen = new Set<string>();
  for (const card of cards) {
    const { project } = card;
    const model = project !== null ? models.get(cardRefKey(project, card.key)) : undefined;
    if (!model || project === null) {
      continue;
    }
    if (model.type === "entrance") {
      for (const target of model.targets) {
        const targetCard = target.resolution === "resolved" && target.target !== null ? cardOf.get(cardRefKey(project, target.target)) : undefined;
        if (targetCard === undefined || targetCard === card.id) {
          continue;
        }
        const handle = targetHandle(target.position);
        const id = `${card.id}|${handle}|${targetCard}`;
        if (!seen.has(id)) {
          seen.add(id);
          arrows.push({ id, source: card.id, sourceHandle: handle, target: targetCard, kind: "handler", dashed: false, order: target.position, line: null });
        }
      }
      continue;
    }
    occurrencesOf(model, stale(project, card.key)).forEach((occurrence, order) => {
      const targetCard = cardOf.get(cardRefKey(project, occurrence.target));
      if (targetCard === undefined || targetCard === card.id) {
        return;
      }
      const id = `${card.id}|${occurrence.source}|${targetCard}`;
      if (seen.has(id)) {
        return;
      }
      seen.add(id);
      arrows.push({
        id,
        source: card.id,
        sourceHandle: occurrence.source,
        target: targetCard,
        kind: occurrence.kind,
        dashed: occurrence.kind === "imports" || occurrence.kind === "references",
        order,
        line: occurrence.line,
      });
    });
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
