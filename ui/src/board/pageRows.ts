/**
 * The rows of On this page: the page's cards in follow order indented by call depth (from the page's
 * outline as of the last save; cards added since go last at depth 0), each card's groups nested under
 * it, and each drawn link right after its `from` card's groups, one level deeper. Frames are headings
 * where their first card comes in follow order, with what they hold indented under them (nested
 * frames nested), each card indented by its call depth from the shallowest card beside it. Pure, so
 * it is tested without a DOM.
 */

import type { Card, Frame, Group, HealthItem, Link, Page, PageOutline, ProjectName } from "../ipc/generated";
import { cardKey, keyDisplayName } from "../state/boards";

interface RowBase {
  /** Unique within the page: card, group, link, and frame ids share one namespace. */
  readonly id: string;
  readonly depth: number;
  /** Something about the row needs review. */
  readonly review: boolean;
}

export type PageRow =
  | (RowBase & {
      readonly kind: "card";
      readonly card: Card;
      readonly name: string;
      readonly entrance: boolean;
      /** The shared name of the card's project, for the monogram column. */
      readonly project: ProjectName;
      /** Whether its project is connected here (else it shows "not connected"). */
      readonly connected: boolean;
    })
  | (RowBase & { readonly kind: "group"; readonly group: Group })
  | (RowBase & { readonly kind: "link"; readonly link: Link; readonly label: string })
  | (RowBase & { readonly kind: "frame"; readonly frame: Frame });

interface PageRows {
  readonly rows: readonly PageRow[];
  /** Whether the page holds cards from more than one project: the rows then carry a monogram. */
  readonly multiProject: boolean;
}

interface RowContext {
  /** The shared name of the board's own project. */
  readonly boardProject: ProjectName;
  /** Whether a card's `ref.project` (undefined: the board's own) resolves to a connected project. */
  readonly connected: (project: ProjectName | undefined) => boolean;
  readonly items: readonly HealthItem[];
}

/** The page's cards in follow order with their call depth. */
export function followOrder(outline: PageOutline | undefined, cards: readonly Card[]): { card: Card; depth: number }[] {
  const byId = new Map(cards.map((card) => [card.id, card]));
  const rows: { card: Card; depth: number }[] = [];
  const seen = new Set<string>();
  for (const row of outline?.rows ?? []) {
    const card = byId.get(row.card);
    if (card && !seen.has(card.id)) {
      seen.add(card.id);
      rows.push({ card, depth: row.depth });
    }
  }
  for (const card of cards) {
    if (!seen.has(card.id)) {
      rows.push({ card, depth: 0 });
    }
  }
  return rows;
}

/** Every row of On this page. */
export function pageRows(page: Page, outline: PageOutline | undefined, context: RowContext): PageRows {
  const marked = new Set(
    context.items.flatMap((item) => {
      if (item.page !== page.id || item.kind === "notConnected") {
        return [];
      }
      const { target } = item;
      return "card" in target ? [target.card] : "group" in target ? [target.group] : "link" in target ? [target.link] : "frame" in target ? [target.frame] : [];
    }),
  );
  const names = new Map(page.cards.map((card) => [card.id, keyDisplayName(cardKey(card))]));
  const frames = new Map((page.frames ?? []).map((frame) => [frame.id, frame]));
  const rows: PageRow[] = [];
  const projects = new Set<ProjectName>();
  const cardRow = (card: Card, depth: number) => {
    const project = card.ref.project ?? context.boardProject;
    projects.add(project);
    rows.push({
      kind: "card",
      id: card.id,
      depth,
      review: marked.has(card.id),
      card,
      name: names.get(card.id) ?? card.id,
      entrance: "entrance" in card.ref,
      project,
      connected: context.connected(card.ref.project),
    });
    for (const group of page.groups) {
      if (group.card === card.id) {
        rows.push({ kind: "group", id: group.id, depth: depth + 1, review: marked.has(group.id), group });
      }
    }
    for (const link of page.links) {
      if (link.from === card.id) {
        const label = link.label?.text ?? `link to ${names.get(link.to) ?? frames.get(link.to)?.title ?? link.to}`;
        rows.push({ kind: "link", id: link.id, depth: depth + 1, review: marked.has(link.id), link, label });
      }
    }
  };

  const follow = followOrder(outline, page.cards);
  const order = new Map(follow.map(({ card, depth }, index) => [card.id, { card, depth, index }]));
  const held = new Set([...frames.values()].flatMap((frame) => frame.members));
  // Where a frame comes: where its first card comes in follow order (last when it holds none).
  const first = (id: string, seen: ReadonlySet<string>): number =>
    order.get(id)?.index ?? Math.min(Number.POSITIVE_INFINITY, ...(seen.has(id) ? [] : (frames.get(id)?.members ?? []).map((member) => first(member, new Set([...seen, id])))));
  const listed = new Set<string>();
  const emit = (ids: readonly string[], level: number) => {
    const items = ids.filter((id) => !listed.has(id) && (order.has(id) || frames.has(id))).sort((a, b) => first(a, new Set()) - first(b, new Set()));
    const shallowest = Math.min(...items.flatMap((id) => order.get(id)?.depth ?? []));
    for (const id of items) {
      listed.add(id);
      const card = order.get(id);
      const frame = frames.get(id);
      if (card) {
        cardRow(card.card, level === 0 ? card.depth : level + card.depth - shallowest);
      } else if (frame) {
        rows.push({ kind: "frame", id, depth: level, review: marked.has(id), frame });
        emit(frame.members, level + 1);
      }
    }
  };
  emit([...follow.map(({ card }) => card.id), ...frames.keys()].filter((id) => !held.has(id)), 0);
  return { rows, multiProject: projects.size > 1 };
}
