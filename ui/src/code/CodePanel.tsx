/**
 * The code panel (docs/ui-shell.md#the-code-panel): on the right of the
 * board, full height beside the top bar. Its header is the path from the file to the selection (the step
 * showing in bold; a click widens the view to that step, the file to the whole file), Open in editor,
 * and ×. Under it, a banner when the lines can't be trusted, then the lines the reader chose, with their
 * numbers: the selection's lines tinted with an accent edge, a call's expression filled, and a line the
 * engine cut ending in `…`. Lines are asked for and rendered in pages of 200 as they scroll into view.
 *
 * While it is open it follows the page's selection (something with no source leaves it where it is),
 * and lines opened from a card's flow follow the card as it is drawn now. Hovering a line lights its
 * block on the canvas (`setCodeHover`): a call's when on its name, falling back to the line's block when
 * the call isn't drawn on its own. Clicking one selects that block and leaves the panel where it is.
 * `⊕` in the gutter beside a definition's first line, when it has no card on the page, adds its card
 * without moving the canvas. × or Escape closes it.
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type UIEvent } from "react";
import { IconButton, isModalOpen, showToast } from "../components";
import { call, toUnfoldError } from "../ipc";
import type { CardView, CodeLine, CodeState, CodeTarget, CodeView, Key, Page, PathStep } from "../ipc/generated";
import { openInEditor } from "../lib/files";
import { isTyping } from "../lib/typing";
import { addBlockWithoutFocus, cardRefKey, findPage, itemsFor, select, useOpenBoard, usePageKeys, usePageView, type PageTarget } from "../state/boards";
import { cardName, usePageCards } from "../state/cards";
import { closeCode, openCode, quietly, retargetCode, setCodeHover, takeQuiet, widenCode, type CodeOpen } from "../state/codePanel";
import { ReviewButton } from "../canvas/cards/ReviewBanner";
import { pageLines, pagesOf, type LineSpan } from "./paging";
import { fileForms, letters, shortenPath, type Shortened } from "./path";
import { blockSelection, selectionCode, sourceLines } from "./targets";
import { TokenText } from "./TokenText";
import { markedLine, useCodeLines, useCodeView, type LoadedLines } from "./view";
import "./panel.css";

/** A line's height (panel.css). */
const LINE = 20;
/** How far past the view lines are rendered, in pixels. */
const ROOM = 400;

/** What the banner says when the lines can't be trusted. */
const STATE_TEXT: Readonly<Record<Exclude<CodeState, "current">, string>> = {
  changed: "Changed since the last compile · showing the whole file",
  doesNotParse: "Doesn't parse · showing the whole file",
  notFound: "Not found in the code",
  notAnalyzed: "Not analyzed",
};

/** What a step of the path widens the view to: the file, a block (an app, for an entrance), or the selection's own lines. */
function stepShow(open: CodeOpen, view: CodeView, step: PathStep, index: number): CodeTarget | null {
  if (index === 0 && view.file !== undefined) {
    return { kind: "file", path: view.file };
  }
  const own = (open.target.kind === "block" || open.target.kind === "start") && open.target.key === step.key;
  if (step.key === undefined || own) {
    return null;
  }
  return open.target.kind === "start" ? { kind: "start", key: step.key } : { kind: "block", key: step.key };
}

/**
 * The path, shortened while it doesn't fit (`shortenPath`): in the middle first, then the file's
 * folders, then the definition; the step showing and the selection never go, and the selection's label
 * shortens only past all of that. Hovering it shows the selection's full key (its labels, when the
 * selection is lines). Its steps can't be clicked while it is the previous view's, kept while the next
 * one loads.
 */
function Path({ steps, stale, onWiden }: { readonly steps: readonly PathStep[]; readonly stale: boolean; readonly onWiden: (step: PathStep, index: number) => void }) {
  const row = useRef<HTMLDivElement>(null);
  const forms = useMemo(() => fileForms(steps[0]?.label ?? ""), [steps]);
  // How it is shortened; null until the row is measured whole.
  const [shortened, setShortened] = useState<Shortened | null>(null);
  useLayoutEffect(() => {
    const element = row.current;
    const [file, ...rest] = element ? [...element.children] : [];
    if (!element || !file || shortened !== null) {
      return;
    }
    // Each step at its separator's and crumb's own widths (a crumb that gives way still scrolls its
    // whole label). The code font is monospaced: each of the file's shorter forms is a letter narrower
    // for each letter it drops, and `…`, drawn as a separator is, makes a `› …` two separators wide.
    const natural = (step: Element) => [...step.children].reduce((width, part) => width + part.scrollWidth, 0);
    const whole = natural(file);
    const length = letters(steps[0]?.label ?? "");
    const style = getComputedStyle(file.firstElementChild ?? file);
    const padding = (Number.parseFloat(style.paddingLeft) || 0) + (Number.parseFloat(style.paddingRight) || 0);
    const letter = (whole - padding) / Math.max(1, length);
    const separator = element.querySelector(".code-path-sep")?.scrollWidth ?? 0;
    const widths = { file: forms.map((form) => whole - (length - letters(form)) * letter), steps: rest.map(natural), gap: 2 * separator };
    setShortened(shortenPath(widths, steps.findIndex((step) => step.showing), element.clientWidth));
  }, [shortened, steps, forms]);
  return (
    <div ref={row} className="code-path" title={steps.at(-1)?.key ?? steps.map((step) => step.label).join(" › ")}>
      {steps.map((step, index) => {
        if (shortened?.out.has(index)) {
          // Each run of steps left out draws one `› …`, where its first step was.
          return shortened.out.has(index - 1) ? null : (
            <span key={index} className="code-path-step">
              <span className="code-path-sep" aria-hidden="true">
                ›
              </span>
              <span className="code-path-sep">…</span>
            </span>
          );
        }
        return (
          <span key={index} className="code-path-step">
            {index > 0 ? (
              <span className="code-path-sep" aria-hidden="true">
                ›
              </span>
            ) : null}
            <button
              type="button"
              className={`code-crumb${index === 0 ? " file" : ""}${step.showing ? " showing" : ""}`}
              aria-current={step.showing ? "true" : undefined}
              disabled={stale}
              onClick={() => {
                onWiden(step, index);
              }}
            >
              {index === 0 ? forms[shortened?.form ?? 0] : step.label}
            </button>
          </span>
        );
      })}
    </div>
  );
}

/** The banner over the lines when they can't be trusted; not found offers the card's repairs. */
function Banner({ target, state, cardId }: { readonly target: PageTarget; readonly state: Exclude<CodeState, "current">; readonly cardId: string | null }) {
  const health = useOpenBoard(target.board, (board) => board.health);
  const items = state === "notFound" && cardId !== null ? itemsFor(health, target.page, { card: cardId }) : [];
  return (
    <div className="code-banner" role="status">
      <span className="code-banner-text">{STATE_TEXT[state]}</span>
      <ReviewButton target={target} project={target.board.project} items={items} />
    </div>
  );
}

/** Adds a definition's card to the page; the panel and the canvas stay where they are. */
async function addDefinition(target: PageTarget, key: Key): Promise<void> {
  try {
    const view = await call("card", { project: target.board.project, key });
    quietly(target, () => {
      addBlockWithoutFocus(target, key, cardName(view));
    });
  } catch (error) {
    showToast({ title: "Couldn't add it", body: toUnfoldError(error).message, tone: "danger" });
  }
}

interface LinesProps {
  readonly view: CodeView;
  /** The view's lines loaded so far. */
  readonly loaded: LoadedLines;
  /** The pages within reach of the scrolled view, as a key (`pagesInView`). */
  readonly shown: string;
  readonly onShown: (pages: string) => void;
  /** The definition `⊕` adds beside each line that starts one with no card on the page. */
  readonly adds: ReadonlyMap<number, Key>;
  readonly onAdd: (key: Key) => void;
  readonly onHover: (line: CodeLine | null, call: Key | null) => void;
  readonly onPick: (line: CodeLine, call: Key | null) => void;
}

/** The pages of the view `span` within `ROOM` pixels of the view at `top` (`height` tall), as a key. */
function pagesInView(span: LineSpan, top: number, height: number): string {
  return pagesOf(span)
    .filter((page) => {
      const [start, end] = pageLines(page, span);
      return (end - span[0] + 1) * LINE >= top - ROOM && (start - span[0]) * LINE <= top + height + ROOM;
    })
    .join(",");
}

function Lines({ view, loaded, shown, onShown, adds, onAdd, onHover, onPick }: LinesProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const { span } = loaded;
  const trusted = view.state === "current";
  const mark = trusted ? view.mark : undefined;
  const first = markedLine(view);
  const anchor = first === undefined ? -1 : first - span[0];

  // A new view starts at its mark, when that is out of view, else at its top; always at its left, with
  // its line numbers showing.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) {
      return;
    }
    const top = anchor > 0 && (anchor + 1) * LINE > element.clientHeight ? Math.max(0, (anchor - 2) * LINE) : 0;
    element.scrollTop = top;
    element.scrollLeft = 0;
    onShown(pagesInView(span, top, element.clientHeight));
  }, [view, anchor, span, onShown]);

  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    const next = pagesInView(span, element.scrollTop, element.clientHeight);
    if (next !== shown) {
      onShown(next);
    }
  };

  const lineOf = (event: MouseEvent): { readonly line: CodeLine; readonly call: Key | null } | null => {
    if (!(event.target instanceof Element)) {
      return null;
    }
    const row = event.target.closest<HTMLElement>("[data-line]");
    const line = row ? loaded.lines.get(Number(row.dataset["line"]))?.line : undefined;
    return line ? { line, call: event.target.closest<HTMLElement>("[data-call]")?.dataset["call"] ?? null } : null;
  };

  const visible = new Set(shown.split(",").filter((page) => page !== "").map(Number));
  return (
    <div
      ref={scroller}
      className="code-lines"
      onScroll={onScroll}
      onMouseOver={(event) => {
        const at = trusted ? lineOf(event) : null;
        onHover(at?.line ?? null, at?.call ?? null);
      }}
      onMouseLeave={() => {
        onHover(null, null);
      }}
      onClick={(event) => {
        const at = trusted ? lineOf(event) : null;
        if (at && (window.getSelection()?.toString() ?? "") === "") {
          onPick(at.line, at.call);
        }
      }}
    >
      {pagesOf(span).map((page) => {
        const [start, end] = pageLines(page, span);
        if (!visible.has(page)) {
          return <div key={page} style={{ height: (end - start + 1) * LINE }} />;
        }
        return Array.from({ length: end - start + 1 }, (_, offset) => start + offset).map((number) => {
          const at = loaded.lines.get(number);
          if (!at) {
            // Its page is still being read.
            return (
              <div key={number} className="code-line" data-line={number}>
                <span className="code-number">{number}</span>
              </div>
            );
          }
          const { line } = at;
          const tinted = mark?.lines !== undefined && line.number >= mark.lines[0] && line.number <= mark.lines[1];
          const marks = trusted ? at.marks : [];
          const add = adds.get(line.number);
          return (
            <div key={number} className={`code-line${tinted ? " marked" : ""}`} data-line={number}>
              <span className="code-number">{line.number}</span>
              {add !== undefined ? (
                <button
                  type="button"
                  className="code-add"
                  aria-label={`Add the definition on line ${line.number} to the page`}
                  title="Add its card"
                  onClick={(event) => {
                    event.stopPropagation();
                    onAdd(add);
                  }}
                >
                  ⊕
                </button>
              ) : null}
              <span className="code-text">
                <TokenText text={line.text} tokens={line.tokens} calls={line.calls} marks={marks} />
                {line.cut ? <span className="code-cut">…</span> : null}
              </span>
            </div>
          );
        });
      })}
    </div>
  );
}

/** Follows the page's selection while the panel is open: what has source moves the panel to it. */
function useFollowSelection(target: PageTarget, page: Page | undefined, views: ReadonlyMap<string, CardView>): void {
  const { project } = target.board;
  const selection = usePageView(target, (view) => view.selection);
  const last = useRef(JSON.stringify(selection));
  useEffect(() => {
    // Whatever was selected quietly before the panel opened was for its opening.
    takeQuiet();
  }, []);
  useEffect(() => {
    const now = JSON.stringify(selection);
    if (now === last.current) {
      return;
    }
    last.current = now;
    if (takeQuiet() || !page) {
      return;
    }
    const choice = selectionCode(selection, page, views);
    if (choice) {
      openCode({ project, target: choice.target, cardId: choice.cardId, ...(choice.from ? { from: choice.from } : {}) });
    }
  }, [selection, page, views, project]);
}

/**
 * Keeps lines opened from a card's flow (`CodeOpen.from`) as the card draws them now; once it no longer
 * draws them, the panel shows their whole file.
 */
function useCurrentLines(open: CodeOpen, views: ReadonlyMap<string, CardView>): void {
  useEffect(() => {
    const view = open.cardId !== null ? views.get(open.cardId) : undefined;
    if (!open.from || !view || (open.target.kind !== "lines" && open.target.kind !== "file")) {
      return;
    }
    retargetCode(sourceLines(open.from, view) ?? { kind: "file", path: open.target.path });
  }, [open, views]);
}

/** × and Escape close the panel (unless a field, a menu, or a dialog took the Escape first). */
function useEscapeCloses(): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented && !isTyping(event.target) && !isModalOpen()) {
        event.preventDefault();
        closeCode();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, []);
}

export function CodePanel({ target, open }: { readonly target: PageTarget; readonly open: CodeOpen }) {
  const { project } = open;
  const query = useCodeView(open);
  const view = query.data;
  const [shown, setShown] = useState("");
  const pages = useMemo(() => shown.split(",").filter((page) => page !== "").map(Number), [shown]);
  const loaded = useCodeLines(open, view, pages, query.isPlaceholderData);
  const onPage = usePageKeys(target);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const views = usePageCards(target);
  useFollowSelection(target, page, views);
  useCurrentLines(open, views);
  useEscapeCloses();

  // `⊕` adds the outermost definition a line starts: the first the reader lists there, in written order.
  const adds = useMemo(() => {
    const outermost = new Map<number, Key>();
    for (const { line, key } of loaded.definitions) {
      if (!outermost.has(line)) {
        outermost.set(line, key);
      }
    }
    return new Map([...outermost].filter(([, key]) => !onPage.has(cardRefKey(project, key))));
  }, [loaded, onPage, project]);
  const file = view?.file;
  const editorLine = view ? (markedLine(view) ?? view.lines[0]?.number) : undefined;

  return (
    <aside className="code-panel" aria-label="Code panel">
      <header className="code-panel-header">
        {view ? (
          <Path
            // A new path, or another step showing, is measured afresh.
            key={JSON.stringify(view.path.map((step) => [step.label, step.showing]))}
            steps={view.path}
            stale={query.isPlaceholderData}
            onWiden={(step, index) => {
              widenCode(stepShow(open, view, step, index));
            }}
          />
        ) : (
          <div className="code-path" />
        )}
        <IconButton
          icon="external-link"
          label="Open in editor"
          size={24}
          disabled={file === undefined}
          onClick={() => {
            if (file !== undefined) {
              void openInEditor(project, file, editorLine);
            }
          }}
        />
        <IconButton icon="close" label="Close" size={24} onClick={closeCode} />
      </header>
      {view && view.state !== "current" ? <Banner target={target} state={view.state} cardId={open.cardId} /> : null}
      {query.isError && !view ? <p className="code-note">{query.error.message}</p> : null}
      {view ? (
        <Lines
          view={view}
          loaded={loaded}
          shown={shown}
          onShown={setShown}
          adds={adds}
          onAdd={(key) => void addDefinition(target, key)}
          onHover={(line, callKey) => {
            setCodeHover(line && file !== undefined ? { key: callKey ?? line.block ?? null, block: line.block ?? null, path: file, line: line.number } : null);
          }}
          onPick={(line, callKey) => {
            const keys = [callKey, line.block].filter((key): key is Key => key !== null && key !== undefined);
            const selection = page ? blockSelection(page, views, keys, open.cardId) : null;
            if (selection) {
              quietly(target, () => {
                select(target, selection);
              });
            }
          }}
        />
      ) : null}
    </aside>
  );
}
