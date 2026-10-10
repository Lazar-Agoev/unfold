/**
 * Add's `>` mode (R-13, 04b): every registered action that shows in Add, matched against what follows
 * the `>`. Without a query the commands are grouped (`ADD_GROUPS` order); with one they are ranked in a
 * single "Commands" group, the matched part of each name in bold.
 */

import { ADD_GROUPS, type Action } from "../keyboard";
import type { TextRange } from "./highlight";

/** A command row: the action, the submenu it lives in (`Share ›`), and the bold runs of its title. */
export interface CommandItem {
  readonly action: Action;
  /** The group shown before the title, for an action whose group differs from its category. */
  readonly prefix: string | null;
  readonly ranges: readonly TextRange[];
}

interface CommandGroup {
  readonly id: string;
  readonly label: string;
  readonly items: readonly CommandItem[];
}

interface Match {
  readonly score: number;
  readonly ranges: readonly TextRange[];
}

const WORD_BREAK = /[\s\-_./›:]/;

function atWordStart(text: string, index: number): boolean {
  return index === 0 || WORD_BREAK.test(text.charAt(index - 1));
}

/** Where each character of `query` appears in order in `text`, merged into runs; null when it doesn't. */
function subsequence(text: string, query: string): TextRange[] | null {
  const ranges: [number, number][] = [];
  let from = 0;
  for (const character of query) {
    if (character === " ") {
      continue;
    }
    const index = text.indexOf(character, from);
    if (index < 0) {
      return null;
    }
    const last = ranges.at(-1);
    if (last?.[1] === index) {
      last[1] = index + 1;
    } else {
      ranges.push([index, index + 1]);
    }
    from = index + 1;
  }
  return ranges;
}

/**
 * How well `query` (lowercase, trimmed) matches an action: its title by prefix, word start, substring,
 * every word, or in order; else its keywords, group, or category (no bold run). Null when it doesn't.
 */
export function matchCommand(query: string, action: Pick<Action, "title" | "keywords" | "group" | "category">): Match | null {
  if (query.length === 0) {
    return { score: 0, ranges: [] };
  }
  const title = action.title.toLowerCase();
  const index = title.indexOf(query);
  if (index >= 0) {
    const score = index === 0 ? 1000 : atWordStart(title, index) ? 800 : 600;
    return { score: score - title.length, ranges: [[index, index + query.length]] };
  }
  const words = query.split(/\s+/);
  if (words.length > 1) {
    const ranges: TextRange[] = [];
    for (const word of words) {
      const at = title.indexOf(word);
      if (at < 0) {
        break;
      }
      ranges.push([at, at + word.length]);
    }
    if (ranges.length === words.length) {
      return { score: 500 - title.length, ranges };
    }
  }
  const ordered = subsequence(title, query);
  if (ordered) {
    const first = ordered[0]?.[0] ?? 0;
    const last = ordered.at(-1)?.[1] ?? 0;
    return { score: 300 - (last - first) - ordered.length * 4, ranges: ordered };
  }
  const extra = [...(action.keywords ?? []), action.group ?? "", action.category].map((word) => word.toLowerCase());
  if (extra.some((word) => word.length > 0 && word.includes(query))) {
    return { score: 100, ranges: [] };
  }
  return null;
}

/** An action's Add group: its `group`, else its category. */
function groupOf(action: Action): string {
  return action.group ?? action.category;
}

function groupRank(group: string): number {
  const index = (ADD_GROUPS as readonly string[]).indexOf(group);
  return index < 0 ? ADD_GROUPS.length : index;
}

/**
 * The command groups for `query` (what follows the `>`): the enabled actions that show in Add. No
 * query: one group per `ADD_GROUPS` entry (others after, by name), titles in order. A query: one
 * "Commands" group, best match first.
 */
export function commandGroups(actions: readonly Action[], query: string, isEnabled: (id: string) => boolean): CommandGroup[] {
  const visible = actions.filter((action) => action.showInAdd !== false && isEnabled(action.id));
  const wanted = query.trim().toLowerCase();
  if (wanted.length === 0) {
    const byGroup = new Map<string, CommandItem[]>();
    for (const action of visible) {
      const group = groupOf(action);
      byGroup.set(group, [...(byGroup.get(group) ?? []), { action, prefix: null, ranges: [] }]);
    }
    return [...byGroup.entries()]
      .sort(([a], [b]) => groupRank(a) - groupRank(b) || a.localeCompare(b))
      .map(([group, items]) => ({
        id: group,
        label: group,
        items: items.sort((a, b) => a.action.title.localeCompare(b.action.title)),
      }));
  }
  const ranked = visible
    .flatMap((action) => {
      const match = matchCommand(wanted, action);
      return match ? [{ action, match }] : [];
    })
    .sort(
      (a, b) =>
        b.match.score - a.match.score ||
        groupRank(groupOf(a.action)) - groupRank(groupOf(b.action)) ||
        a.action.title.localeCompare(b.action.title),
    );
  const items = ranked.map(
    ({ action, match }): CommandItem => ({
      action,
      prefix: action.group !== undefined && action.group !== action.category ? action.group : null,
      ranges: match.ranges,
    }),
  );
  return items.length === 0 ? [] : [{ id: "commands", label: "Commands", items }];
}
