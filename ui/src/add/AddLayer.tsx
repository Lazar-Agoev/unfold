/**
 * The Add box: one box over the window. Empty, it browses entrances (LOOK IN, the scope's list, and a
 * preview); typing searches every connected project; `>` lists commands. Focus stays in its input: ↑ ↓
 * move the selection, ← → move between LOOK IN and the list, Enter adds to the current page, ⌘Enter
 * on a new page, Tab switches the kind, Escape closes.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { newCardView } from "../code/typeKinds";
import { KeyChip, Tooltip } from "../components";
import { Icon } from "../icons";
import { engineQueryOptions, type AppRow, type EntranceScope, type ProjectId, type ProjectRow } from "../ipc";
import { actions, pushModal, shortcutLabel, useActions, usePlatform, useShortcutLookup } from "../keyboard";
import { plural } from "../lib/format";
import { findPage, useOpenBoard, usePageKeys, type BoardRef } from "../state/boards";
import { closeAdd, openDialog, useAddTarget, type AddRequest, type AddTarget, type DialogRequest } from "../state/overlays";
import { refProjectFor, useProjectIndex, useWorkspace } from "../state/workspace";
import { activateChoice, type AddChoice } from "./activate";
import {
  applicationSections,
  entranceSections,
  findApplication,
  navItems,
  scopeId,
  scopeProject,
  selectableNav,
  UNREACHED_NAME,
  type ListItem,
  type NavItem,
  type Overview,
  type Scope,
} from "./browse";
import { BrowsePreview } from "./BrowsePreview";
import { EntranceList, LookIn, navOptionId, type ListHeader } from "./Browser";
import { commandGroups, type CommandItem } from "./commands";
import { Preview, type PreviewItem } from "./Preview";
import { listHeight, optionId, ResultList } from "./ResultList";
import { CommandRow, MoreRow, ROW_HEIGHT, ResultRow, resultHeight, resultText } from "./Rows";
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
import { useOverviews, useScopeRows, type ScopeRows } from "./useBrowse";
import "./add.css";

/** Debounces: the query and the preview. */
const QUERY_DELAY = 120;
const PREVIEW_DELAY = 150;
/** The box's fixed height while browsing and searching; its chrome is the input row, the footer, and the border. */
const BOX_HEIGHT = 660;
const CHROME_HEIGHT = 56 + 40 + 2;

const BLOCKED = "Rename one of the projects in its settings";

type Mode = "browse" | "search" | "commands";

/** Where the browser's keyboard is: LOOK IN or the list. */
type Pane = "nav" | "list";

/** What the browser shows; kept between openings of Add (a return from the launch editor restores it). */
interface Browse {
  readonly scope: Scope;
  readonly expanded: ReadonlySet<ProjectId>;
  readonly openCategories: ReadonlySet<string>;
  /** The selected row's id (the first row when it isn't listed). */
  readonly row: string | null;
  readonly pane: Pane;
  /** The keyboard's row in LOOK IN. */
  readonly cursor: string | null;
}

let lastBrowse: Browse | null = null;

/**
 * The browser as Add opens: on a board page, its project expanded, with the last selection when it is
 * in that project, else the project; from Home, the last selection or All projects.
 */
function initialBrowse(pageProject: ProjectId | null): Browse {
  const last = lastBrowse === null ? null : { ...lastBrowse, pane: "list" as const, cursor: null };
  if (pageProject === null) {
    return last ?? { scope: { kind: "all" }, expanded: new Set(), openCategories: new Set(), row: null, pane: "list", cursor: null };
  }
  const expanded = new Set([...(last?.expanded ?? []), pageProject]);
  if (last !== null && scopeProject(last.scope) === pageProject) {
    return { ...last, expanded };
  }
  return { scope: { kind: "project", project: pageProject }, expanded, openCategories: last?.openCategories ?? new Set(), row: null, pane: "list", cursor: null };
}

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
 * While Add is open: window shortcuts stay quiet, the app behind is inert, and focus returns to where it
 * was when Add closes. (The kit's dialogs do the same for their own layers.)
 */
function useModalLayer(): void {
  useLayoutEffect(() => {
    const previous = document.activeElement;
    const popModal = pushModal();
    const root = document.getElementById("root");
    const madeInert = root !== null && !root.hasAttribute("inert");
    if (madeInert) {
      root.setAttribute("inert", "");
    }
    return () => {
      popModal();
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
  if (query.trimStart().startsWith(">")) {
    return "commands";
  }
  return query.trim().length === 0 ? "browse" : "search";
}

function initialQuery(request: AddRequest): string {
  return request.mode === "commands" ? `> ${request.query ?? ""}` : (request.query ?? "");
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

/** The tabs of a mode, in Tab order: browsing has no All. */
function tabsOf(mode: Mode): readonly AddTab[] {
  return mode === "browse" ? KINDS : ["all", ...KINDS];
}

/** The `>` keycap and the hints along the bottom. */
function Footer({ mode, onPage }: { readonly mode: Mode; readonly onPage: boolean }) {
  const platform = usePlatform();
  const hint = (key: string, label: string) => (
    <span className="add-hint">
      <KeyChip variant="keycap" glyph={key} />
      {label}
    </span>
  );
  if (mode === "commands") {
    return (
      <footer className="add-footer">
        {hint(shortcutLabel(platform, "Enter"), "Run")}
        {hint("esc", "Close")}
      </footer>
    );
  }
  return (
    <footer className="add-footer">
      {hint(shortcutLabel(platform, "Enter"), onPage ? "Add to this page" : "Add to a new draft")}
      {onPage ? hint(shortcutLabel(platform, "Mod+Enter"), "Add on a new page") : null}
      {hint("⇥", "Switch kind")}
      <span className="add-footer-spacer" />
      <span className="add-hint">
        Type
        <KeyChip variant="keycap" glyph=">" />
        for commands
      </span>
    </footer>
  );
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

export function AddLayer({ request }: { readonly request: AddRequest }) {
  useModalLayer();
  const listId = useId();
  const navId = useId();
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const target = useAddTarget();
  const page = target.kind === "page" ? target.target : null;
  const pageTitle = usePageTitle(target);
  const onPageKeys = usePageKeys(page);
  const index = useProjectIndex();
  const workspace = useWorkspace();
  const rows = useMemo<readonly ProjectRow[]>(() => (workspace?.projects ?? []).filter((row) => row.state.kind !== "unavailable"), [workspace]);

  const [query, setQuery] = useState(() => initialQuery(request));
  const [chosenTab, setChosenTab] = useState<AddTab | null>(request.kind ?? null);
  const [options, setOptions] = useState<TextOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [browse, setBrowse] = useState<Browse>(() => initialBrowse(page?.board.project ?? null));
  useEffect(() => {
    lastBrowse = browse;
  }, [browse]);

  const mode = modeOf(query);
  const browseTab: AddTab = chosenTab === null || chosenTab === "all" ? "entrances" : chosenTab;
  const searchTab: AddTab = chosenTab ?? "all";
  const tab = mode === "browse" ? browseTab : searchTab;

  useEffect(() => {
    input.current?.focus();
  }, []);

  /** Why results of `project` can't be added to the page, if they can't. */
  const blocked = (project: ProjectId | null): string | null =>
    page !== null && project !== null && !refProjectFor(index, page.board.project, project).ok ? BLOCKED : null;
  const projectName = (project: string) => index.byId.get(project)?.name ?? "?";

  // ---- Browse ----
  const overviews = useOverviews(rows);
  // A scope in a project that is no longer connected falls back to All projects.
  const scope = useMemo<Scope>(
    () => (browse.scope.kind === "all" || rows.some((row) => row.id === scopeProject(browse.scope)) ? browse.scope : { kind: "all" }),
    [browse.scope, rows],
  );
  const nav = useMemo(() => navItems(overviews, browse.expanded, browse.openCategories, scope), [overviews, browse.expanded, browse.openCategories, scope]);
  const scopeRows = useScopeRows(scope);
  const app = findApplication(overviews, scope);
  const sections = useMemo(() => browseSections(overviews, scope, scopeRows.pages, app), [overviews, scope, scopeRows.pages, app]);
  const browseItems = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const browseIndex = Math.max(0, browseItems.findIndex((item) => item.id === browse.row));
  const browseItem = browseItems[browseIndex] ?? null;
  const browsePreview = useDebounced(browseItem, PREVIEW_DELAY);
  const browsing = mode === "browse" && browseTab === "entrances";

  // ---- Search ----
  const settled = useDebounced(query, QUERY_DELAY);
  const searching = mode === "search" && modeOf(settled) === "search";
  const nameQuery = settled.trim();
  const allNames = useQuery({ ...engineQueryOptions("search", allSearchArgs(nameQuery)), enabled: searching, placeholderData: keepPreviousData });
  const kindArgs = kindSearchArgs(nameQuery, searchTab);
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
    () => (mode === "search" ? resultGroups(searchTab, names, kindData, textResults) : []),
    [mode, searchTab, names, kindData, textResults],
  );
  const items = useMemo(() => flatItems(groups), [groups]);
  const counts = mode === "search" ? kindCounts(names, textResults) : null;

  // ---- Commands ----
  const registered = useActions();
  const shortcutOf = useShortcutLookup();
  // Which commands apply is decided once per opening: Add is modal, so nothing changes underneath it.
  const [enabled] = useState(() => new Set(actions.list().flatMap((action) => (actions.isEnabled(action.id) ? [action.id] : []))));
  const commandQuery = mode === "commands" ? query.trimStart().slice(1) : "";
  const commands = useMemo(
    () => (mode === "commands" ? commandGroups(registered, commandQuery, (id) => enabled.has(id)) : []),
    [mode, registered, commandQuery, enabled],
  );
  const commandItems = useMemo(() => commands.flatMap((group) => group.items), [commands]);

  // The selection: the chosen row while it is listed, else the first.
  const listIds = mode === "commands" ? commandItems.map((item) => item.action.id) : items.map((item) => item.id);
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

  const runCommand = (item: CommandItem) => {
    closeThen(() => void actions.run(item.action.id));
  };

  const update = (change: Partial<Browse>) => {
    setBrowse((current) => ({ ...current, ...change }));
  };

  /** Selects a scope (its project unfolds); the list starts at its first row. */
  const selectScope = (next: Scope, pane: Pane = browse.pane) => {
    const project = scopeProject(next);
    const expanded = project === null || browse.expanded.has(project) ? browse.expanded : new Set([...browse.expanded, project]);
    update({ scope: next, expanded, row: null, pane, cursor: scopeId(next) });
  };

  const toggleProject = (project: ProjectId) => {
    const expanded = new Set(browse.expanded);
    if (!expanded.delete(project)) {
      expanded.add(project);
    }
    update({ expanded });
  };

  const chooseNav = (item: NavItem, pane: Pane = browse.pane) => {
    if (item.type === "more") {
      update({ openCategories: new Set([...browse.openCategories, item.category]), cursor: null });
    } else if (item.type !== "label" && item.type !== "note") {
      selectScope(item.scope, pane);
    }
  };

  const activateBrowse = (item: ListItem, newPage: boolean) => {
    switch (item.type) {
      case "entrance":
        choose({ kind: "entrance", project: item.project, key: item.row.key }, newPage);
        return;
      case "application":
      case "unreached":
        selectScope(item.scope, "list");
        return;
      case "more":
        scopeRows.fetchMore();
        return;
    }
  };

  const edit = (request: DialogRequest) => {
    closeThen(() => {
      openDialog(request);
    });
  };

  const moveNav = (delta: number) => {
    const items = selectableNav(nav);
    const at = items.findIndex((item) => item.id === (browse.cursor ?? scopeId(scope)));
    const next = items[Math.max(0, Math.min(items.length - 1, (at < 0 ? 0 : at) + delta))];
    if (next === undefined) {
      return;
    }
    if (next.type === "more") {
      update({ cursor: next.id });
    } else {
      chooseNav(next, "nav");
    }
  };

  /** ← and → in the browser: between LOOK IN and the list; in LOOK IN they fold and unfold a project. */
  const sideways = (right: boolean) => {
    if (browse.pane === "list") {
      if (!right) {
        update({ pane: "nav", cursor: scopeId(scope) });
      }
      return;
    }
    const cursor = nav.find((item) => item.id === (browse.cursor ?? scopeId(scope)));
    if (cursor?.type === "project" && cursor.expanded !== right) {
      toggleProject(cursor.row.id);
    } else if (cursor?.type === "more" && right) {
      chooseNav(cursor);
    } else if (right) {
      update({ pane: "list" });
    }
  };

  const switchTab = (delta: number) => {
    const tabs = tabsOf(mode);
    const at = tabs.indexOf(tab);
    const next = tabs[(at + delta + tabs.length) % tabs.length];
    if (next !== undefined) {
      setChosenTab(next);
      setSelectedId(null);
    }
  };

  const moveSelection = (delta: number) => {
    if (mode === "browse") {
      if (!browsing) {
        return;
      }
      if (browse.pane === "nav") {
        moveNav(delta);
        return;
      }
      const next = browseItems[Math.max(0, Math.min(browseItems.length - 1, browseIndex + delta))];
      if (next) {
        update({ row: next.id });
      }
      return;
    }
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
        if (mode !== "commands") {
          switchTab(event.shiftKey ? -1 : 1);
        }
        break;
      case "ArrowDown":
        moveSelection(1);
        break;
      case "ArrowUp":
        moveSelection(-1);
        break;
      case "ArrowRight":
      case "ArrowLeft":
        if (!browsing) {
          return;
        }
        sideways(event.key === "ArrowRight");
        break;
      case "PageDown":
      case "PageUp":
        if (mode === "browse" && (!browsing || browse.pane === "nav")) {
          return;
        }
        moveSelection(event.key === "PageDown" ? 10 : -10);
        break;
      case "Enter":
        if (mode === "browse") {
          if (!browsing) {
            break;
          }
          if (browse.pane === "nav") {
            const cursor = nav.find((item) => item.id === browse.cursor);
            if (cursor?.type === "more") {
              chooseNav(cursor);
            } else {
              update({ pane: "list" });
            }
          } else if (browseItem) {
            activateBrowse(browseItem, newPage);
          }
        } else if (mode === "commands") {
          const item = commandItems[selectedIndex];
          if (item) {
            runCommand(item);
          }
        } else if (selectedItem) {
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
  if (mode === "commands") {
    body = (
      <CommandsBody
        listId={listId}
        groups={commands}
        selected={selected}
        shortcutOf={shortcutOf}
        onRun={runCommand}
      />
    );
  } else if (mode === "browse") {
    body = !browsing ? (
      <div className="add-body">
        <p className="add-empty">{`Type to find ${tab === "text" ? "text" : KIND_LABELS[browseTab].toLowerCase()} in ${plural(rows.length, "project")}.`}</p>
      </div>
    ) : rows.length === 0 ? (
      <div className="add-body">
        <p className="add-empty">No projects are connected. Add one from Home.</p>
      </div>
    ) : (
      <div className="add-body">
        <LookIn
          id={navId}
          items={nav}
          selected={scopeId(scope)}
          cursor={browse.pane === "nav" ? (browse.cursor ?? scopeId(scope)) : null}
          onChoose={(item) => {
            chooseNav(item);
          }}
          onToggle={toggleProject}
        />
        <EntranceList
          id={listId}
          header={listHeader(scope, overviews, app, scopeRows)}
          sections={sections}
          message={listMessage(scope, overviews, scopeRows, sections.length)}
          selected={browseItem?.id ?? null}
          onPage={onPageKeys}
          blocked={blocked}
          loadingMore={scopeRows.fetchingMore}
          onActivate={(item, newPage) => {
            update({ row: item.id, pane: "list" });
            activateBrowse(item, newPage);
          }}
          onEnd={scopeRows.fetchMore}
        />
        <BrowsePreview
          item={browsePreview}
          app={app}
          projectName={projectName}
          onPage={page !== null}
          blocked={browsePreview?.type === "entrance" ? blocked(browsePreview.project) : null}
          onActivate={(newPage) => {
            if (browsePreview) {
              activateBrowse(browsePreview, newPage);
            }
          }}
          onEdit={edit}
        />
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

  const activeDescendant = browsing ? browseDescendant(browse, scope, nav, navId, listId, browseItem === null ? -1 : browseIndex) : selectedIndex >= 0 ? optionId(listId, selectedIndex) : undefined;

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
        className={mode === "commands" ? "add-box commands" : "add-box"}
        onKeyDown={(event) => {
          if (event.key === "Escape" && !event.defaultPrevented) {
            event.preventDefault();
            close();
          }
        }}
        style={mode === "commands" ? undefined : { height: BOX_HEIGHT }}
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
            aria-label="Find or run a command"
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
          <KeyChip variant="keycap" glyph="esc" className="add-esc" />
        </div>
        {mode === "commands" ? null : (
          <div className="add-kinds">
            <Tabs
              tabs={tabsOf(mode)}
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
        )}
        {body}
        <Footer mode={mode} onPage={page !== null} />
      </div>
    </div>,
    document.body,
  );
}

/** The list of a scope: an application's (or the unreached) rows, or the applications of a project or of all. */
function browseSections(overviews: readonly Overview[], scope: Scope, pages: readonly EntranceScope[], app: AppRow | null) {
  switch (scope.kind) {
    case "application":
    case "unreached":
      return entranceSections(scope.project, pages, app);
    case "project":
      return applicationSections(
        overviews.filter((overview) => overview.row.id === scope.project),
        false,
      );
    case "all":
      return applicationSections(overviews, true);
  }
}

/** The list's header: the application with its launches and routes, the unreached, a project, or all. */
function listHeader(scope: Scope, overviews: readonly Overview[], app: AppRow | null, rows: ScopeRows): ListHeader | null {
  switch (scope.kind) {
    case "application": {
      if (app === null) {
        return null;
      }
      const launches = app.launches.length === 0 ? "No launch defined" : plural(app.launches.length, "launch", "launches");
      return { title: app.name, meta: app.routes > 0 ? `${launches} · ${plural(app.routes, "route")}` : launches, description: app.description };
    }
    case "unreached": {
      const total = rows.pages.at(-1)?.total;
      return { title: UNREACHED_NAME, meta: total === undefined ? null : plural(total, "entrance"), description: null };
    }
    case "project": {
      const overview = overviews.find((entry) => entry.row.id === scope.project);
      if (!overview) {
        return null;
      }
      const counts = overview.status === "ready" ? overview.overview.counts : null;
      const meta = counts ? [plural(counts.applications, "application"), plural(counts.launches, "launch", "launches"), plural(counts.routes, "route")].join(" · ") : null;
      return { title: overview.row.name, meta, description: null };
    }
    case "all": {
      const ready = overviews.flatMap((overview) => (overview.status === "ready" ? [overview.overview.counts.applications] : []));
      return {
        title: "All projects",
        meta: `${plural(overviews.length, "project")} · ${plural(
          ready.reduce((sum, count) => sum + count, 0),
          "application",
        )}`,
        description: null,
      };
    }
  }
}

/** What the list says instead of rows: loading, an error, or nothing to show. */
function listMessage(scope: Scope, overviews: readonly Overview[], rows: ScopeRows, sections: number): string | null {
  if (sections > 0) {
    return null;
  }
  if (scope.kind === "application" || scope.kind === "unreached") {
    return rows.error ?? (rows.loading ? "Loading entrances…" : "No entrances found.");
  }
  const shown = scope.kind === "all" ? overviews : overviews.filter((overview) => overview.row.id === scope.project);
  if (shown.some((overview) => overview.status === "loading")) {
    return "Loading entrances…";
  }
  const failed = shown.find((overview) => overview.status === "error");
  return failed?.status === "error" ? failed.message : "No entrances found.";
}

/** The row the input points at: LOOK IN's keyboard row, or the list's selection. */
function browseDescendant(browse: Browse, scope: Scope, nav: readonly NavItem[], navId: string, listId: string, index: number): string | undefined {
  if (browse.pane === "nav") {
    const at = nav.findIndex((item) => item.id === (browse.cursor ?? scopeId(scope)));
    return at < 0 ? undefined : navOptionId(navId, at);
  }
  return index < 0 ? undefined : optionId(listId, index);
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

function CommandsBody({
  listId,
  groups,
  selected,
  shortcutOf,
  onRun,
}: {
  readonly listId: string;
  readonly groups: ReturnType<typeof commandGroups>;
  readonly selected: string | null;
  readonly shortcutOf: (id: string) => string | null;
  readonly onRun: (item: CommandItem) => void;
}) {
  if (groups.length === 0) {
    return <p className="add-empty commands">No matching commands.</p>;
  }
  const height = Math.min(listHeight(groups, () => ROW_HEIGHT), BOX_HEIGHT - CHROME_HEIGHT);
  return (
    <ResultList<CommandItem>
      id={listId}
      label="Commands"
      className="add-commands"
      sections={groups}
      getId={(item) => item.action.id}
      getHeight={() => ROW_HEIGHT}
      getText={(item) => (item.prefix === null ? item.action.title : `${item.prefix} › ${item.action.title}`)}
      selectedId={selected}
      onActivate={onRun}
      renderItem={(item, isSelected) => <CommandRow item={item} selected={isSelected} shortcut={shortcutOf(item.action.id)} />}
      height={height}
    />
  );
}
