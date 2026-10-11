/**
 * The source peek sheet: one whole file, read-only, sliding in from the right over the content; pinned,
 * the content narrows instead. Virtualized by line and fetched in `file_view` pages of 200 lines.
 * Syntax colors, folds on definitions, calls as links (hover peeks, click adds the callee), a `⊕`
 * beside each definition to add it to the page (`●` when it's there), the text matches it was opened
 * with (ticks beside the scrollbar), and Open in editor / Reveal in Finder. One sheet per window, kept
 * across tab switches; Escape closes it.
 */

import { useQueries } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type JSX, type ReactNode } from "react";
import { ProjectScope } from "../canvas/ProjectScope";
import { CallLink } from "../code/CallLink";
import { tokenClass } from "../code/CodeText";
import { newCardView } from "../code/typeKinds";
import { groupByLink, linkRanges, segmentRange, spanRanges, type LinkRange, type SpanRange } from "../code/segments";
import { EmptyState, IconButton, KeyChip, MenuButton, Monogram, Sheet, showToast, type MenuEntry } from "../components";
import { Icon } from "../icons";
import { engineQueryOptions, toUnfoldError, useEngineQuery, type CallLink as CallLinkModel, type CodeLine, type FileView, type Key, type OutlineItem, type ProjectId, type TextMatch, type View } from "../ipc";
import { usePlatform, useRegisterActions, type ActionSpec } from "../keyboard";
import { openInEditor, revealInFinder } from "../lib/files";
import { languageList, plural } from "../lib/format";
import { addBlock, cardRefKey, findPage, getOpenBoard, keyDisplayName, newBoard, useOpenBoard, usePageKeys, type BoardRef, type PageTarget } from "../state/boards";
import { setPeekPinned, useLayout } from "../state/layout";
import { closePeek, useAddTarget, usePeekTarget, type AddTarget, type PeekTarget } from "../state/overlays";
import { useProjectRow } from "../state/workspace";
import {
  PAGE_LINES,
  canFold,
  definitionAt,
  markRuns,
  marksOn,
  matchTicks,
  pageOf,
  pageRange,
  visibleLines,
  type FoldedRange,
  type LineMark,
} from "./model";
import "../code/code.css";
import "./peek.css";

const ROW_HEIGHT = 20;
/** The code area's padding above the first row and below the last. */
const PADDING = 8;

/** Adds a definition of `project` where Add would: the active page, or a new draft from Home; in `view` when given. */
async function addDefinition(target: AddTarget, project: ProjectId, key: Key, view: View | undefined): Promise<void> {
  try {
    if (target.kind === "page") {
      addBlock(target.target, project, key, view);
      return;
    }
    const board = await newBoard(project, { title: keyDisplayName(key) });
    const page = getOpenBoard(board)?.activePage;
    if (page !== undefined) {
      addBlock({ board, page }, project, key, view);
    }
  } catch (error) {
    showToast({ title: "Couldn't add it", body: toUnfoldError(error).message, tone: "danger" });
  }
}

/** One page's text model, its spans and links relative to its text (computed once per page). */
interface PageModel {
  readonly view: FileView;
  readonly spans: readonly SpanRange[];
  readonly links: readonly LinkRange[];
}

const pageModels = new WeakMap<FileView, PageModel>();

function modelOf(view: FileView): PageModel {
  let model = pageModels.get(view);
  if (!model) {
    model = { view, spans: spanRanges(view.spans, view.textStart), links: linkRanges(view.links, view.textStart) };
    pageModels.set(view, model);
  }
  return model;
}

interface LineProps {
  readonly page: PageModel;
  readonly line: CodeLine;
  readonly linked: boolean;
  readonly marks: readonly LineMark[];
  /** The folded definition that starts here, if folded. */
  readonly folded: OutlineItem | null;
  readonly onUnfold: () => void;
  readonly onFollow: (link: CallLinkModel) => void;
}

/** One line's colored runs, its call links, its text matches, and a folded definition's pill. */
function LineText({ page, line, linked, marks, folded, onUnfold, onFollow }: LineProps) {
  const { view } = page;
  const lineStart = line.start - view.textStart;
  const fold = folded?.fold;
  const end = fold ? Math.min(line.end, Math.max(line.start, fold.headerEnd)) - view.textStart : line.end - view.textStart;
  const shifted = marks.map((mark) => ({ ...mark, start: mark.start + lineStart, end: mark.end + lineStart }));
  const groups = groupByLink(segmentRange(lineStart, end, page.spans, linked ? page.links : []));
  const out: ReactNode[] = groups.map((group) => {
    const runs = group.segments.flatMap((segment) =>
      markRuns(segment.start, segment.end, shifted).map((run) => (
        <span
          key={run.start}
          className={[tokenClass(segment.token), run.mark === null ? undefined : `peek-match ${run.mark}`].filter(Boolean).join(" ") || undefined}
        >
          {view.text.slice(run.start, run.end)}
        </span>
      )),
    );
    const key = group.segments[0]?.start ?? 0;
    const link = group.link === null ? undefined : view.links[group.link];
    return link ? (
      <CallLink key={`l${key}`} link={link} onFollow={onFollow} hint="Click to add it">
        {runs}
      </CallLink>
    ) : (
      <span key={`r${key}`}>{runs}</span>
    );
  });
  if (fold) {
    out.push(
      <span key="fold">
        {fold.braces ? "{ " : " "}
        <button type="button" className="peek-fold-pill" aria-label={`Unfold ${fold.summary}`} onClick={onUnfold}>
          {fold.summary}
        </button>
        {fold.braces ? " }" : ""}
      </span>,
    );
  }
  return <>{out}</>;
}

/** What the header says of the file's place in the graph. */
const GRAPH_NOTES: Readonly<Record<FileView["graph"], string>> = {
  current: "",
  not_compiled: " · changed since the last update",
  evidence: " · evidence file",
  not_indexed: " · not indexed",
};

function revealLabel(platform: string): string {
  if (platform === "darwin") {
    return "Reveal in Finder";
  }
  return platform === "win32" ? "Show in Explorer" : "Show in Files";
}

function Header({ target, view, pinned, projectName }: { readonly target: PeekTarget; readonly view: FileView | undefined; readonly pinned: boolean; readonly projectName: string }) {
  const platform = usePlatform();
  const entries: MenuEntry[] = [
    { id: "editor", label: "Open in editor", icon: "external-link", shortcut: "Mod+Shift+O", onSelect: () => void openInEditor(target.project, target.path, target.line) },
    { id: "reveal", label: revealLabel(platform), icon: "reveal", onSelect: () => void revealInFinder(target.project, target.path) },
  ];
  const meta = view && !view.unshowable ? `${plural(view.lineCount, "line")}${view.language ? ` · ${languageList([view.language])}` : ""}${GRAPH_NOTES[view.graph]}` : "";
  return (
    <header className="peek-header">
      <Monogram name={projectName} size={20} />
      <span className="peek-path" title={target.path}>
        {target.path}
      </span>
      <span className="peek-meta">{meta}</span>
      <span className="peek-spacer" />
      <div className="peek-actions">
        <IconButton
          icon="pin"
          label={pinned ? "Unpin" : "Pin"}
          pressed={pinned}
          onClick={() => {
            setPeekPinned(!pinned);
          }}
        />
        <MenuButton variant="icon" icon="dots" label="More" menuLabel="File actions" entries={entries} align="end" width={208} />
        <IconButton icon="close" label="Close" shortcut="Escape" onClick={closePeek} />
      </div>
    </header>
  );
}

function Footer({ pageTitle }: { readonly pageTitle: string | null }) {
  return (
    <footer className="peek-footer">
      <Icon name="add-circle" size={14} />
      <span>adds a function to</span>
      <span className="peek-footer-page">{pageTitle ?? "a new draft"}</span>
      <span>·</span>
      <KeyChip variant="outlined" glyph="esc" />
      <span>closes · pin keeps it open</span>
    </footer>
  );
}

/** Stands in for the board while the sheet adds to Home (hooks can't be skipped). */
const NO_BOARD: BoardRef = { project: "", id: "" };

function usePageTitle(page: PageTarget | null): string | null {
  return useOpenBoard(page?.board ?? NO_BOARD, (board) => (page ? findPage(board.document, page.page)?.title : undefined)) ?? null;
}

const NO_MATCHES: readonly TextMatch[] = [];

function Lines({
  target,
  first,
  onPage,
  add,
}: {
  readonly target: PeekTarget;
  readonly first: FileView;
  readonly onPage: ReadonlySet<string>;
  readonly add: (key: Key, view?: View) => void;
}) {
  const { project, path } = target;
  const scroller = useRef<HTMLDivElement | null>(null);
  const [folded, setFolded] = useState<ReadonlyMap<string, FoldedRange>>(new Map());
  const lineCount = first.lineCount;
  const linked = first.graph === "current";
  const matches = target.matches ?? NO_MATCHES;
  const rows = useMemo(() => visibleLines(lineCount, [...folded.values()]), [lineCount, folded]);

  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual returns fresh functions by design.
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    paddingStart: PADDING,
    paddingEnd: PADDING,
    overscan: 30,
  });
  const items = virtualizer.getVirtualItems();

  // The pages the shown rows need; page 0 is `first`.
  const wanted = useMemo(() => {
    const pages = new Set<number>();
    for (const item of items) {
      const line = rows[item.index];
      if (line !== undefined && pageOf(line) > 0) {
        pages.add(pageOf(line));
      }
    }
    return [...pages].sort((a, b) => a - b);
  }, [items, rows]);
  const loaded = useQueries({ queries: wanted.map((page) => engineQueryOptions("file_view", { project, path, range: pageRange(page, lineCount) })) });
  const pages = new Map<number, FileView>([[0, first]]);
  wanted.forEach((page, index) => {
    const data = loaded[index]?.data;
    if (data) {
      pages.set(page, data);
    }
  });

  // Opened at a line: scroll it to the middle once.
  const line = target.line;
  useEffect(() => {
    if (line === undefined) {
      return;
    }
    const index = rows.indexOf(Math.min(line, lineCount));
    if (index >= 0) {
      virtualizer.scrollToIndex(index, { align: "center" });
    }
    // Once per opening (the sheet remounts for each target).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggle = (item: OutlineItem) => {
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

  const ticks = matchTicks(rows, matches, target.current);

  return (
    <div className="peek-code-area">
      <div ref={scroller} className="peek-scroller" role="document" aria-label={`${path}, read-only`} tabIndex={0}>
        <div className="peek-spacer-rows" style={{ height: virtualizer.getTotalSize() }}>
          {items.map((item) => {
            const number = rows[item.index];
            if (number === undefined) {
              return null;
            }
            const view = pages.get(pageOf(number));
            const entry = view?.lines[number - view.startLine];
            const data = view && entry?.number === number ? { page: modelOf(view), line: entry } : null;
            const def = data && linked ? definitionAt(data.page.view.outline, number) : undefined;
            const foldable = def !== undefined && canFold(def);
            const isFolded = foldable && folded.has(def.key);
            const marks = marksOn(number, matches, target.current);
            const current = marks.some((mark) => mark.current);
            const there = def !== undefined && onPage.has(cardRefKey(project, def.key));
            return (
              <div
                key={item.key}
                className={current ? "peek-row current" : "peek-row"}
                data-line={number}
                style={{ height: ROW_HEIGHT, transform: `translateY(${item.start}px)` }}
              >
                <span className="peek-add">
                  {def === undefined ? null : there ? (
                    <span className="peek-on-page" role="img" aria-label={`${keyDisplayName(def.key)} is on the page`} />
                  ) : (
                    <button
                      type="button"
                      className="peek-add-button"
                      aria-label={`Add ${keyDisplayName(def.key)} to the page`}
                      onClick={() => {
                        add(def.key, newCardView(def.kind));
                      }}
                    >
                      <Icon name="add-circle" size={14} />
                    </button>
                  )}
                </span>
                <span className="peek-number">{number}</span>
                <span className="peek-fold">
                  {foldable ? (
                    <button
                      type="button"
                      className={isFolded ? "peek-fold-button folded" : "peek-fold-button"}
                      aria-label={`${isFolded ? "Unfold" : "Fold"} lines ${def.startLine}–${def.endLine}`}
                      aria-expanded={!isFolded}
                      onClick={() => {
                        toggle(def);
                      }}
                    >
                      <Icon name={isFolded ? "chevron-right" : "chevron-down"} size={12} />
                    </button>
                  ) : null}
                </span>
                <span className="peek-text">
                  {data ? (
                    <LineText
                      page={data.page}
                      line={data.line}
                      linked={linked}
                      marks={marks}
                      folded={isFolded ? def : null}
                      onUnfold={() => {
                        if (def) {
                          toggle(def);
                        }
                      }}
                      onFollow={(link) => {
                        if (link.target !== undefined) {
                          add(link.target);
                        }
                      }}
                    />
                  ) : null}
                </span>
              </div>
            );
          })}
        </div>
      </div>
      {ticks.length > 0 ? (
        <div className="peek-ticks" aria-hidden="true">
          {ticks.map((tick) => (
            <span key={tick.line} className={tick.current ? "peek-tick current" : "peek-tick"} style={{ top: `${tick.at * 100}%` }} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

function PeekBody({ target, pinned }: { readonly target: PeekTarget; readonly pinned: boolean }) {
  const addTarget = useAddTarget();
  const page = addTarget.kind === "page" ? addTarget.target : null;
  const pageTitle = usePageTitle(page);
  const onPage = usePageKeys(page);
  const projectName = useProjectRow(target.project)?.name ?? "";
  const first = useEngineQuery("file_view", { project: target.project, path: target.path, range: { startLine: 1, endLine: PAGE_LINES } });
  const view = first.data;

  let body: ReactNode = <div className="peek-code-area" />;
  if (first.error) {
    body = <EmptyState className="peek-empty" title="Can't show this file" body={first.error.message} />;
  } else if (view?.unshowable) {
    body = <EmptyState className="peek-empty" title={view.unshowable === "binary" ? "A binary file" : "Too large to show"} body="Open it in your editor instead." />;
  } else if (view) {
    body = (
      <ProjectScope.Provider value={target.project}>
        <Lines
          target={target}
          first={view}
          onPage={onPage}
          add={(key, view) => void addDefinition(addTarget, target.project, key, view)}
        />
      </ProjectScope.Provider>
    );
  }
  return (
    <>
      <Header target={target} view={view} pinned={pinned} projectName={projectName} />
      {body}
      <Footer pageTitle={pageTitle} />
    </>
  );
}

/** The sheet's own key, so each target (file, line, match) opens fresh. */
function targetKey(target: PeekTarget): string {
  return [target.project, target.path, target.line ?? "", target.current ?? ""].join("\u0000");
}

export function PeekSheet(): JSX.Element | null {
  const target = usePeekTarget();
  const pinned = useLayout((layout) => layout.peek.pinned);
  const specs: readonly ActionSpec[] = [
    {
      id: "peek.close",
      title: "Close Peek",
      category: "View",
      shortcuts: ["Escape"],
      showInAdd: false,
      enabled: () => target !== null,
      run: closePeek,
    },
    {
      id: "peek.openInEditor",
      title: "Open in Editor",
      category: "File",
      icon: "external-link",
      shortcuts: ["Mod+Shift+O"],
      enabled: () => target !== null,
      run: async () => {
        if (target) {
          await openInEditor(target.project, target.path, target.line);
        }
      },
    },
  ];
  useRegisterActions(specs);
  if (!target) {
    return null;
  }
  return (
    <Sheet open label={`Source of ${target.path}`} pinned={pinned} onClose={closePeek} className="peek">
      <PeekBody key={targetKey(target)} target={target} pinned={pinned} />
    </Sheet>
  );
}
