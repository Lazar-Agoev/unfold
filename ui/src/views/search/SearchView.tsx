/**
 * The Search view (architecture §11.3, U-30): one field, results grouped Definitions, Entrances,
 * Files, and Text, with tabs to narrow them. Names come from `search`; text streams from
 * `search_text`, and a new query cancels the running one. Definitions and entrances open cards;
 * files and text matches open source tabs. A definition's context menu can add it as an entrance.
 */

import { CaseSensitive, Regex, WholeWord } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { create } from "zustand";
import { IconButton, SearchField, VirtualList, useContextMenu, type ListGroup } from "../../components";
import type { FileHit, SearchHit, SearchResults, TextMatch, TextSearchFile } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { addAsEntranceItem } from "../../settings";
import { openBlockCard, openEntrance } from "../../state/boards/api";
import { openSource } from "../../state/tabs";
import { plural } from "../../shell/status";
import { KindGlyph } from "../glyphs";
import { ViewHeader } from "../ViewHeader";
import "../views.css";
import { highlightChars, highlightRanges } from "./highlight";
import { runTextSearch, useTextSearch } from "./textSearch";

export type SearchTab = "all" | "definitions" | "entrances" | "files" | "text";

const SEARCH_TABS: readonly SearchTab[] = ["all", "definitions", "entrances", "files", "text"];

export type SearchItem =
  | { readonly kind: "definition"; readonly id: string; readonly hit: SearchHit }
  | { readonly kind: "entrance"; readonly id: string; readonly hit: SearchHit }
  | { readonly kind: "file"; readonly id: string; readonly hit: FileHit }
  | { readonly kind: "textFile"; readonly id: string; readonly path: string; readonly count: number }
  | { readonly kind: "textMatch"; readonly id: string; readonly path: string; readonly match: TextMatch };

interface SearchViewState {
  readonly query: string;
  readonly tab: SearchTab;
  readonly regex: boolean;
  readonly caseSensitive: boolean;
  readonly wholeWord: boolean;
}

/** The view's state, kept while the sidebar shows other views. */
export const useSearchView = create<SearchViewState>()(() => ({
  query: "",
  tab: "all",
  regex: false,
  caseSensitive: false,
  wholeWord: false,
}));

/** `value`, once it has stopped changing for `delay` ms. */
export function useDebounced<T>(value: T, delay: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setSettled(value);
    }, delay);
    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);
  return settled;
}

const NAME_DELAY = 120;
const NO_FILES: readonly TextSearchFile[] = [];
const TEXT_DELAY = 250;

/** The result groups for a tab, from name results and streamed text files. */
export function searchGroups(
  tab: SearchTab,
  names: SearchResults | undefined,
  textFiles: readonly TextSearchFile[],
  textLabel: string | null,
): ListGroup<SearchItem>[] {
  const groups: ListGroup<SearchItem>[] = [];
  const show = (group: SearchTab) => tab === "all" || tab === group;
  if (names && show("definitions")) {
    groups.push({
      id: "definitions",
      label: "Definitions",
      count: String(names.definitions.length),
      items: names.definitions.map((hit) => ({ kind: "definition", id: `d:${hit.key}`, hit })),
    });
  }
  if (names && show("entrances")) {
    groups.push({
      id: "entrances",
      label: "Entrances",
      count: String(names.entrances.length),
      items: names.entrances.map((hit) => ({ kind: "entrance", id: `e:${hit.key}`, hit })),
    });
  }
  if (names && show("files")) {
    groups.push({
      id: "files",
      label: "Files",
      count: names.filesTruncated ? `${names.files.length}+` : String(names.files.length),
      items: names.files.map((hit) => ({ kind: "file", id: `f:${hit.path}`, hit })),
    });
  }
  if (show("text")) {
    const items: SearchItem[] = [];
    for (const file of textFiles) {
      items.push({ kind: "textFile", id: `t:${file.path}`, path: file.path, count: file.matches.length });
      for (const match of file.matches) {
        items.push({ kind: "textMatch", id: `m:${file.path}:${match.line}`, path: file.path, match });
      }
    }
    groups.push({ id: "text", label: "Text", ...(textLabel === null ? {} : { count: textLabel }), items });
  }
  return groups;
}

/** "12 matches in 5 files", with "+" when the search stopped at its limit. */
export function textCountLabel(files: readonly TextSearchFile[], truncated: boolean): string {
  const matches = files.reduce((sum, file) => sum + file.matches.length, 0);
  return `${plural(matches, "match", "matches")}${truncated ? "+" : ""} in ${plural(files.length, "file")}`;
}

function activate(item: SearchItem): void {
  switch (item.kind) {
    case "definition":
      openBlockCard(item.hit.key);
      return;
    case "entrance":
      openEntrance(item.hit.key);
      return;
    case "file":
      openSource(item.hit.path);
      return;
    case "textFile":
      openSource(item.path);
      return;
    case "textMatch":
      openSource(item.path, item.match.line);
      return;
  }
}

function itemText(item: SearchItem): string {
  switch (item.kind) {
    case "definition":
    case "entrance":
      return `${item.hit.qualifiedName} ${item.hit.path}`;
    case "file":
      return item.hit.path;
    case "textFile":
      return `${item.path}, ${plural(item.count, "match", "matches")}`;
    case "textMatch":
      return `Line ${item.match.line}: ${item.match.text}`;
  }
}

function renderItem(item: SearchItem): ReactNode {
  switch (item.kind) {
    case "definition": {
      const { hit } = item;
      const prefix = hit.qualifiedName.endsWith(hit.name) ? hit.qualifiedName.slice(0, hit.qualifiedName.length - hit.name.length) : "";
      return (
        <div className="result-row">
          <KindGlyph kind={hit.kind} />
          <span className="result-name">
            {prefix ? <span className="result-qualifier">{prefix}</span> : null}
            {highlightChars(hit.name, hit.nameMatches)}
          </span>
          <span className="result-detail" title={hit.path}>
            {hit.path.slice(hit.path.lastIndexOf("/") + 1)}
            {hit.line === null ? "" : `:${hit.line}`}
          </span>
        </div>
      );
    }
    case "entrance": {
      const { hit } = item;
      return (
        <div className="result-row">
          <span className="result-kind">{hit.kind}</span>
          <span className="result-name">{highlightChars(hit.name, hit.nameMatches)}</span>
          <span className="result-detail" title={hit.path}>
            {hit.path.slice(hit.path.lastIndexOf("/") + 1)}
          </span>
        </div>
      );
    }
    case "file": {
      const { hit } = item;
      // Matches are offsets into the path: the ones in the file name light up the name.
      const nameStart = hit.path.length - hit.name.length;
      const dir = hit.path.slice(0, nameStart);
      return (
        <div className="result-row">
          <span className="result-name">
            {highlightChars(
              hit.name,
              hit.matches.filter((offset) => offset >= nameStart).map((offset) => offset - nameStart),
            )}
          </span>
          <span className="result-detail">
            {highlightChars(
              dir,
              hit.matches.filter((offset) => offset < nameStart),
            )}
          </span>
        </div>
      );
    }
    case "textFile": {
      const slash = item.path.lastIndexOf("/");
      return (
        <div className="result-row result-file">
          <span className="result-name">{item.path.slice(slash + 1)}</span>
          <span className="result-detail">{item.path.slice(0, slash + 1)}</span>
          <span className="result-count">{item.count}</span>
        </div>
      );
    }
    case "textMatch": {
      const { match } = item;
      // Leading indentation says nothing in a result list; the ranges move with the text.
      const indent = /^\s*/.exec(match.text)?.[0].length ?? 0;
      const ranges = match.ranges.map(([start, end]): [number, number] => [start - indent, end - indent]);
      return (
        <div className="result-row result-match">
          <span className="result-line">{match.line}</span>
          <span className="result-text">
            {match.column > 0 ? "…" : ""}
            {highlightRanges(match.text.slice(indent), ranges)}
          </span>
        </div>
      );
    }
  }
}

export function SearchView() {
  const { query, tab, regex, caseSensitive, wholeWord } = useSearchView();
  const trimmed = query.trim();
  const nameQuery = useDebounced(trimmed, NAME_DELAY);
  const textQuery = useDebounced(query, TEXT_DELAY);
  const list = useRef<HTMLDivElement>(null);
  const menu = useContextMenu("Search result actions");

  const names = useEngineQuery("search", { query: nameQuery }, { enabled: nameQuery.length > 0 && tab !== "text" });
  const text = useTextSearch();
  const wantsText = tab === "all" || tab === "text";

  useEffect(() => {
    runTextSearch(wantsText && textQuery.trim().length > 0 ? { query: textQuery, regex, caseSensitive, wholeWord } : null);
  }, [wantsText, textQuery, regex, caseSensitive, wholeWord]);

  const nameResults = nameQuery.length > 0 && trimmed.length > 0 ? names.data : undefined;
  const hasQuery = trimmed.length > 0;
  const textFiles = useMemo(() => (hasQuery ? text.files : NO_FILES), [hasQuery, text.files]);
  const textLabel =
    text.signature === null || trimmed.length === 0
      ? null
      : text.running
        ? `${textCountLabel(textFiles, false)} · searching…`
        : textCountLabel(textFiles, text.summary?.truncated ?? false);
  const groups = useMemo(() => searchGroups(tab, nameResults, textFiles, textLabel), [tab, nameResults, textFiles, textLabel]);

  const counts: Record<SearchTab, number | undefined> = {
    all: undefined,
    definitions: nameResults?.definitions.length,
    entrances: nameResults?.entrances.length,
    files: nameResults?.files.length,
    text: textFiles.reduce((sum, file) => sum + file.matches.length, 0) || undefined,
  };
  const tabs = SEARCH_TABS.map((id) => ({
    id,
    label: id === "all" ? "All" : id.charAt(0).toUpperCase() + id.slice(1),
    ...(counts[id] ? { count: counts[id] } : {}),
  }));

  const empty =
    trimmed.length === 0 ? (
      <p className="view-hint">Search definitions, entrances, files, and text.</p>
    ) : names.isFetching || text.running ? (
      <p className="view-hint">Searching…</p>
    ) : names.error && tab !== "text" ? (
      <p className="view-error">{names.error.message}</p>
    ) : text.error && wantsText ? (
      <p className="view-error">{text.error}</p>
    ) : (
      <p className="view-hint">No results for “{trimmed}”.</p>
    );

  return (
    <section className="view search-view" aria-labelledby="search-title">
      <ViewHeader title="Search" id="search-title" />
      <div className="search-controls">
        <SearchField
          label="Search"
          placeholder="Search"
          focusKey="search"
          value={query}
          onChange={(value) => {
            useSearchView.setState({ query: value });
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "Enter") {
              event.preventDefault();
              list.current?.focus();
            }
          }}
        />
        <div className="search-options" aria-label="Text search options" role="group">
          <IconButton
            icon={CaseSensitive}
            label="Match case"
            size="small"
            pressed={caseSensitive}
            onClick={() => {
              useSearchView.setState({ caseSensitive: !caseSensitive });
            }}
          />
          <IconButton
            icon={WholeWord}
            label="Match whole word"
            size="small"
            pressed={wholeWord}
            onClick={() => {
              useSearchView.setState({ wholeWord: !wholeWord });
            }}
          />
          <IconButton
            icon={Regex}
            label="Use regular expression"
            size="small"
            pressed={regex}
            onClick={() => {
              useSearchView.setState({ regex: !regex });
            }}
          />
        </div>
      </div>
      <div role="tablist" aria-label="Result kinds" className="search-tabs">
        {tabs.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            aria-selected={entry.id === tab}
            tabIndex={entry.id === tab ? 0 : -1}
            className="search-tab"
            onClick={() => {
              useSearchView.setState({ tab: entry.id });
            }}
            onKeyDown={(event) => {
              const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
              if (step !== 0) {
                event.preventDefault();
                const index = SEARCH_TABS.indexOf(entry.id);
                const next = SEARCH_TABS[(index + step + SEARCH_TABS.length) % SEARCH_TABS.length] ?? "all";
                useSearchView.setState({ tab: next });
                event.currentTarget.parentElement?.querySelector<HTMLElement>(`[data-tab="${next}"]`)?.focus();
              }
            }}
            data-tab={entry.id}
          >
            {entry.label}
            {entry.count === undefined ? null : <span className="search-tab-count">{entry.count}</span>}
          </button>
        ))}
      </div>
      <VirtualList<SearchItem>
        ref={list}
        label="Search results"
        className="search-results"
        groups={groups}
        getId={(item) => item.id}
        getText={itemText}
        renderItem={renderItem}
        onActivate={activate}
        onContextMenu={(item, at) => {
          if (item.kind !== "definition") {
            return;
          }
          const { hit } = item;
          menu.open(at, [
            { id: "card", label: "Open card", onSelect: () => { openBlockCard(hit.key); } },
            { id: "source", label: "Open source", onSelect: () => { openSource(hit.path, hit.line); } },
            ...addAsEntranceItem({ path: hit.path, kind: hit.kind, name: hit.name, qualifiedName: hit.qualifiedName, line: hit.line }),
          ]);
        }}
        empty={empty}
      />
      {menu.element}
    </section>
  );
}
