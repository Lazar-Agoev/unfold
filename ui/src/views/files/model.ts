/**
 * The Files view's tree model: folders and files from `list_dir`, and an indexed file's outline
 * from `file_outline`, as one node type with stable ids. Pure, so tests can check it directly.
 */

import { create } from "zustand";
import type { DirEntry, OutlineEntry, SkipKind } from "../../ipc/generated";

export type FileNode =
  | { readonly kind: "entry"; readonly id: string; readonly entry: DirEntry }
  | { readonly kind: "def"; readonly id: string; readonly def: OutlineEntry; readonly path: string }
  | { readonly kind: "note"; readonly id: string; readonly text: string };

export const dirId = (path: string): string => `dir:${path}`;
export const fileId = (path: string): string => `file:${path}`;
export const defId = (key: string): string => `def:${key}`;

/** Ignored by ignore rules or defaults: dimmed, and hidden with the toggle. */
export const IGNORED: ReadonlySet<SkipKind> = new Set<SkipKind>(["ignored", "default"]);
/** Source-like files that aren't indexed ("not indexed"). */
export const NOT_INDEXED: ReadonlySet<SkipKind> = new Set<SkipKind>(["excluded", "not_included", "language_off", "javascript"]);

export const SKIP_REASONS: Readonly<Record<SkipKind, string>> = {
  default: "Skipped by default (dependencies, build output, caches)",
  ignored: "Ignored by an ignore file",
  excluded: "Excluded in .unfold/config.toml",
  not_included: "Outside the folders .unfold/config.toml includes",
  language_off: "Its language is turned off in .unfold/config.toml",
  javascript: "JavaScript isn't graphed",
  symlink: "A link; links aren't followed",
  invalid_path: "The path can't be graphed",
};

export function entryNode(entry: DirEntry): FileNode {
  return { kind: "entry", id: entry.kind === "folder" ? dirId(entry.path) : fileId(entry.path), entry };
}

export function defNode(def: OutlineEntry, path: string): FileNode {
  return { kind: "def", id: defId(def.key), def, path };
}

/** The open folder itself, as the tree's root row. */
export function rootNode(name: string): FileNode {
  return entryNode({
    name,
    path: "",
    kind: "folder",
    language: null,
    state: null,
    pending: false,
    entrances: 0,
    launches: 0,
    skipped: null,
    unfold: false,
  });
}

/** Whether a file is in the graph, so expanding it shows its outline. */
export function isIndexedFile(entry: DirEntry): boolean {
  return entry.kind === "file" && entry.language !== null && entry.state !== null && entry.skipped === null;
}

export function isExpandableNode(node: FileNode): boolean {
  switch (node.kind) {
    case "entry":
      return node.entry.kind === "folder" || isIndexedFile(node.entry);
    case "def":
      return node.def.children.length > 0;
    case "note":
      return false;
  }
}

/** The folders and files whose children an expanded set needs, by kind. */
export function expandedTargets(expanded: ReadonlySet<string>): { dirs: string[]; files: string[] } {
  const dirs: string[] = [];
  const files: string[] = [];
  for (const id of expanded) {
    if (id.startsWith("dir:")) {
      dirs.push(id.slice(4));
    } else if (id.startsWith("file:")) {
      files.push(id.slice(5));
    }
  }
  dirs.sort();
  files.sort();
  return { dirs, files };
}

interface FilesViewState {
  readonly expanded: ReadonlySet<string>;
  readonly showIgnored: boolean;
  readonly selectedId: string | null;
}

/** The view's state, kept while the sidebar shows other views. */
export const useFilesView = create<FilesViewState>()(() => ({
  expanded: new Set([dirId("")]),
  showIgnored: true,
  selectedId: null,
}));
