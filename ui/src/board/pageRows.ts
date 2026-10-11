/**
 * The rows of On this page: the page's cards in follow order indented by call depth (from the page's
 * outline as of the last save; cards added since go last at depth 0), each card's groups nested under
 * it, and each drawn link and connection right after its `from` card's groups, one level deeper; then
 * the resource cards, each with the connections leaving it. Frames are headings where their first card
 * comes in follow order, with what they hold indented under them (nested frames nested), each card
 * indented by its call depth from the shallowest card beside it. Pure, so it is tested without a DOM.
 */

import { KIND_WORDS } from "../canvas/connections/chip";
import type { Card, Frame, Group, HealthItem, Link, Page, PageOutline, ProjectName, Resource } from "../ipc/generated";
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
  | (RowBase & { readonly kind: "frame"; readonly frame: Frame })
  | (RowBase & { readonly kind: "resource"; readonly resource: Resource });

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
      if ("card" in target) return [target.card];
      if ("group" in target) return [target.group];
      if ("link" in target) return [target.link];
      if ("frame" in target) return [target.frame];
      return "resource" in target ? [target.resource] : [];
    }),
  );
  const names = new Map([...page.cards.map((card) => [card.id, keyDisplayName(cardKey(card))] as const), ...(page.resources ?? []).map((resource) => [resource.id, resource.name] as const)]);
  const frames = new Map((page.frames ?? []).map((frame) => [frame.id, frame]));
  const resources = new Map((page.resources ?? []).map((resource) => [resource.id, resource]));
  const rows: PageRow[] = [];
  const projects = new Set<ProjectName>();
  /** The links and connections leaving item `id`, one level under it. */
  const linkRows = (id: string, depth: number) => {
    for (const link of page.links) {
      if (link.from === id) {
        const to = names.get(link.to) ?? frames.get(link.to)?.title ?? link.to;
        const label = link.kind !== undefined ? `${KIND_WORDS[link.kind]} · ${link.label?.text ?? to}` : (link.label?.text ?? `link to ${to}`);
        rows.push({ kind: "link", id: link.id, depth, review: marked.has(link.id), link, label });
      }
    }
  };
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
    linkRows(card.id, depth + 1);
  };

  const follow = followOrder(outline, page.cards);
  const order = new Map(follow.map(({ card, depth }, index) => [card.id, { card, depth, index }]));
  // Resource cards come after the cards, in the page's order.
  const after = new Map([...resources.keys()].map((id, index) => [id, follow.length + index]));
  const rank = (id: string) => order.get(id)?.index ?? after.get(id);
  const held = new Set([...frames.values()].flatMap((frame) => frame.members));
  // Where a frame comes: where its first card comes in follow order (last when it holds none).
  const first = (id: string, seen: ReadonlySet<string>): number =>
    rank(id) ?? Math.min(Number.MAX_SAFE_INTEGER, ...(seen.has(id) ? [] : (frames.get(id)?.members ?? []).map((member) => first(member, new Set([...seen, id])))));
  const listed = new Set<string>();
  const emit = (ids: readonly string[], level: number) => {
    const items = ids.filter((id) => !listed.has(id) && (order.has(id) || frames.has(id) || resources.has(id))).sort((a, b) => first(a, new Set()) - first(b, new Set()));
    const shallowest = Math.min(...items.flatMap((id) => order.get(id)?.depth ?? []));
    for (const id of items) {
      listed.add(id);
      const card = order.get(id);
      const frame = frames.get(id);
      const resource = resources.get(id);
      if (card) {
        cardRow(card.card, level === 0 ? card.depth : level + card.depth - shallowest);
      } else if (resource) {
        rows.push({ kind: "resource", id, depth: level, review: marked.has(id), resource });
        linkRows(id, level + 1);
      } else if (frame) {
        rows.push({ kind: "frame", id, depth: level, review: marked.has(id), frame });
        emit(frame.members, level + 1);
      }
    }
  };
  emit([...follow.map(({ card }) => card.id), ...resources.keys(), ...frames.keys()].filter((id) => !held.has(id)), 0);
  return { rows, multiProject: projects.size > 1 };
}
