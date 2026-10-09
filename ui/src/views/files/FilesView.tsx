/**
 * The Files view (architecture §11.3): the folder as it is on disk, one level at a time from
 * `list_dir`, with quiet marks; an indexed file expands into its outline (`file_outline`). Clicking
 * a definition opens its card, double-clicking a file opens a source tab.
 */

import { useQueries, type UseQueryResult } from "@tanstack/react-query";
import { Eye, EyeOff, File, FileCode, Folder, FolderOpen, ListCollapse } from "lucide-react";
import { useCallback, useMemo } from "react";
import { IconButton, TreeView, useContextMenu, type MenuEntry, type TreeChildren, type TreeRowContent } from "../../components";
import type { DirListing, FileOutline } from "../../ipc/generated";
import { engineQueryOptions } from "../../ipc/queries";
import { usePlatform, type Platform } from "../../keyboard";
import { openBlockCard, useActiveBoardKeys } from "../../state/boards/api";
import { addAsEntranceItem } from "../../settings";
import { openSource } from "../../state/tabs";
import { folderName, useWindow } from "../../state/window";
import { editScope, openInEditor, revealInFileManager, revealLabel, showCallers } from "../actions";
import { KindGlyph } from "../glyphs";
import { ViewHeader } from "../ViewHeader";
import "../views.css";
import { entryMarks, defMarks } from "./marks";
import {
  IGNORED,
  defNode,
  dirId,
  entryNode,
  expandedTargets,
  isExpandableNode,
  isIndexedFile,
  rootNode,
  useFilesView,
  type FileNode,
} from "./model";

type Loaded<T> = { readonly data: T } | { readonly error: string } | null;

function combineResults<T>(results: UseQueryResult<T>[]): Loaded<T>[] {
  return results.map((result) =>
    result.data !== undefined ? { data: result.data } : result.error ? { error: result.error.message } : null,
  );
}

function toChildren<T>(loaded: Loaded<T> | undefined, map: (data: T) => FileNode[]): TreeChildren<FileNode> {
  if (!loaded) {
    return { status: "loading" };
  }
  return "error" in loaded ? { status: "error", message: loaded.error } : map(loaded.data);
}

/** One menu item; `run` may return a promise (its failure is reported where it is made). */
function item(id: string, label: string, run: () => unknown, disabled = false): MenuEntry {
  return {
    id,
    label,
    disabled,
    onSelect: () => {
      void run();
    },
  };
}

function contextEntries(node: FileNode, platform: Platform): MenuEntry[] {
  if (node.kind === "note") {
    return [];
  }
  const reveal = (path: string) => item("reveal", revealLabel(platform), () => revealInFileManager(path));
  if (node.kind === "def") {
    const { def, path } = node;
    return [
      item("card", "Open card", () => {
        openBlockCard(def.key);
      }),
      item("source", "Open source", () => {
        openSource(path, def.startLine);
      }),
      item("editor", "Open in editor", () => openInEditor(path, def.startLine)),
      item("callers", "Show callers", () => {
        showCallers(def.key);
      }),
      ...addAsEntranceItem({ path, kind: def.kind, name: def.name, qualifiedName: def.qualifiedName, line: def.startLine }),
      { kind: "separator", id: "s1" },
      reveal(path),
    ];
  }
  const { entry } = node;
  // The open folder itself and `.unfold/` can't be scoped out.
  const fixed = entry.unfold || entry.path === "";
  const scope =
    entry.skipped === null
      ? item("exclude", "Exclude from graph", () => editScope("exclude_from_graph", entry.path), fixed)
      : item("include", "Include in graph", () => editScope("include_in_graph", entry.path));
  if (entry.kind === "folder") {
    return [
      scope,
      item("only", "Graph only this folder", () => editScope("graph_only", entry.path), fixed),
      { kind: "separator", id: "s1" },
      reveal(entry.path),
    ];
  }
  return [
    // A file's own block is its root, keyed `path#`.
    item(
      "card",
      "Open card",
      () => {
        openBlockCard(`${entry.path}#`);
      },
      !isIndexedFile(entry),
    ),
    item("source", "Open source", () => {
      openSource(entry.path);
    }),
    item("editor", "Open in editor", () => openInEditor(entry.path)),
    { kind: "separator", id: "s1" },
    scope,
    { kind: "separator", id: "s2" },
    reveal(entry.path),
  ];
}

export function FilesView() {
  const folder = useWindow((state) => state.info?.folder ?? null);
  const expanded = useFilesView((state) => state.expanded);
  const showIgnored = useFilesView((state) => state.showIgnored);
  const selectedId = useFilesView((state) => state.selectedId);
  const onBoard = useActiveBoardKeys();
  const platform = usePlatform();
  const menu = useContextMenu("File actions");

  const { dirs, files } = useMemo(() => expandedTargets(expanded), [expanded]);
  const listings = useQueries({
    queries: dirs.map((path) => engineQueryOptions("list_dir", { path })),
    combine: combineResults<DirListing>,
  });
  const outlines = useQueries({
    queries: files.map((path) => engineQueryOptions("file_outline", { path })),
    combine: combineResults<FileOutline>,
  });

  const children = useMemo(() => {
    const map = new Map<string, TreeChildren<FileNode>>();
    dirs.forEach((path, index) => {
      map.set(
        dirId(path),
        toChildren(listings[index], (listing) => {
          const nodes = listing.entries
            .filter((entry) => showIgnored || entry.skipped === null || !IGNORED.has(entry.skipped))
            .map(entryNode);
          if (listing.truncated) {
            nodes.push({ kind: "note", id: `note:${path}`, text: "More entries than the tree shows" });
          }
          return nodes;
        }),
      );
    });
    files.forEach((path, index) => {
      map.set(
        `file:${path}`,
        toChildren(outlines[index], (outline) => outline.items.map((def) => defNode(def, path))),
      );
    });
    return map;
  }, [dirs, files, listings, outlines, showIgnored]);

  const getChildren = useCallback(
    (node: FileNode): TreeChildren<FileNode> | undefined =>
      node.kind === "def" ? node.def.children.map((def) => defNode(def, node.path)) : children.get(node.id),
    [children],
  );

  const roots = useMemo(() => [rootNode(folder === null ? "Folder" : folderName(folder))], [folder]);

  const renderRow = (node: FileNode, state: { expanded: boolean }): TreeRowContent => {
    if (node.kind === "note") {
      return { label: node.text, dimmed: true };
    }
    if (node.kind === "def") {
      return {
        label: node.def.name,
        icon: <KindGlyph kind={node.def.kind} />,
        marks: defMarks(node.def, onBoard),
        title: `${node.def.qualifiedName} · line ${node.def.startLine}`,
      };
    }
    const { entry } = node;
    const isFolder = entry.kind === "folder";
    const FolderIcon = state.expanded ? FolderOpen : Folder;
    const Icon = isFolder ? FolderIcon : entry.language ? FileCode : File;
    return {
      label: entry.name,
      icon: <Icon size={14} />,
      ...(entry.unfold ? { description: "shared settings" } : {}),
      marks: entryMarks(entry, { onBoard, expanded: state.expanded }),
      dimmed: entry.skipped !== null && IGNORED.has(entry.skipped),
      title: entry.path === "" ? (folder ?? entry.name) : entry.path,
    };
  };

  return (
    <section className="view" aria-labelledby="files-title">
      <ViewHeader title="Files" id="files-title">
        <IconButton
          icon={showIgnored ? Eye : EyeOff}
          label={showIgnored ? "Hide ignored files" : "Show ignored files"}
          size="small"
          pressed={!showIgnored}
          onClick={() => {
            useFilesView.setState({ showIgnored: !showIgnored });
          }}
        />
        <IconButton
          icon={ListCollapse}
          label="Collapse all"
          size="small"
          onClick={() => {
            useFilesView.setState({ expanded: new Set([dirId("")]) });
          }}
        />
      </ViewHeader>
      <TreeView<FileNode>
        label="Files"
        className="view-tree"
        roots={roots}
        getId={(node) => node.id}
        getText={(node) => (node.kind === "entry" ? node.entry.name : node.kind === "def" ? node.def.name : node.text)}
        isExpandable={isExpandableNode}
        getChildren={getChildren}
        renderRow={renderRow}
        expanded={expanded}
        onExpandedChange={(next) => {
          useFilesView.setState({ expanded: next });
        }}
        selectedId={selectedId}
        onSelect={(node) => {
          useFilesView.setState({ selectedId: node.id });
          if (node.kind === "def") {
            openBlockCard(node.def.key);
          }
        }}
        onOpen={(node) => {
          if (node.kind === "entry" && node.entry.kind === "file") {
            openSource(node.entry.path);
          } else if (node.kind === "def") {
            openBlockCard(node.def.key);
          }
        }}
        onContextMenu={(node, at) => {
          const entries = contextEntries(node, platform);
          if (entries.length > 0) {
            menu.open(at, entries);
          }
        }}
      />
      {menu.element}
    </section>
  );
}
