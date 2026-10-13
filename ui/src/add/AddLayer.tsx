/**
 * The Add box: one box over the window. Typing searches every connected project. Focus stays in its
 * input: ↑ ↓ move the selection, Enter adds to the current page, ⌘Enter on a new page, Tab switches the
 * kind, Escape closes.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { newCardView } from "../code/typeKinds";
import { Tooltip } from "../components";
import { Icon } from "../icons";
import { engineQueryOptions, type ProjectId, type ProjectRow } from "../ipc";
import { plural } from "../lib/format";
import { findPage, useOpenBoard, type BoardRef } from "../state/boards";
import { closeAdd, useAddTarget, type AddTarget } from "../state/overlays";
import { refProjectFor, useProjectIndex, useWorkspace } from "../state/workspace";
import { activateChoice, type AddChoice } from "./activate";
import { Preview, type PreviewItem } from "./Preview";
import { optionId, ResultList } from "./ResultList";
import { MoreRow, ResultRow, resultHeight, resultText } from "./Rows";
import {
  KINDS,
  KIND_LABELS,
  TEXT_MIN,
  allSearchArgs,
  flatItems,
  kindCounts,
  kindSearchArgs,
  projectsWithResults,
  resultGroups,
  type AddTab,
  type ResultItem,
} from "./search";
import { useTextSearch } from "./textSearch";
import "./add.css";

/** Debounces: the query and the preview. */
const QUERY_DELAY = 120;
const PREVIEW_DELAY = 150;
/** The box's fixed height. */
const BOX_HEIGHT = 660;

const BLOCKED = "Rename one of the projects in its settings";

type Mode = "empty" | "search";

/** The tabs, in Tab order. */
const TABS: readonly AddTab[] = ["all", ...KINDS];

interface TextOptions {
  readonly caseSensitive: boolean;
  readonly wholeWord: boolean;
  readonly regex: boolean;
}

/** `value`, once it has stopped changing for `delay` ms. */
function useDebounced<T>(value: T, delay: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => {
      setSettled(value);
    }, delay);
    return () => {
      window.clearTimeout(timer);
    };
  }, [value, delay]);
  return settled;
}

/**
 * While Add is open: the app behind is inert, and focus returns to where it was when Add closes. (The
 * kit's dialogs do the same for their own layers.)
 */
function useModalLayer(): void {
  useLayoutEffect(() => {
    const previous = document.activeElement;
    const root = document.getElementById("root");
    const madeInert = root !== null && !root.hasAttribute("inert");
    if (madeInert) {
      root.setAttribute("inert", "");
    }
    return () => {
      if (madeInert) {
        root.removeAttribute("inert");
      }
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus({ preventScroll: true });
      }
    };
  }, []);
}

function modeOf(query: string): Mode {
  return query.trim().length === 0 ? "empty" : "search";
}

/** The choice a result stands for (null for a "more" row). */
function choiceOf(item: ResultItem): AddChoice | null {
  switch (item.type) {
    case "entrance":
      return { kind: "entrance", project: item.hit.project, key: item.hit.key };
    case "function": {
      const view = newCardView(item.hit.kind);
      return { kind: "definition", project: item.hit.project, key: item.hit.key, name: item.hit.name, ...(view ? { view } : {}) };
    }
    case "file":
      return { kind: "file", project: item.hit.project, path: item.hit.path };
    case "text":
      return { kind: "text", project: item.hit.file.project, path: item.hit.file.path, matches: item.hit.file.matches, current: item.hit.index };
    case "more":
      return null;
  }
}

function itemProject(item: ResultItem): ProjectId | null {
  switch (item.type) {
    case "entrance":
    case "function":
    case "file":
      return item.hit.project;
    case "text":
      return item.hit.file.project;
    case "more":
      return null;
  }
}

function OptionToggle({ label, glyph, pressed, onToggle }: { readonly label: string; readonly glyph: string; readonly pressed: boolean; readonly onToggle: () => void }) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={pressed}
        className="add-option"
        onMouseDown={(event) => {
          event.preventDefault();
        }}
        onClick={onToggle}
      >
        {glyph}
      </button>
    </Tooltip>
  );
}

function Tabs({ tabs, active, counts, onChange }: { readonly tabs: readonly AddTab[]; readonly active: AddTab; readonly counts: Readonly<Record<string, number | null>> | null; readonly onChange: (tab: AddTab) => void }) {
  return (
    <div role="tablist" aria-label="Kinds" className="add-tabs">
      {tabs.map((tab) => {
        const count = tab === "all" || counts === null ? null : counts[tab];
        return (
          <button
            key={tab}
            type="button"
            role="tab"
            aria-selected={tab === active}
            tabIndex={-1}
            className="add-tab"
            onMouseDown={(event) => {
              event.preventDefault();
            }}
            onClick={() => {
              onChange(tab);
            }}
          >
            {tab === "all" ? "All" : KIND_LABELS[tab]}
            {count === null || count === undefined ? null : <span className="add-tab-count">{count.toLocaleString("en-US")}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** Stands in for the board while Add adds to Home (hooks can't be skipped). */
const NO_BOARD: BoardRef = { project: "", id: "" };

/** The title of the page Add adds to, if a board page is active. */
function usePageTitle(target: AddTarget): string | null {
  const page = target.kind === "page" ? target.target : null;
  return useOpenBoard(page?.board ?? NO_BOARD, (board) => (page ? findPage(board.document, page.page)?.title : undefined)) ?? null;
}

export function AddLayer() {
  useModalLayer();
  const listId = useId();
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const target = useAddTarget();
  const page = target.kind === "page" ? target.target : null;
  const pageTitle = usePageTitle(target);
  const index = useProjectIndex();
  const workspace = useWorkspace();
  const rows = useMemo<readonly ProjectRow[]>(() => (workspace?.projects ?? []).filter((row) => row.state.kind !== "unavailable"), [workspace]);

  const [query, setQuery] = useState("");
  const [chosenTab, setChosenTab] = useState<AddTab | null>(null);
  const [options, setOptions] = useState<TextOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const mode = modeOf(query);
  const tab: AddTab = chosenTab ?? "all";

  useEffect(() => {
    input.current?.focus();
  }, []);

  /** Why results of `project` can't be added to the page, if they can't. */
  const blocked = (project: ProjectId | null): string | null =>
    page !== null && project !== null && !refProjectFor(index, page.board.project, project).ok ? BLOCKED : null;
  const projectName = (project: string) => index.byId.get(project)?.name ?? "?";

  const settled = useDebounced(query, QUERY_DELAY);
  const searching = mode === "search" && modeOf(settled) === "search";
  const nameQuery = settled.trim();
  const allNames = useQuery({ ...engineQueryOptions("search", allSearchArgs(nameQuery)), enabled: searching, placeholderData: keepPreviousData });
  const kindArgs = kindSearchArgs(nameQuery, tab);
  const kindNames = useQuery({
    ...engineQueryOptions("search", kindArgs ?? allSearchArgs(nameQuery)),
    enabled: searching && kindArgs !== null,
    placeholderData: keepPreviousData,
  });
  const text = useTextSearch(searching && nameQuery.length >= TEXT_MIN ? { query: settled, ...options } : null);
  const textResults = searching && nameQuery.length >= TEXT_MIN ? text : null;
  const names = searching ? allNames.data : undefined;
  const kindData = kindArgs ? kindNames.data : undefined;
  const groups = useMemo(
    () => (mode === "search" ? resultGroups(tab, names, kindData, textResults) : []),
    [mode, tab, names, kindData, textResults],
  );
  const items = useMemo(() => flatItems(groups), [groups]);
  const counts = mode === "search" ? kindCounts(names, textResults) : null;

  // The selection: the chosen row while it is listed, else the first.
  const listIds = items.map((item) => item.id);
  const selected = selectedId !== null && listIds.includes(selectedId) ? selectedId : (listIds[0] ?? null);
  const selectedIndex = selected === null ? -1 : listIds.indexOf(selected);
  const selectedItem = mode === "search" ? (items[selectedIndex] ?? null) : null;
  const previewSubject: PreviewItem | null = selectedItem && selectedItem.type !== "more" ? selectedItem : null;
  const previewItem = useDebounced(previewSubject, PREVIEW_DELAY);

  const close = () => {
    closeAdd();
  };

  const runAfterClose = useRef<(() => void) | null>(null);
  // Declared after the modal layer, so it runs after focus went back: what it opens keeps focus.
  useEffect(
    () => () => {
      const run = runAfterClose.current;
      runAfterClose.current = null;
      run?.();
    },
    [],
  );
  const closeThen = (run: () => void) => {
    runAfterClose.current = run;
    closeAdd();
  };

  const choose = (choice: AddChoice | null, newPage: boolean) => {
    if (choice === null) {
      return;
    }
    if ((choice.kind === "entrance" || choice.kind === "definition") && blocked(choice.project) !== null) {
      return;
    }
    closeThen(() => void activateChoice(choice, target, newPage));
  };

  const activateResult = (item: ResultItem, newPage: boolean) => {
    if (item.type === "more") {
      setChosenTab(item.kind);
      setSelectedId(null);
      return;
    }
    choose(choiceOf(item), newPage);
  };

  const switchTab = (delta: number) => {
    const at = TABS.indexOf(tab);
    const next = TABS[(at + delta + TABS.length) % TABS.length];
    if (next !== undefined) {
      setChosenTab(next);
      setSelectedId(null);
    }
  };

  const moveSelection = (delta: number) => {
    if (listIds.length === 0) {
      return;
    }
    const next = Math.max(0, Math.min(listIds.length - 1, selectedIndex + delta));
    setSelectedId(listIds[next] ?? null);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    const newPage = event.metaKey || event.ctrlKey;
    switch (event.key) {
      case "Escape":
        close();
        break;
      case "Tab":
        switchTab(event.shiftKey ? -1 : 1);
        break;
      case "ArrowDown":
        moveSelection(1);
        break;
      case "ArrowUp":
        moveSelection(-1);
        break;
      case "PageDown":
      case "PageUp":
        moveSelection(event.key === "PageDown" ? 10 : -10);
        break;
      case "Enter":
        if (selectedItem) {
          activateResult(selectedItem, newPage);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  const placeholder = `Find entrances, functions, files and text in ${plural(rows.length, "project")}`;
  const totalResults = counts ? Object.values(counts).reduce<number>((sum, count) => sum + (count ?? 0), 0) : 0;
  const rightText =
    mode === "search"
      ? `${plural(projectsWithResults(names, textResults), "project")} · ${plural(totalResults, "result")}`
      : page
        ? `Adds to “${pageTitle ?? "this page"}”`
        : "Adds to a new draft";

  let body: ReactNode;
  if (mode === "empty") {
    body = (
      <div className="add-body">
        <p className="add-empty">{workspace && rows.length === 0 ? "No projects are connected. Add one from Home." : emptyText(tab, rows.length)}</p>
      </div>
    );
  } else {
    body = (
      <div className="add-body">
        <SearchResults
          listId={listId}
          groups={groups}
          selected={selected}
          projectName={projectName}
          blocked={blocked}
          status={searchStatus(allNames.data === undefined && allNames.isFetching, allNames.error?.message ?? null, text.error, tab, nameQuery)}
          onActivate={(item, newPage) => {
            activateResult(item, newPage);
          }}
        />
        <Preview
          item={previewItem}
          projectName={projectName}
          onPage={page !== null}
          blocked={previewItem ? blocked(itemProject(previewItem)) : null}
          onActivate={(newPage) => {
            if (previewItem) {
              activateResult(previewItem, newPage);
            }
          }}
        />
      </div>
    );
  }

  const activeDescendant = selectedIndex >= 0 ? optionId(listId, selectedIndex) : undefined;

  return createPortal(
    <div
      className="add-scrim"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label="Add"
        className="add-box"
        onKeyDown={(event) => {
          if (event.key === "Escape" && !event.defaultPrevented) {
            event.preventDefault();
            close();
          }
        }}
        style={{ height: BOX_HEIGHT }}
      >
        <div className="add-input-row">
          <Icon name="search" className="add-input-icon" />
          <input
            ref={input}
            className="add-input"
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={activeDescendant}
            aria-label="Find"
            placeholder={placeholder}
            spellCheck={false}
            autoComplete="off"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedId(null);
            }}
            onKeyDown={onKeyDown}
          />
          {mode === "search" && tab === "text" ? (
            <div className="add-options" role="group" aria-label="Text search options">
              <OptionToggle label="Match case" glyph="Aa" pressed={options.caseSensitive} onToggle={() => { setOptions({ ...options, caseSensitive: !options.caseSensitive }); }} />
              <OptionToggle label="Match whole word" glyph="ab" pressed={options.wholeWord} onToggle={() => { setOptions({ ...options, wholeWord: !options.wholeWord }); }} />
              <OptionToggle label="Use regular expression" glyph=".*" pressed={options.regex} onToggle={() => { setOptions({ ...options, regex: !options.regex }); }} />
            </div>
          ) : null}
        </div>
        <div className="add-kinds">
          <Tabs
            tabs={TABS}
            active={tab}
            counts={counts}
            onChange={(next) => {
              setChosenTab(next);
              setSelectedId(null);
            }}
          />
          <span className="add-kinds-spacer" />
          <span className="add-kinds-text">{rightText}</span>
        </div>
        {body}
      </div>
    </div>,
    document.body,
  );
}

/** What the box says before anything is typed. */
function emptyText(tab: AddTab, projects: number): string {
  const what = tab === "all" ? "entrances, functions, files and text" : KIND_LABELS[tab].toLowerCase();
  return `Type to find ${what} in ${plural(projects, "project")}.`;
}

type SearchStatus = { readonly kind: "ok" } | { readonly kind: "message"; readonly text: string };

function searchStatus(loading: boolean, nameError: string | null, textError: string | null, tab: AddTab, query: string): SearchStatus {
  if (tab === "text" && textError !== null) {
    return { kind: "message", text: textError };
  }
  if (tab === "text" && query.length < TEXT_MIN) {
    return { kind: "message", text: `Type at least ${TEXT_MIN} characters to search text.` };
  }
  if (tab !== "text" && nameError !== null) {
    return { kind: "message", text: nameError };
  }
  if (loading) {
    return { kind: "message", text: "Searching…" };
  }
  return { kind: "ok" };
}

function SearchResults({
  listId,
  groups,
  selected,
  projectName,
  blocked,
  status,
  onActivate,
}: {
  readonly listId: string;
  readonly groups: ReturnType<typeof resultGroups>;
  readonly selected: string | null;
  readonly projectName: (project: string) => string;
  readonly blocked: (project: ProjectId | null) => string | null;
  readonly status: SearchStatus;
  readonly onActivate: (item: ResultItem, newPage: boolean) => void;
}) {
  if (groups.length === 0) {
    return (
      <div className="add-results">
        <p className="add-empty">{status.kind === "message" ? status.text : "No results."}</p>
      </div>
    );
  }
  return (
    <ResultList<ResultItem>
      id={listId}
      label="Results"
      className="add-results"
      sections={groups}
      getId={(item) => item.id}
      getHeight={resultHeight}
      getText={resultText}
      selectedId={selected}
      onActivate={(item, event) => {
        onActivate(item, event.metaKey || event.ctrlKey);
      }}
      renderItem={(item, isSelected) =>
        item.type === "more" ? (
          <MoreRow label={item.label} />
        ) : (
          <ResultRow item={item} selected={isSelected} projectName={projectName(itemProject(item) ?? "")} blocked={blocked(itemProject(item))} />
        )
      }
    />
  );
}
