/**
 * The map's data: `entrances {perApplication: 12}` for every connected project (they rest, so nothing
 * wakes), and `entrances` with 500 rows for each lane the person expanded. Refreshed by
 * `graph_updated` through the query cache.
 */

import { useQueries, type UseQueryResult } from "@tanstack/react-query";
import { useMemo } from "react";
import { engineQueryOptions, toUnfoldError, type EntranceMap, type EntrancesArgs, type ProjectId, type ProjectRow } from "../ipc";
import { EXPANDED_LANE, PER_LANE, expandedRows, laneChips, lanesOf, mapTotal, type Lane, type ProjectMap } from "./map";

interface AddMap {
  /** Every connected project's map, in workspace order. */
  readonly projects: readonly ProjectMap[];
  /** Each loaded project's entrance count (Look in). */
  readonly counts: ReadonlyMap<ProjectId, number>;
}

type Results = readonly Pick<UseQueryResult<EntranceMap>, "data" | "error">[];

// Module-level so the query observer keeps its combined result between renders.
function combine(results: Results): Results {
  return results.map(({ data, error }) => ({ data, error }));
}

function expandedArgs(lane: Lane): EntrancesArgs {
  return lane.startup === null
    ? { project: lane.project, perApplication: EXPANDED_LANE }
    : { project: lane.project, application: lane.startup, perApplication: EXPANDED_LANE };
}

function errorText(cause: Error): string {
  const error = toUnfoldError(cause);
  return error.code === "not_compiled" ? "Not compiled yet." : error.message;
}

/** The map of `rows`, with the lanes in `expanded` (by lane id) showing every entrance. */
export function useAddMap(rows: readonly ProjectRow[], expanded: ReadonlyMap<string, Lane>): AddMap {
  const maps = useQueries({
    queries: rows.map((row) => engineQueryOptions("entrances", { project: row.id, perApplication: PER_LANE })),
    combine,
  });
  const expandedLanes = useMemo(() => [...expanded.values()], [expanded]);
  const more = useQueries({
    queries: expandedLanes.map((lane) => engineQueryOptions("entrances", expandedArgs(lane))),
    combine,
  });

  return useMemo(() => {
    const expandedByLane = new Map<string, EntranceMap>();
    expandedLanes.forEach((lane, index) => {
      const data = more[index]?.data;
      if (data) {
        expandedByLane.set(lane.id, data);
      }
    });
    const counts = new Map<ProjectId, number>();
    const projects = rows.map((row, index): ProjectMap => {
      const result = maps[index];
      if (result?.error) {
        return { status: "error", row, message: errorText(result.error) };
      }
      const map = result?.data;
      if (!map) {
        return { status: "loading", row };
      }
      counts.set(row.id, mapTotal(map));
      const views = lanesOf(map).map((lane) => {
        const all = expandedByLane.get(lane.id);
        return { lane, chips: laneChips(lane, all ? expandedRows(lane, all) : undefined) };
      });
      return { status: "ready", row, lanes: views };
    });
    return { projects, counts };
  }, [rows, maps, more, expandedLanes]);
}
