/**
 * The source peek sheet: one whole file, read-only, sliding in from the right over the content; pinned,
 * the content narrows instead. Virtualized by line and fetched in `file_view` pages of 200 lines.
 * Syntax colors, folds on definitions, calls as links (hover peeks, click adds the callee), a `⊕`
 * beside each definition to add it to the page (`●` when it's there), and Open in editor. One sheet
 * per window, kept when the window shows another board; Escape closes it, unless a field, a menu, a
 * dialog, or the canvas (a selected arrow) had it first.
 *
 * Opened by View code, it highlights its focus: the rows tinted with an accent edge, and the exact
 * expression marked when the focus has columns. Only verified source shows it (`showsFocus`); a file
 * changed since the compile, or outside the graph, says the range isn't shown instead. The focus must
 * also still be where View code finds its item on the current card models (`useFocusCurrent`): while
 * they or the text refresh it isn't drawn, and a recompile that moved the item re-targets the sheet.
 */

import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type JSX, type ReactNode } from "react";
import { CallLink } from "../code/CallLink";
import { tokenClass } from "../code/CodeText";
import { newCardView } from "../code/typeKinds";
import { groupByLink, linkRanges, segmentRange, spanRanges, type LinkRange, type SpanRange } from "../code/segments";
import { EmptyState, IconButton, MenuButton, Monogram, Sheet, showToast, type MenuEntry } from "../components";
import { Icon } from "../icons";
import { engineQueryOptions, toUnfoldError, useEngineQuery, type CallLink as CallLinkModel, type CodeLine, type FileView, type Key, type OutlineItem, type ProjectId, type View } from "../ipc";
import { openInEditor } from "../lib/files";
import { isTyping } from "../lib/typing";
import { languageList, plural } from "../lib/format";
import { addBlock, cardRefKey, findPage, getOpenBoard, keyDisplayName, newBoard, useOpenBoard, usePageKeys, type BoardRef, type PageTarget } from "../state/boards";
import { setPeekPinned, useLayout } from "../state/layout";
import { closePeek, openPeek, useAddPage, usePeekTarget, type PeekFocus, type PeekTarget } from "../state/overlays";
import { useProjectRow } from "../state/workspace";
import {
  PAGE_LINES,
  canFold,
  definitionAt,
  focusAnchor,
  focusMarkOn,
  inFocus,
  markRuns,
  pageOf,
  pageRange,
  showsFocus,
  visibleLines,
  type FoldedRange,
  type LineMark,
} from "./model";
import "../code/code.css";
import "./peek.css";

const ROW_HEIGHT = 20;
const NO_MARKS: readonly LineMark[] = [];
/** The code area's padding above the first row and below the last. */
const PADDING = 8;

/**
 * Adds a definition of `project` to the shown board's active page when it is of that project (a page
 * shows one project), else to a new board in `project`; in `view` when given.
 */
async function addDefinition(target: PageTarget | null, project: ProjectId, key: Key, view: View | undefined): Promise<void> {
  try {
    if (target?.board.project === project) {
      addBlock(target, key, view);
      return;
    }
    const board = await newBoard(project, { title: keyDisplayName(key) });
    const page = getOpenBoard(board)?.activePage;
    if (page !== undefined) {
      addBlock({ board, page }, key, view);
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
  readonly project: ProjectId;
  readonly page: PageModel;
  readonly line: CodeLine;
  readonly linked: boolean;
  readonly marks: readonly LineMark[];
  /** The folded definition that starts here, if folded. */
  readonly folded: OutlineItem | null;
  readonly onUnfold: () => void;
  readonly onFollow: (link: CallLinkModel) => void;
}

/** One line's colored runs, its call links, View code's expression, and a folded definition's pill. */
function LineText({ project, page, line, linked, marks, folded, onUnfold, onFollow }: LineProps) {
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
          className={[tokenClass(segment.token), run.marked ? "peek-focus" : undefined].filter(Boolean).join(" ") || undefined}
        >
          {view.text.slice(run.start, run.end)}
        </span>
      )),
    );
    const key = group.segments[0]?.start ?? 0;
    const link = group.link === null ? undefined : view.links[group.link];
    return link ? (
      <CallLink key={`l${key}`} project={project} link={link} onFollow={onFollow} hint="Click to add it">
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

/** Why a file doesn't show View code's range. */
const FOCUS_WITHHELD: Readonly<Record<FileView["graph"], string>> = {
  current: "",
  evidence: "",
  not_compiled: "Changed since the last compile · the range isn't shown",
  not_indexed: "Not in the compiled graph · the range isn't shown",
};

function Header({ target, view, pinned, projectName }: { readonly target: PeekTarget; readonly view: FileView | undefined; readonly pinned: boolean; readonly projectName: string }) {
  const entries: MenuEntry[] = [
    { id: "editor", label: "Open in editor", icon: "external-link", onSelect: () => void openInEditor(target.project, target.path, target.line) },
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
        <IconButton icon="close" label="Close" onClick={closePeek} />
      </div>
    </header>
  );
}

function Footer({ pageTitle }: { readonly pageTitle: string | null }) {
  return (
    <footer className="peek-footer">
      <Icon name="add-circle" size={14} />
      <span>adds a function to</span>
      <span className="peek-footer-page">{pageTitle ?? "a new board"}</span>
      <span>· pin keeps it open</span>
    </footer>
  );
}

/** Stands in for the board while no board is shown (hooks can't be skipped). */
const NO_BOARD: BoardRef = { project: "", id: "" };

function usePageTitle(page: PageTarget | null): string | null {
  return useOpenBoard(page?.board ?? NO_BOARD, (board) => (page ? findPage(board.document, page.page)?.title : undefined)) ?? null;
}

function Lines({
  target,
  first,
  focus,
  onPage,
  add,
}: {
  readonly target: PeekTarget;
  readonly first: FileView;
  /** View code's focus, when the text shown is the source it was recorded for. */
  readonly focus: PeekFocus | undefined;
  readonly onPage: ReadonlySet<string>;
  readonly add: (key: Key, view?: View) => void;
}) {
  const { project, path } = target;
  const scroller = useRef<HTMLDivElement | null>(null);
  const [folded, setFolded] = useState<ReadonlyMap<string, FoldedRange>>(new Map());
  const lineCount = first.lineCount;
  const linked = first.graph === "current";
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

  // Opened at a line (or a focus): scroll it to the middle once.
  const anchor = focus ? focusAnchor(focus) : target.line !== undefined ? { line: target.line, align: "center" as const } : undefined;
  useEffect(() => {
    if (anchor === undefined) {
      return;
    }
    const index = rows.indexOf(Math.min(anchor.line, lineCount));
    if (index >= 0) {
      virtualizer.scrollToIndex(index, { align: anchor.align });
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
            const mark = focus ? focusMarkOn(number, focus, data ? data.line.end - data.line.start : 0) : null;
            const focused = focus !== undefined && inFocus(number, focus);
            const there = def !== undefined && onPage.has(cardRefKey(project, def.key));
            return (
              <div
                key={item.key}
                className={focused ? "peek-row focus" : "peek-row"}
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
                      project={target.project}
                      page={data.page}
                      line={data.line}
                      linked={linked}
                      marks={mark ? [mark] : NO_MARKS}
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
    </div>
  );
}

/**
 * Whether View code's focus is still where its item is: `target.resolve` asked again as card models
 * change. While they refresh (or the item has no source now) it isn't; when a recompile moved the
 * item, the sheet re-targets to where it is now.
 */
function useFocusCurrent(target: PeekTarget): boolean {
  const client = useQueryClient();
  const { resolve, follow } = target;
  const subscribe = useCallback((notify: () => void) => client.getQueryCache().subscribe(notify), [client]);
  const agrees = useSyncExternalStore(
    subscribe,
    (): "same" | "moved" | "none" => {
      const now = resolve ? resolve() : target;
      if (now === null) {
        return "none";
      }
      return targetKey(now) === targetKey(target) ? "same" : "moved";
    },
  );
  useEffect(() => {
    const now = agrees === "moved" ? resolve?.() : null;
    if (now) {
      openPeek({ ...now, ...(follow ? { follow } : {}), ...(resolve ? { resolve } : {}) });
    }
  }, [agrees, resolve, follow]);
  return agrees === "same";
}

function PeekBody({ target, pinned }: { readonly target: PeekTarget; readonly pinned: boolean }) {
  const page = useAddPage();
  const pageTitle = usePageTitle(page);
  const onPage = usePageKeys(page);
  const projectName = useProjectRow(target.project)?.name ?? "";
  const first = useEngineQuery("file_view", { project: target.project, path: target.path, range: { startLine: 1, endLine: PAGE_LINES } });
  const view = first.data;
  const current = useFocusCurrent(target);
  // View code's focus, on verified source that is done refreshing and still where its item is.
  const focus = target.focus !== undefined && view !== undefined && showsFocus(view.graph) && !first.isFetching && current ? target.focus : undefined;
  // A focus the file can't show (changed since the compile, or not in the graph) is said, never drawn.
  const withheld = target.focus !== undefined && view !== undefined && !view.unshowable && !showsFocus(view.graph) ? FOCUS_WITHHELD[view.graph] : null;

  let body: ReactNode = <div className="peek-code-area" />;
  if (first.error) {
    body = <EmptyState className="peek-empty" title="Can't show this file" body={first.error.message} />;
  } else if (view?.unshowable) {
    body = <EmptyState className="peek-empty" title={view.unshowable === "binary" ? "A binary file" : "Too large to show"} body="Open it in your editor instead." />;
  } else if (view) {
    body = (
      <Lines
        target={target}
        first={view}
        focus={focus}
        onPage={onPage}
        add={(key, view) => void addDefinition(page, target.project, key, view)}
      />
    );
  }
  return (
    <>
      <Header target={target} view={view} pinned={pinned} projectName={projectName} />
      {withheld !== null ? (
        <p className="peek-note" role="note">
          <Icon name="warning-triangle" size={12} />
          {withheld}
        </p>
      ) : null}
      {body}
      <Footer pageTitle={pageTitle} />
    </>
  );
}

/** The sheet's own key, so each target (file, line, focus) opens fresh. */
function targetKey(target: PeekTarget): string {
  const { focus } = target;
  const range = focus ? [focus.startLine, focus.startColumn ?? "", focus.endLine, focus.endColumn ?? ""].join(":") : "";
  return [target.project, target.path, target.line ?? "", range].join("\u0000");
}

/** While the sheet is open, Escape closes it wherever focus is, unless something before it took the key. */
function useEscapeCloses(open: boolean): void {
  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented && !isTyping(event.target)) {
        closePeek();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
}

export function PeekSheet(): JSX.Element | null {
  const target = usePeekTarget();
  const pinned = useLayout((layout) => layout.peek.pinned);
  useEscapeCloses(target !== null);
  if (!target) {
    return null;
  }
  return (
    <Sheet open label={`Source of ${target.path}`} pinned={pinned} onClose={closePeek} className="peek">
      <PeekBody key={targetKey(target)} target={target} pinned={pinned} />
    </Sheet>
  );
}
