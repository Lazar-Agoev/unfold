/**
 * The rows of On this page (redesign §4.4, components §4.5, V-36, V-59): the page's cards in follow
 * order indented by call depth (from the page's outline as of the last save; cards added since go last
 * at depth 0), each card's groups nested under it, and each drawn link right after its `from` card's
 * groups, one level deeper. Pure, so it is tested without a DOM.
 */

import type { Card, Group, HealthItem, Link, Page, PageOutline, ProjectName } from "../ipc/generated";
import { cardKey, keyDisplayName } from "../state/boards";

interface RowBase {
  /** Unique within the page: card, group, and link ids share one namespace. */
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
  | (RowBase & { readonly kind: "link"; readonly link: Link; readonly label: string });

interface PageRows {
  readonly rows: readonly PageRow[];
  /** Whether the page holds cards from more than one project: the rows then carry a monogram (V-36). */
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
      return "card" in target ? [target.card] : "group" in target ? [target.group] : "link" in target ? [target.link] : [];
    }),
  );
  const names = new Map(page.cards.map((card) => [card.id, keyDisplayName(cardKey(card))]));
  const rows: PageRow[] = [];
  const projects = new Set<ProjectName>();
  for (const { card, depth } of followOrder(outline, page.cards)) {
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
        const label = link.label?.text ?? `link to ${names.get(link.to) ?? link.to}`;
        rows.push({ kind: "link", id: link.id, depth: depth + 1, review: marked.has(link.id), link, label });
      }
    }
  }
  return { rows, multiProject: projects.size > 1 };
}
