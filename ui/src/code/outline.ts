/**
 * Helpers over the Code model's statement outline (reader.md "Outline, folds, and groups"): the
 * item tree, default folds, which fold hides an offset, and the rule that turns a run of lines into a
 * group of consecutive sibling statements.
 */

import type { CodeLine, OutlineItem } from "../ipc/generated";

/** Children of each outline item by index (`-1` holds the top-level items), in source order. */
export function outlineChildren(outline: readonly OutlineItem[]): Map<number, number[]> {
  const children = new Map<number, number[]>();
  outline.forEach((item, index) => {
    const parent = item.parent ?? -1;
    const list = children.get(parent) ?? [];
    list.push(index);
    children.set(parent, list);
  });
  return children;
}

/** The folds a card starts with: the model's default-folded flags (callbacks, long `else if` chains). */
export function defaultFolds(outline: readonly OutlineItem[]): string[] {
  return outline.filter((item) => item.fold?.folded === true).map((item) => item.key);
}

/**
 * The outermost folded item containing `offset`, i.e. the fold row that shows it, or undefined
 * when the offset is visible.
 */
export function foldHiding(outline: readonly OutlineItem[], folded: ReadonlySet<string>, offset: number): OutlineItem | undefined {
  let found: OutlineItem | undefined;
  for (const item of outline) {
    if (item.fold && folded.has(item.key) && offset >= item.start && offset < item.end) {
      if (!found || (item.start <= found.start && item.end >= found.end)) {
        found = item;
      }
    }
  }
  return found;
}

/** The line containing `offset` (lines are sorted; offsets are relative to the shown text). */
export function lineAt(lines: readonly CodeLine[], offset: number): CodeLine | undefined {
  let low = 0;
  let high = lines.length - 1;
  let found: CodeLine | undefined = lines[0];
  while (low <= high) {
    const middle = (low + high) >> 1;
    const line = lines[middle];
    if (!line) {
      break;
    }
    if (line.start <= offset) {
      found = line;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return found;
}

/** The roles a group member keeps (reader.md: body, then, else, catch, finally). */
const BODY_ROLES = new Set(["body", "then", "else", "catch", "finally"]);

/** What a run of lines makes: the members of a group, or why it can't be one. */
export type GroupSelection = { readonly members: string[] } | { readonly error: string };

/**
 * The group lines `from`–`to` make (reader.md "Groups"): the consecutive siblings they cover,
 * descending into a compound item only when the lines lie inside its body
 * (below its first line). A group holds statements of one body (`body`, `then`, `else`, `catch`,
 * `finally`) and never the card's block itself, a nested definition, or statements of two bodies.
 * Saving validates again in Rust.
 */
export function groupSelection(outline: readonly OutlineItem[], fromLine: number, toLine: number): GroupSelection {
  const from = Math.min(fromLine, toLine);
  const to = Math.max(fromLine, toLine);
  const children = outlineChildren(outline);
  let parent = -1;
  for (let guard = 0; guard <= outline.length; guard += 1) {
    const siblings = (children.get(parent) ?? []).filter((index) => {
      const item = outline[index];
      return item !== undefined && item.startLine <= to && item.endLine >= from;
    });
    const only = siblings.length === 1 ? siblings[0] : undefined;
    const single = only !== undefined ? outline[only] : undefined;
    if (only !== undefined && single && from > single.startLine && to <= single.endLine && (children.get(only)?.length ?? 0) > 0) {
      parent = only;
      continue;
    }
    const covered = siblings.map((index) => outline[index]).filter((item): item is OutlineItem => item !== undefined);
    const first = covered[0];
    if (!first) {
      return { error: "Select the lines of whole statements to group them." };
    }
    if (first.role === undefined || !BODY_ROLES.has(first.role)) {
      return { error: "A group holds statements inside a body, not the card's own header." };
    }
    const roles = new Set(covered.map((item) => item.role));
    if (roles.size > 1) {
      return { error: "A group can't cross a body boundary: these lines are in more than one branch." };
    }
    if (covered.some((item) => !item.groupable)) {
      return { error: "A nested definition can't be part of a group." };
    }
    return { members: covered.map((item) => item.key) };
  }
  return { error: "Select the lines of whole statements to group them." };
}

/**
 * Checks a new group against the card's groups (by their members' lines): groups never overlap or
 * nest. Returns the reason, or null when it fits.
 */
export function groupConflict(
  outline: readonly OutlineItem[],
  members: readonly string[],
  groups: readonly { readonly title: string; readonly members: readonly string[] }[],
): string | null {
  const byKey = new Map(outline.map((item) => [item.key, item]));
  const spans = members.map((key) => byKey.get(key)).filter((item): item is OutlineItem => item !== undefined);
  for (const group of groups) {
    const taken = group.members.map((key) => byKey.get(key)).filter((item): item is OutlineItem => item !== undefined);
    if (spans.some((a) => taken.some((b) => linesOverlap(a, b)))) {
      return `These lines overlap the group "${group.title}". Groups never overlap or nest.`;
    }
  }
  return null;
}

/** Whether two line ranges overlap. */
function linesOverlap(a: { startLine: number; endLine: number }, b: { startLine: number; endLine: number }): boolean {
  return a.startLine <= b.endLine && b.startLine <= a.endLine;
}

/** What a connection's port id starts with: it leaves from a step, a statement or call of the card (design §4.7). */
const STEP_PORT = "step:";

/** The port of a connection leaving from step `key`. */
export function stepPort(key: string): string {
  return `${STEP_PORT}${key}`;
}

/** The step a connection's port leaves from; undefined for any other port (an occurrence, a field, the node). */
export function portStep(port: string): string | undefined {
  return port.startsWith(STEP_PORT) ? port.slice(STEP_PORT.length) : undefined;
}

/** Whether `key` is `outer` or a block inside it. */
export function isWithin(key: string, outer: string): boolean {
  return key === outer || key.startsWith(`${outer}/`);
}

/** The innermost outline item that is `key` or holds it, if any. */
export function innermostItem(outline: readonly OutlineItem[], key: string): OutlineItem | undefined {
  let found: OutlineItem | undefined;
  for (const item of outline) {
    if (isWithin(key, item.key) && (!found || item.key.length > found.key.length)) {
      found = item;
    }
  }
  return found;
}
