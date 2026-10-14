/**
 * The Add box: one box over the board. Typing searches names in the board's project, as one ranked
 * list, and its files after them. Focus stays in its input: ↑ ↓ move the selection, Enter adds a name
 * to the page or opens a file in the code panel, Escape closes.
 */

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Icon } from "../icons";
import { engineQueryOptions } from "../ipc";
import { useDebounced } from "../lib/debounce";
import type { PageTarget } from "../state/boards";
import { closeAdd } from "../state/overlays";
import { addResult } from "./activate";
import { optionId, ResultList } from "./ResultList";
import { ResultRow, resultText } from "./Rows";
import { resultItems, searchArgs, type ResultItem } from "./search";
import "./add.css";

/** The query's debounce. */
const QUERY_DELAY = 120;

const NO_RESULTS: readonly ResultItem[] = [];

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

export function AddLayer({ page }: { readonly page: PageTarget }) {
  useModalLayer();
  const listId = useId();
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    input.current?.focus();
  }, []);

  const typed = query.trim();
  const settled = useDebounced(typed, QUERY_DELAY);
  const searching = typed !== "" && settled !== "";
  const search = useQuery({ ...engineQueryOptions("search", searchArgs(settled, page.board.project)), enabled: searching, placeholderData: keepPreviousData });
  const data = searching ? search.data : undefined;
  const items = useMemo(() => (data ? resultItems(data) : NO_RESULTS), [data]);

  // The selection: the chosen row while it is listed, else the first.
  const selectedIndex = Math.max(0, selectedId === null ? 0 : items.findIndex((item) => item.id === selectedId));
  const selected = items[selectedIndex] ?? null;

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
  const choose = (item: ResultItem) => {
    runAfterClose.current = () => {
      addResult(item, page);
    };
    closeAdd();
  };

  const moveSelection = (delta: number) => {
    const next = items[Math.max(0, Math.min(items.length - 1, selectedIndex + delta))];
    if (next) {
      setSelectedId(next.id);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    switch (event.key) {
      case "Escape":
        closeAdd();
        break;
      case "ArrowDown":
      case "ArrowUp":
        moveSelection(event.key === "ArrowDown" ? 1 : -1);
        break;
      case "PageDown":
      case "PageUp":
        moveSelection(event.key === "PageDown" ? 10 : -10);
        break;
      case "Enter":
        if (selected) {
          choose(selected);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  let body: ReactNode = null;
  if (typed !== "" && items.length > 0) {
    body = (
      <ResultList<ResultItem>
        id={listId}
        label="Results"
        className="add-results"
        items={items}
        getId={(item) => item.id}
        getText={resultText}
        selectedId={selected?.id ?? null}
        onActivate={choose}
        renderItem={(item) => <ResultRow item={item} />}
      />
    );
  } else if (typed !== "") {
    const message = search.error?.message ?? (data === undefined ? "Searching…" : "No results.");
    body = <p className="add-empty">{message}</p>;
  }

  return createPortal(
    <div
      className="add-scrim"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) {
          closeAdd();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Add"
        className="add-box"
        onKeyDown={(event) => {
          if (event.key === "Escape" && !event.defaultPrevented) {
            event.preventDefault();
            closeAdd();
          }
        }}
      >
        <div className="add-input-row">
          <Icon name="search" className="add-input-icon" />
          <input
            ref={input}
            className="add-input"
            role="combobox"
            aria-expanded={items.length > 0}
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={selected && typed !== "" ? optionId(listId, selectedIndex) : undefined}
            aria-label="Find"
            placeholder="Find functions, types, entrances, and files"
            spellCheck={false}
            autoComplete="off"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedId(null);
            }}
            onKeyDown={onKeyDown}
          />
        </div>
        {body}
      </div>
    </div>,
    document.body,
  );
}
