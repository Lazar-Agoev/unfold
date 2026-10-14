/**
 * The name search an entrance's `code` is chosen with (entrance-definitions.md, "`code`"): Add's list,
 * narrowed to what can start. That is modules, classes, and functions, inline handlers among them by
 * their labels ("callback to ordersRouter.post"). Home's Add entrance and Choose code… use it, and Add
 * resource…'s From a type… narrows it to types and classes. Focus stays in its input: ↑ ↓ move the
 * selection, and Enter picks it (once the list answers what is typed).
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useId, useMemo, useState, type KeyboardEvent, type Ref } from "react";
import { optionId, ResultList } from "../add/ResultList";
import { ResultRow, resultText } from "../add/Rows";
import { resultItems, type ResultItem } from "../add/search";
import { engineQueryOptions } from "../ipc";
import type { NodeKind, ProjectId, SearchArgs, SearchHit } from "../ipc/generated";
import { useDebounced } from "../lib/debounce";
import "../add/add.css";
import "./entrances.css";

/** The query's debounce, as Add's. */
const QUERY_DELAY = 120;
/** The most results listed. */
const LIMIT = 20;

const NO_RESULTS: readonly ResultItem[] = [];

/** What can start: modules, classes, and functions. */
const START_KINDS: readonly NodeKind[] = ["module", "class", "function"];

/** The `search` arguments for definitions of `kinds`, in one project. */
function searchArgs(query: string, project: ProjectId, kinds: readonly NodeKind[]): SearchArgs {
  return { query, kinds: ["definitions"], definitionKinds: [...kinds], limit: LIMIT, projects: [project] };
}

export interface CodeSearchProps {
  readonly project: ProjectId;
  /** The definitions it finds (what can start when absent: modules, classes, and functions). */
  readonly kinds?: readonly NodeKind[];
  /** The field's accessible name and placeholder (an entrance's code when absent). */
  readonly label?: string;
  readonly placeholder?: string;
  /** Called with the result picked; the field then shows its name until the person types again. */
  readonly onPick: (hit: SearchHit) => void;
  /** Called when the person types, which leaves no result picked. */
  readonly onEdit?: () => void;
  readonly ref?: Ref<HTMLInputElement>;
}

export function CodeSearch({ project, kinds = START_KINDS, label = "Code", placeholder = "Find a module or function", onPick, onEdit, ref }: CodeSearchProps) {
  const listId = useId();
  const [query, setQuery] = useState("");
  // Whether the list shows: typing opens it, and picking closes it.
  const [listing, setListing] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const typed = query.trim();
  const settled = useDebounced(typed, QUERY_DELAY);
  const searching = listing && typed !== "" && settled !== "";
  const search = useQuery({ ...engineQueryOptions("search", searchArgs(settled, project, kinds)), enabled: searching, placeholderData: keepPreviousData });
  const data = searching ? search.data : undefined;
  const items = useMemo(() => (data ? resultItems(data) : NO_RESULTS), [data]);
  // Whether the list answers what is typed now, not a query typed before (still shown while the new one runs).
  const current = data !== undefined && settled === typed && !search.isPlaceholderData;
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
        if (selected && current) {
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
        label={label}
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
        aria-label={label}
        aria-expanded={open && items.length > 0}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && selected ? optionId(listId, selectedIndex) : undefined}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setListing(true);
          setSelectedId(null);
          onEdit?.();
        }}
        onKeyDown={onKeyDown}
      />
      {list}
    </div>
  );
}
