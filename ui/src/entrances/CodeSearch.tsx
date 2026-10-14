/**
 * The name search an entrance's `code` is chosen with (entrance-definitions.md, "`code`"): Add's list,
 * narrowed to what can start. That is modules, classes, and functions, inline handlers among them by
 * their labels ("callback to ordersRouter.post"). Home's Add entrance and Choose code… use it. Focus
 * stays in its input: ↑ ↓ move the selection, and Enter picks it.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useId, useMemo, useState, type KeyboardEvent, type Ref } from "react";
import { optionId, ResultList } from "../add/ResultList";
import { ResultRow, resultText } from "../add/Rows";
import { resultItems, type ResultItem } from "../add/search";
import { engineQueryOptions } from "../ipc";
import type { ProjectId, SearchArgs, SearchHit } from "../ipc/generated";
import { useDebounced } from "../lib/debounce";
import "../add/add.css";
import "./entrances.css";

/** The query's debounce, as Add's. */
const QUERY_DELAY = 120;
/** The most results listed. */
const LIMIT = 20;

const NO_RESULTS: readonly ResultItem[] = [];

/** The `search` arguments for what can start, in one project. */
function startSearchArgs(query: string, project: ProjectId): SearchArgs {
  return { query, kinds: ["definitions"], definitionKinds: ["module", "class", "function"], limit: LIMIT, projects: [project] };
}

export interface CodeSearchProps {
  readonly project: ProjectId;
  /** Called with the result picked; the field then shows its name until the person types again. */
  readonly onPick: (hit: SearchHit) => void;
  readonly ref?: Ref<HTMLInputElement>;
}

export function CodeSearch({ project, onPick, ref }: CodeSearchProps) {
  const listId = useId();
  const [query, setQuery] = useState("");
  // Whether the list shows: typing opens it, and picking closes it.
  const [listing, setListing] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const typed = query.trim();
  const settled = useDebounced(typed, QUERY_DELAY);
  const searching = listing && typed !== "" && settled !== "";
  const search = useQuery({ ...engineQueryOptions("search", startSearchArgs(settled, project)), enabled: searching, placeholderData: keepPreviousData });
  const data = searching ? search.data : undefined;
  const items = useMemo(() => (data ? resultItems(data) : NO_RESULTS), [data]);
  const selectedIndex = Math.max(0, selectedId === null ? 0 : items.findIndex((item) => item.id === selectedId));
  const selected = items[selectedIndex] ?? null;
  const open = listing && typed !== "";

  const pick = (item: ResultItem) => {
    if (item.kind !== "card") {
      return;
    }
    setQuery(item.hit.qualifiedName);
    setListing(false);
    onPick(item.hit);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing || !open) {
      return;
    }
    const move = (delta: number) => {
      const next = items[Math.max(0, Math.min(items.length - 1, selectedIndex + delta))];
      if (next) {
        setSelectedId(next.id);
      }
    };
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        move(event.key === "ArrowDown" ? 1 : -1);
        break;
      case "Enter":
        if (selected) {
          pick(selected);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  let list = null;
  if (open && items.length > 0) {
    list = (
      <ResultList<ResultItem>
        id={listId}
        label="Code"
        className="code-search-results"
        items={items}
        getId={(item) => item.id}
        getText={resultText}
        selectedId={selected?.id ?? null}
        onActivate={pick}
        renderItem={(item) => <ResultRow item={item} />}
      />
    );
  } else if (open) {
    list = <p className="code-search-empty">{search.error?.message ?? (data === undefined ? "Searching…" : "No results.")}</p>;
  }

  return (
    <div className="code-search">
      <input
        ref={ref}
        className="code-search-input"
        role="combobox"
        aria-label="Code"
        aria-expanded={open && items.length > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && selected ? optionId(listId, selectedIndex) : undefined}
        placeholder="Find a module or function"
        spellCheck={false}
        autoComplete="off"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setListing(true);
          setSelectedId(null);
        }}
        onKeyDown={onKeyDown}
      />
      {list}
    </div>
  );
}
