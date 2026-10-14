/**
 * The code panel (docs/designs/viewing-code.md; core-scope.html's board window): on the right of the
 * board, full height beside the top bar. Its header is the path from the file to the selection (the step
 * showing in bold; a click widens the view to that step, the file to the whole file), Open in editor,
 * and ×. Under it, a banner when the lines can't be trusted, then the lines the reader chose, with their
 * numbers: the selection's lines tinted with an accent edge, and a call's expression filled. Lines render
 * in pages of 200 as they scroll into view.
 *
 * While it is open it follows the page's selection (something with no source leaves it where it is).
 * Hovering a line lights its block on the canvas (`setCodeHover`); clicking one selects that block and
 * leaves the panel where it is. `⊕` beside a definition with no card on the page adds its card. × or
 * Escape closes it.
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type UIEvent } from "react";
import { IconButton, isModalOpen, showToast } from "../components";
import { call, toUnfoldError } from "../ipc";
import type { CardView, CodeLine, CodeState, CodeTarget, CodeView, Key, Page, PathStep } from "../ipc/generated";
import { openInEditor } from "../lib/files";
import { isTyping } from "../lib/typing";
import { addBlock, cardRefKey, findPage, itemsFor, select, useOpenBoard, usePageKeys, usePageView, type PageTarget } from "../state/boards";
import { cardName, usePageCards } from "../state/cards";
import { closeCode, openCode, quietly, setCodeHover, takeQuiet, widenCode, type CodeOpen } from "../state/codePanel";
import { ReviewButton } from "../canvas/cards/ReviewBanner";
import { PAGE_LINES, pageRange } from "./paging";
import { blockSelection, selectionCode } from "./targets";
import { TokenText } from "./TokenText";
import { markedLine, useCodeView } from "./view";
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

/** The path, shortened in the middle (never past the step showing) while it doesn't fit. */
function Path({ steps, onWiden }: { readonly steps: readonly PathStep[]; readonly onWiden: (step: PathStep, index: number) => void }) {
  const row = useRef<HTMLDivElement>(null);
  const [drop, setDrop] = useState(0);
  useLayoutEffect(() => {
    const element = row.current;
    if (!element) {
      return;
    }
    const natural = [...element.children].reduce((width, child) => width + child.scrollWidth, 0);
    const showing = steps.findIndex((step) => step.showing);
    if (natural > element.clientWidth && 1 + drop < steps.length - 1 && !(showing >= 1 && showing <= 1 + drop)) {
      setDrop(drop + 1);
    }
  }, [drop, steps]);
  const shown = steps.map((step, index) => ({ step, index })).filter(({ index }) => index === 0 || index > drop);
  return (
    <div ref={row} className="code-path" title={steps.map((step) => step.label).join(" › ")}>
      {shown.map(({ step, index }, position) => (
        <span key={index} className="code-path-step">
          {position > 0 ? (
            <span className="code-path-sep" aria-hidden="true">
              ›
            </span>
          ) : null}
          {position === 1 && drop > 0 ? (
            <>
              <span className="code-crumb">…</span>
              <span className="code-path-sep" aria-hidden="true">
                ›
              </span>
            </>
          ) : null}
          <button
            type="button"
            className={`code-crumb${index === 0 ? " file" : ""}${step.showing ? " showing" : ""}`}
            aria-current={step.showing ? "true" : undefined}
            onClick={() => {
              onWiden(step, index);
            }}
          >
            {step.label}
          </button>
        </span>
      ))}
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

/** Adds a definition's card to the page; the panel stays where it is. */
async function addDefinition(target: PageTarget, key: Key): Promise<void> {
  try {
    const view = await call("card", { project: target.board.project, key });
    quietly(target, () => {
      addBlock(target, key, cardName(view));
    });
  } catch (error) {
    showToast({ title: "Couldn't add it", body: toUnfoldError(error).message, tone: "danger" });
  }
}

interface LinesProps {
  readonly view: CodeView;
  /** Each definition's first line, for those with no card on the page. */
  readonly adds: ReadonlyMap<number, Key>;
  readonly onAdd: (key: Key) => void;
  readonly onHover: (line: CodeLine | null, call: Key | null) => void;
  readonly onPick: (line: CodeLine, call: Key | null) => void;
}

/** The pages of `count` lines, in order. */
function pagesOf(count: number): number[] {
  return Array.from({ length: Math.ceil(count / PAGE_LINES) }, (_, page) => page);
}

/** The pages of `count` lines within `ROOM` pixels of the view at `top` (`height` tall), as a key. */
function pagesInView(count: number, top: number, height: number): string {
  return pagesOf(count)
    .filter((page) => {
      const { startLine, endLine } = pageRange(page, count);
      return endLine * LINE >= top - ROOM && (startLine - 1) * LINE <= top + height + ROOM;
    })
    .join(",");
}

function Lines({ view, adds, onAdd, onHover, onPick }: LinesProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const { lines } = view;
  const trusted = view.state === "current";
  const mark = trusted ? view.mark : undefined;
  const first = markedLine(view);
  const anchor = first === undefined ? -1 : lines.findIndex((line) => line.number === first);
  const [shown, setShown] = useState(() => pagesInView(lines.length, 0, 0));

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
    setShown(pagesInView(lines.length, top, element.clientHeight));
  }, [view, anchor, lines.length]);

  const onScroll = (event: UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    const next = pagesInView(lines.length, element.scrollTop, element.clientHeight);
    if (next !== shown) {
      setShown(next);
    }
  };

  const lineOf = (event: MouseEvent): { readonly line: CodeLine; readonly call: Key | null } | null => {
    if (!(event.target instanceof Element)) {
      return null;
    }
    const row = event.target.closest<HTMLElement>("[data-line]");
    const line = row ? lines[Number(row.dataset["line"])] : undefined;
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
      {pagesOf(lines.length).map((page) => {
        const { startLine, endLine } = pageRange(page, lines.length);
        if (!visible.has(page)) {
          return <div key={page} style={{ height: (endLine - startLine + 1) * LINE }} />;
        }
        return lines.slice(startLine - 1, endLine).map((line, offset) => {
          const index = startLine - 1 + offset;
          const tinted = mark?.lines !== undefined && line.number >= mark.lines[0] && line.number <= mark.lines[1];
          const marks = (mark?.expression ?? []).filter((range) => range.line === line.number);
          const add = adds.get(line.number);
          return (
            <div key={index} className={`code-line${tinted ? " marked" : ""}`} data-line={index}>
              <span className="code-number">{line.number}</span>
              <span className="code-text">
                <TokenText text={line.text} tokens={line.tokens} calls={line.calls} marks={marks} />
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
      openCode({ project, target: choice.target, cardId: choice.cardId, gap: null });
    }
  }, [selection, page, views, project]);
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
  const onPage = usePageKeys(target);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const views = usePageCards(target);
  useFollowSelection(target, page, views);
  useEscapeCloses();

  const adds = useMemo(
    () => new Map((view?.definitions ?? []).filter((definition) => !onPage.has(cardRefKey(project, definition.key))).map((definition) => [definition.line, definition.key] as const)),
    [view, onPage, project],
  );
  const file = view?.file;
  const editorLine = view ? (markedLine(view) ?? view.lines[0]?.number) : undefined;

  return (
    <aside className="code-panel" aria-label="Code panel">
      <header className="code-panel-header">
        {view ? (
          <Path
            key={view.path.map((step) => step.label).join("\u0000")}
            steps={view.path}
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
          adds={adds}
          onAdd={(key) => void addDefinition(target, key)}
          onHover={(line, callKey) => {
            setCodeHover(line && file !== undefined ? { key: callKey ?? line.block ?? null, path: file, line: line.number } : null);
          }}
          onPick={(line, callKey) => {
            const key = callKey ?? line.block;
            const selection = key !== undefined && page ? blockSelection(page, views, key, open.cardId) : null;
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
