/**
 * A source tab (architecture §11.4): one whole file, read-only, virtualized by line and fetched in
 * `file_view` pages. Syntax colors, folds, and calls as links (hover peeks, click opens the
 * target's card) when the text is the graph's; a `⊕` beside each definition adds it to the board
 * and `●` marks what's there. Text that changed since the graph is labeled and has no links.
 */

import { useQueries } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronRight, CirclePlus, MoreHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Chip, DotMark, EmptyState, HoverCard, MenuButton } from "../../components";
import type { CallLink, CodeLine, FileView, OutlineEntry, OutlineItem } from "../../ipc/generated";
import { engineQueryOptions, useEngineQuery } from "../../ipc/queries";
import { usePlatform } from "../../keyboard";
import { openBlockCard, useActiveBoardKeys } from "../../state/boards/api";
import type { SourceTab as SourceTabModel } from "../../state/tabs";
import { openInEditor, revealInFileManager, revealLabel } from "../../views/actions";
import { PeekContent } from "../../code/CallLink";
import { foldsStartingAt, lineSegments, pageOf, pageRange, PAGE_LINES, visibleLines, type FoldedRange } from "./model";
// The syntax palette (`.tok-*`) is the Code view's.
import "../../code/code.css";
import "./SourceTab.css";

const ROW_HEIGHT = 20;

interface LineData {
  readonly line: CodeLine;
  readonly page: FileView;
}

/** Every definition of an outline, nested ones included. */
function flattenOutline(items: readonly OutlineEntry[]): OutlineEntry[] {
  const out: OutlineEntry[] = [];
  const stack = [...items].reverse();
  while (stack.length > 0) {
    const item = stack.pop();
    if (!item) {
      break;
    }
    out.push(item);
    for (let index = item.children.length - 1; index >= 0; index -= 1) {
      const child = item.children[index];
      if (child) {
        stack.push(child);
      }
    }
  }
  return out;
}

function GraphLabel({ view }: { readonly view: FileView }) {
  if (view.graph === "current") {
    return <span className="source-graph source-graph-current">Graph current</span>;
  }
  if (view.graph === "not_compiled") {
    return (
      <Chip tone="warn" title="This file changed since the graph was built. Links appear after the next update.">
        Not yet compiled
      </Chip>
    );
  }
  return <span className="source-graph">Not indexed</span>;
}

function LinkRun({ link, children }: { readonly link: CallLink; readonly children: ReactNode }) {
  const target = link.resolution === "resolved" ? link.target : undefined;
  if (target === undefined) {
    const note = link.note ?? (link.resolution === "external" ? "external" : "unresolved");
    return (
      <span className={`source-link source-link-${link.resolution}`} title={`${link.text} · ${note}`}>
        {children}
      </span>
    );
  }
  return (
    <HoverCard content={<PeekContent targetKey={target} hint="Click to open its card" />}>
      <span
        role="link"
        tabIndex={-1}
        className="source-link source-link-resolved"
        onClick={(event) => {
          event.stopPropagation();
          openBlockCard(target);
        }}
      >
        {children}
      </span>
    </HoverCard>
  );
}

function LineText({ data, links }: { readonly data: LineData; readonly links: readonly CallLink[] }) {
  const segments = lineSegments(data.page, data.line, links);
  const out: ReactNode[] = [];
  for (let index = 0; index < segments.length; ) {
    const segment = segments[index];
    if (!segment) {
      break;
    }
    // Consecutive runs of one link become one interactive element.
    let last = index;
    while (segment.link && segments[last + 1]?.link === segment.link) {
      last += 1;
    }
    const runs = segments.slice(index, last + 1).map((run) => (
      <span key={run.start} className={run.token ? `tok-${run.token}` : undefined}>
        {run.text}
      </span>
    ));
    out.push(
      segment.link ? (
        <LinkRun key={`l${segment.start}`} link={segment.link}>
          {runs}
        </LinkRun>
      ) : (
        runs
      ),
    );
    index = last + 1;
  }
  return <>{out}</>;
}

export function SourceTab({ tab, active }: { readonly tab: SourceTabModel; readonly active: boolean }) {
  const { path } = tab;
  const platform = usePlatform();
  const onBoard = useActiveBoardKeys();
  const scroller = useRef<HTMLDivElement>(null);
  const [folded, setFolded] = useState<ReadonlyMap<string, FoldedRange>>(new Map());
  const [flash, setFlash] = useState<number | null>(null);

  const first = useEngineQuery("file_view", { path, range: { startLine: 1, endLine: PAGE_LINES } });
  const lineCount = first.data?.lineCount ?? 0;
  const linked = first.data?.graph === "current";
  const outline = useEngineQuery("file_outline", { path }, { enabled: linked });

  const rows = useMemo(() => visibleLines(lineCount, [...folded.values()]), [lineCount, folded]);

  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual returns fresh functions by design.
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 30,
  });
  const items = virtualizer.getVirtualItems();

  // The pages the visible rows need (page 0 always comes from the first query).
  const pages = useMemo(() => {
    const wanted = new Set<number>();
    for (const item of items) {
      const line = rows[item.index];
      if (line !== undefined && pageOf(line) > 0) {
        wanted.add(pageOf(line));
      }
    }
    return [...wanted].sort((a, b) => a - b);
  }, [items, rows]);
  const pageResults = useQueries({
    queries: pages.map((page) => engineQueryOptions("file_view", { path, range: pageRange(page, lineCount) })),
  });

  const pageByIndex = useMemo(() => {
    const map = new Map<number, FileView>();
    if (first.data) {
      map.set(0, first.data);
    }
    pages.forEach((page, index) => {
      const data = pageResults[index]?.data;
      if (data) {
        map.set(page, data);
      }
    });
    return map;
  }, [first.data, pages, pageResults]);

  const lineData = (line: number): LineData | null => {
    const page = pageByIndex.get(pageOf(line));
    const entry = page?.lines[line - page.startLine];
    return page && entry?.number === line ? { line: entry, page } : null;
  };

  const definitionsByLine = useMemo(() => {
    const map = new Map<number, OutlineEntry[]>();
    for (const def of flattenOutline(outline.data?.items ?? [])) {
      map.set(def.startLine, [...(map.get(def.startLine) ?? []), def]);
    }
    return map;
  }, [outline.data]);

  // Reveal a requested line: unfold around it, scroll it to the middle, and flash it.
  useEffect(() => {
    if (tab.line === null || lineCount === 0) {
      return;
    }
    const target = Math.min(tab.line, lineCount);
    setFolded((current) => {
      const next = new Map([...current].filter(([, range]) => !(target > range.startLine && target <= range.endLine)));
      return next.size === current.size ? current : next;
    });
    setFlash(target);
    const timer = setTimeout(() => {
      setFlash(null);
    }, 1200);
    return () => {
      clearTimeout(timer);
    };
  }, [tab.line, tab.reveal, lineCount]);

  useEffect(() => {
    if (flash === null) {
      return;
    }
    const index = rows.indexOf(flash);
    if (index >= 0) {
      virtualizer.scrollToIndex(index, { align: "center" });
    }
    // Scroll once per reveal, after the folds around the line have opened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flash, rows]);

  const toggleFold = (item: OutlineItem) => {
    setFolded((current) => {
      const next = new Map(current);
      if (next.has(item.key)) {
        next.delete(item.key);
      } else {
        next.set(item.key, { startLine: item.startLine, endLine: item.endLine });
      }
      return next;
    });
  };

  const menuEntries = [
    { id: "editor", label: "Open in editor", onSelect: () => void openInEditor(path, tab.line) },
    { id: "reveal", label: revealLabel(platform), onSelect: () => void revealInFileManager(path) },
  ];

  const anyOnBoard = [...definitionsByLine.values()].some((defs) => defs.some((def) => onBoard.has(def.key)));

  let body: ReactNode;
  if (first.error) {
    body = <EmptyState title="Can't show this file" hint={first.error.message} />;
  } else if (!first.data) {
    body = null;
  } else if (first.data.unshowable) {
    body = (
      <EmptyState
        title={first.data.unshowable === "binary" ? "A binary file" : "Too large to show"}
        hint="Open it in your editor instead."
      />
    );
  } else {
    body = (
      <div ref={scroller} className="source-scroller" role="document" aria-label={`${path}, read-only`} tabIndex={0}>
        <div className="source-spacer" style={{ height: virtualizer.getTotalSize() }}>
          {items.map((item) => {
            const number = rows[item.index];
            if (number === undefined) {
              return null;
            }
            const data = lineData(number);
            const folds = data && linked ? foldsStartingAt(data.page.outline, number) : [];
            const fold = folds[0];
            const isFolded = fold !== undefined && folded.has(fold.key);
            const defs = linked ? (definitionsByLine.get(number) ?? []) : [];
            const def = defs[0];
            return (
              <div
                key={item.key}
                className={`source-row${flash === number ? " source-row-flash" : ""}`}
                data-line={number}
                style={{ height: ROW_HEIGHT, transform: `translateY(${item.start}px)` }}
              >
                <span className="source-add">
                  {def ? (
                    onBoard.has(def.key) ? (
                      <DotMark tone="accent" label={`${def.name} is on the board`} />
                    ) : (
                      <button
                        type="button"
                        className="source-add-button"
                        aria-label={`Add ${def.name} to the board`}
                        title={`Add ${def.qualifiedName} to the board`}
                        onClick={() => {
                          openBlockCard(def.key);
                        }}
                      >
                        <CirclePlus size={12} aria-hidden="true" />
                      </button>
                    )
                  ) : null}
                </span>
                <span className="source-number">{number}</span>
                <span className="source-fold">
                  {fold ? (
                    <button
                      type="button"
                      className="source-fold-button"
                      aria-label={`${isFolded ? "Unfold" : "Fold"} lines ${fold.startLine}–${fold.endLine}`}
                      aria-expanded={!isFolded}
                      onClick={() => {
                        toggleFold(fold);
                      }}
                    >
                      {isFolded ? <ChevronRight size={12} aria-hidden="true" /> : <ChevronDown size={12} aria-hidden="true" />}
                    </button>
                  ) : null}
                </span>
                <span className="source-text">
                  {data ? <LineText data={data} links={linked ? data.page.links : []} /> : null}
                  {isFolded && fold.fold ? (
                    <button
                      type="button"
                      className="source-fold-summary"
                      onClick={() => {
                        toggleFold(fold);
                      }}
                    >
                      {fold.fold.summary}
                    </button>
                  ) : null}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div className="source-tab" hidden={!active}>
      <header className="source-header">
        <span className="source-path" title={path}>
          {path}
        </span>
        <span className="source-header-spacer" />
        {anyOnBoard ? (
          <span className="source-on-board">
            <DotMark tone="accent" label="On the board" /> on board
          </span>
        ) : null}
        {first.data ? <GraphLabel view={first.data} /> : null}
        <MenuButton variant="icon" icon={MoreHorizontal} label="File actions" size="small" align="end" entries={menuEntries} />
      </header>
      {body}
    </div>
  );
}
