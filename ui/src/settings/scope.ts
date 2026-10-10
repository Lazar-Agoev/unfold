/**
 * The scope tree of Project settings (08, V-70): where each folder stands with the saved `[scope]`
 * patterns and the ones staged in the dialog, and what checking or unchecking it stages.
 *
 * `scope_dir` says where a folder stands with the saved file (`skipped`). Staged changes are worked
 * out here: patterns this dialog adds are exact folder paths, escaped the way Rust writes them
 * (`escape_glob`), and a saved pattern is matched as Rust's scope does (the pattern, or a folder that
 * contains the path; `*`, `?`, `**` as globs). A pattern this matcher can't read (`[…]`, `{…}`) leaves
 * the folder where Rust put it.
 */

import type { ScopeFolder, SkipKind } from "../ipc";

/** Why a folder isn't graphed, as the tree's reason column says it (08). */
const SKIP_REASONS: Readonly<Record<SkipKind, string>> = {
  default: "skipped by default",
  ignored: "ignored by .gitignore",
  excluded: "excluded",
  not_included: "not included",
  language_off: "language turned off",
  javascript: "JavaScript isn't graphed",
  symlink: "link not followed",
  invalid_path: "name can't be graphed",
};

/** Skips that come from discovery, not from `[scope]`: the box is disabled. */
const FIXED: ReadonlySet<SkipKind> = new Set<SkipKind>(["default", "ignored", "language_off", "javascript", "symlink", "invalid_path"]);

/** The two `[scope]` lists. */
export interface ScopeLists {
  readonly include: readonly string[];
  readonly exclude: readonly string[];
}

/** A folder path as a pattern, escaped as Rust's `escape_glob` writes it. */
export function escapeGlob(path: string): string {
  return path.replace(/[*?[\]{}\\]/g, "\\$&");
}

/** A pattern's folder path when it has no glob characters (escapes undone), else null. */
export function literalOf(pattern: string): string | null {
  let text = "";
  for (let index = 0; index < pattern.length; index += 1) {
    const character = pattern[index] ?? "";
    if (character === "\\") {
      index += 1;
      text += pattern[index] ?? "";
    } else if ("*?[]{}".includes(character)) {
      return null;
    } else {
      text += character;
    }
  }
  return text.replace(/\/$/, "");
}

/** A glob of `*`, `?`, and `**` as a regular expression over a whole path; null for other syntax. */
function globExpression(pattern: string): RegExp | null {
  let source = "";
  const text = pattern.replace(/\/$/, "");
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index] ?? "";
    if (character === "\\") {
      index += 1;
      source += (text[index] ?? "").replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");
    } else if (character === "*" && text[index + 1] === "*") {
      index += 1;
      if (text[index + 1] === "/") {
        index += 1;
        source += "(?:.*/)?";
      } else {
        source += ".*";
      }
    } else if (character === "*") {
      source += "[^/]*";
    } else if (character === "?") {
      source += "[^/]";
    } else if ("[]{}".includes(character)) {
      return null;
    } else {
      source += character.replace(/[.+^$()|/]/g, "\\$&");
    }
  }
  return new RegExp(`^${source}$`, "u");
}

/** The path and every folder above it, innermost first. */
function selfAndAncestors(path: string): string[] {
  const parts = path.split("/");
  return parts.map((_, index) => parts.slice(0, parts.length - index).join("/"));
}

/**
 * Whether a pattern covers a path (matches it or a folder above it, as Rust's scope does). Null when
 * the pattern uses glob syntax this matcher doesn't read.
 */
export function covers(pattern: string, path: string): boolean | null {
  const literal = literalOf(pattern);
  if (literal !== null) {
    return path === literal || path.startsWith(`${literal}/`);
  }
  const expression = globExpression(pattern);
  return expression === null ? null : selfAndAncestors(path).some((candidate) => expression.test(candidate));
}

/** Whether a pattern names something strictly inside the folder at `path`. */
function liesUnder(pattern: string, path: string): boolean {
  const literal = literalOf(pattern) ?? pattern.slice(0, pattern.search(/[*?[\]{}]/));
  return literal.startsWith(`${path}/`);
}

/** Where a folder stands in the tree, with the staged lists. */
export type FolderScope =
  /** Skipped by discovery (default, .gitignore, …): shown disabled with its reason. */
  | { readonly kind: "fixed"; readonly reason: string }
  /** Graphed; mixed when something inside it is excluded. */
  | { readonly kind: "included"; readonly mixed: boolean }
  /** Excluded; `locked` names the pattern above it when unchecking that pattern is what it takes. */
  | { readonly kind: "excluded"; readonly locked: string | null }
  /** Outside the `include` list; mixed when something inside it is included. */
  | { readonly kind: "not_included"; readonly mixed: boolean };

/** Where `folder` stands: `saved` are the lists `scope_dir` reflects, `draft` the staged ones. */
export function folderScope(folder: Pick<ScopeFolder, "path" | "skipped">, saved: ScopeLists, draft: ScopeLists): FolderScope {
  const { path, skipped } = folder;
  if (skipped !== null && skippedByDiscovery(folder)) {
    return { kind: "fixed", reason: SKIP_REASONS[skipped] };
  }
  const exact = escapeGlob(path);
  // A pattern the matcher can't read counts only where Rust found the folder excluded.
  const mayCover = (pattern: string) => {
    const covered = covers(pattern, path);
    return covered === true || (covered === null && skipped === "excluded");
  };
  // Patterns that exclude it now, and the saved ones Rust may have excluded it by.
  const staged = draft.exclude.filter(mayCover);
  const savedCandidates = skipped === "excluded" ? saved.exclude.filter(mayCover) : [];
  // Rust says excluded but no saved pattern looks like the reason: trust Rust.
  const unexplained = skipped === "excluded" && savedCandidates.length === 0;
  if (staged.length > 0 || unexplained) {
    const other = staged.find((pattern) => pattern !== exact);
    return { kind: "excluded", locked: other ?? (staged.includes(exact) ? null : "a pattern in the file") };
  }
  const included =
    draft.include.length === 0 ||
    draft.include.some((pattern) => covers(pattern, path) === true) ||
    (draft.include.some((pattern) => covers(pattern, path) === null) && skipped !== "not_included");
  if (!included) {
    return { kind: "not_included", mixed: draft.include.some((pattern) => liesUnder(pattern, path)) };
  }
  return { kind: "included", mixed: draft.exclude.some((pattern) => liesUnder(pattern, path)) };
}

/** The check box a folder shows. */
export function checkOf(scope: FolderScope): boolean | "mixed" {
  switch (scope.kind) {
    case "included":
      return scope.mixed ? "mixed" : true;
    case "not_included":
      return scope.mixed ? "mixed" : false;
    default:
      return false;
  }
}

/** Whether the folder's box can change (not skipped by discovery, not excluded from above). */
export function canToggle(scope: FolderScope): boolean {
  return scope.kind === "included" || scope.kind === "not_included" || (scope.kind === "excluded" && scope.locked === null);
}

function without(list: readonly string[], pattern: string): string[] {
  return list.filter((item) => item !== pattern);
}

function adding(list: readonly string[], pattern: string): string[] {
  return list.includes(pattern) ? [...list] : [...list, pattern];
}

/**
 * The staged lists after checking or unchecking a folder, mirroring what Rust's `exclude_from_graph`
 * and `include_in_graph` do with the file: unchecking drops the folder's own `include` pattern when
 * other ones remain, else excludes it; checking drops its own `exclude` pattern, then includes it when
 * an `include` list leaves it out.
 */
export function toggleFolder(folder: Pick<ScopeFolder, "path" | "skipped">, saved: ScopeLists, draft: ScopeLists): ScopeLists {
  const scope = folderScope(folder, saved, draft);
  if (!canToggle(scope)) {
    return draft;
  }
  const exact = escapeGlob(folder.path);
  if (scope.kind === "included") {
    return draft.include.includes(exact) && draft.include.length > 1
      ? { ...draft, include: without(draft.include, exact) }
      : { ...draft, exclude: adding(draft.exclude, exact) };
  }
  const next = { ...draft, exclude: without(draft.exclude, exact) };
  return folderScope(folder, saved, next).kind === "not_included" ? { ...next, include: adding(next.include, exact) } : next;
}

/** One line of the Rules list: a pattern as Rust writes it, and what Save does with it. */
interface RuleRow {
  readonly list: "include" | "exclude";
  readonly pattern: string;
  /** `kept`: saved and staying; `removed`: saved, staged for removal (struck through); `added`: staged. */
  readonly status: "kept" | "removed" | "added";
}

/** The Rules list: saved patterns in file order (removed ones struck through), then the staged ones. */
export function ruleRows(saved: ScopeLists, draft: ScopeLists): RuleRow[] {
  const rows: RuleRow[] = [];
  for (const list of ["exclude", "include"] as const) {
    for (const pattern of saved[list]) {
      rows.push({ list, pattern, status: draft[list].includes(pattern) ? "kept" : "removed" });
    }
    for (const pattern of draft[list]) {
      if (!saved[list].includes(pattern)) {
        rows.push({ list, pattern, status: "added" });
      }
    }
  }
  return rows;
}

/** The staged lists after a Rules row's button: removing a pattern, or putting a removed one back. */
export function toggleRule(draft: ScopeLists, row: RuleRow): ScopeLists {
  const list = draft[row.list];
  return { ...draft, [row.list]: row.status === "removed" ? adding(list, row.pattern) : without(list, row.pattern) };
}

/** Whether discovery skips the folder (its box is disabled, whatever `[scope]` says). */
export function skippedByDiscovery(folder: Pick<ScopeFolder, "skipped">): boolean {
  return folder.skipped !== null && FIXED.has(folder.skipped);
}

/** Folders that can be checked come first, then the ones discovery skips (as drawn). */
export function treeOrder<T extends Pick<ScopeFolder, "skipped">>(folders: readonly T[]): T[] {
  return [...folders.filter((folder) => !skippedByDiscovery(folder)), ...folders.filter(skippedByDiscovery)];
}
