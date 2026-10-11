/**
 * The entrance browser's data: the `entrances` overview of every connected project (they rest, so
 * nothing wakes), and the selected application's (or unreached) rows a page at a time. Refreshed by
 * `graph_updated` through the query cache.
 */

import { useInfiniteQuery, useQueries, type UseQueryResult } from "@tanstack/react-query";
import { useMemo } from "react";
import { call, engineKey, engineQueryOptions, toUnfoldError, type EntranceScope, type EntrancesArgs, type EntrancesView, type ProjectRow } from "../ipc";
import type { Overview, Scope } from "./browse";

/** Rows per page of a scope. */
const PAGE_ROWS = 100;

type Results = readonly Pick<UseQueryResult<EntrancesView>, "data" | "error">[];

// Module-level so the query observer keeps its combined result between renders.
function combine(results: Results): Results {
  return results.map(({ data, error }) => ({ data, error }));
}

function errorText(cause: Error): string {
  const error = toUnfoldError(cause);
  return error.code === "not_compiled" ? "Not compiled yet." : error.message;
}

/** Every connected project's overview, in workspace order. */
export function useOverviews(rows: readonly ProjectRow[]): readonly Overview[] {
  const results = useQueries({
    queries: rows.map((row) => engineQueryOptions("entrances", { project: row.id })),
    combine,
  });
  return useMemo(
    () =>
      rows.map((row, index): Overview => {
        const result = results[index];
        if (result?.error) {
          return { status: "error", row, message: errorText(result.error) };
        }
        const data = result?.data;
        return data?.view === "overview" ? { status: "ready", row, overview: data } : { status: "loading", row };
      }),
    [rows, results],
  );
}

function scopeArgs(scope: Scope): EntrancesArgs | null {
  switch (scope.kind) {
    case "application":
      return { project: scope.project, application: scope.application, limit: PAGE_ROWS };
    case "unreached":
      return { project: scope.project, unreached: true, limit: PAGE_ROWS };
    case "all":
    case "project":
      return null;
  }
}

/** Scope rows' state: the loaded pages, and how to ask for the next. */
export interface ScopeRows {
  readonly pages: readonly EntranceScope[];
  readonly error: string | null;
  readonly loading: boolean;
  readonly fetchingMore: boolean;
  readonly fetchMore: () => void;
}

const NO_ARGS: EntrancesArgs = { project: "" };

/** An application's or the unreached rows, paged with `nextCursor`; nothing for the other scopes. */
export function useScopeRows(scope: Scope): ScopeRows {
  const args = scopeArgs(scope);
  const query = useInfiniteQuery({
    queryKey: engineKey("entrances", args ?? NO_ARGS),
    queryFn: ({ pageParam }) => call("entrances", pageParam === null ? (args ?? NO_ARGS) : { ...(args ?? NO_ARGS), cursor: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => (last.view === "scope" ? last.nextCursor : null),
    enabled: args !== null,
  });
  const pages = useMemo(() => (query.data?.pages ?? []).filter((page): page is Extract<EntrancesView, { view: "scope" }> => page.view === "scope"), [query.data]);
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = query;
  return {
    pages,
    error: query.error ? errorText(query.error) : null,
    loading: args !== null && query.isPending,
    fetchingMore: isFetchingNextPage,
    fetchMore: () => {
      if (hasNextPage && !isFetchingNextPage) {
        void fetchNextPage();
      }
    },
  };
}
