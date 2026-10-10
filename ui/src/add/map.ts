/**
 * Add's map before typing (redesign §4.5, 03, V-64): each connected project's applications as lanes of
 * entrance chips, twelve per lane and then a "+N more" chip that expands the lane, plus a lane for the
 * entrances no launch reaches. Pure: the view and the keyboard both walk what this builds.
 */

import type { EntranceMap, EntranceRow, LaunchRow, ProjectId, ProjectRow } from "../ipc/generated";

/** Entrance chips per lane before "+N more" (V-64), and the most an expanded lane shows. */
export const PER_LANE = 12;
export const EXPANDED_LANE = 500;

/** The lane of the entrances that no launch reaches. */
const UNREACHED_LANE = "unreached";

export interface Lane {
  /**
   * `${project}␟${startup}`, or `${project}␟unreached` (U+001F, not NUL: chip ids go into `data-chip-id`
   * selectors, and `CSS.escape` replaces NUL).
   */
  readonly id: string;
  readonly project: ProjectId;
  /** The application's startup key; null for the unreached lane. */
  readonly startup: string | null;
  readonly name: string;
  /** How it is started (`npm run start`), when a launch says. */
  readonly command: string | null;
  readonly total: number;
  readonly entrances: readonly EntranceRow[];
}

export type MapChip =
  | { readonly type: "entrance"; readonly id: string; readonly project: ProjectId; readonly lane: Lane; readonly row: EntranceRow }
  | { readonly type: "more"; readonly id: string; readonly project: ProjectId; readonly lane: Lane; readonly count: number };

export interface LaneView {
  readonly lane: Lane;
  readonly chips: readonly MapChip[];
}

export type ProjectMap =
  | { readonly status: "loading"; readonly row: ProjectRow }
  | { readonly status: "error"; readonly row: ProjectRow; readonly message: string }
  | { readonly status: "ready"; readonly row: ProjectRow; readonly lanes: readonly LaneView[] };

function laneId(project: ProjectId, startup: string | null): string {
  return `${project}\u001f${startup ?? UNREACHED_LANE}`;
}

/** How a launch is typed: `npm run start` for a package script, else its trigger or startup. */
export function launchCommand(launch: LaunchRow): string | null {
  if (launch.detector === "package.script" && launch.trigger !== null) {
    return `npm run ${launch.trigger}`;
  }
  return launch.trigger ?? launch.startup;
}

/** The lanes of one project's map: applications with entrances, then the unreached ones. */
export function lanesOf(map: EntranceMap): Lane[] {
  const lanes: Lane[] = map.applications
    .filter((application) => application.total > 0)
    .map((application) => {
      const launch = application.launches[0];
      return {
        id: laneId(map.project, application.startup),
        project: map.project,
        startup: application.startup,
        name: application.name,
        command: launch ? launchCommand(launch) : null,
        total: application.total,
        entrances: application.entrances,
      };
    });
  if (map.unreached.total > 0) {
    lanes.push({
      id: laneId(map.project, null),
      project: map.project,
      startup: null,
      name: "Not reached from a launch",
      command: null,
      total: map.unreached.total,
      entrances: map.unreached.entrances,
    });
  }
  return lanes;
}

/** The entrances a project's map counts (Look in), summed from its totals without waking it (V-64). */
export function mapTotal(map: EntranceMap): number {
  return map.applications.reduce((sum, application) => sum + application.total, 0) + map.unreached.total;
}

/**
 * A lane's chips: its first twelve entrances and "+N more", or every entrance (up to 500) once it is
 * expanded and `expanded` holds them.
 */
export function laneChips(lane: Lane, expanded: readonly EntranceRow[] | undefined): MapChip[] {
  const rows = expanded ?? lane.entrances.slice(0, PER_LANE);
  const chips: MapChip[] = rows.map((row) => ({
    type: "entrance",
    id: `${lane.id}\u001f${row.key}`,
    project: lane.project,
    lane,
    row,
  }));
  const hidden = lane.total - rows.length;
  if (expanded === undefined && hidden > 0) {
    chips.push({ type: "more", id: `${lane.id}\u001fmore`, project: lane.project, lane, count: hidden });
  }
  return chips;
}

/** The rows of an expanded lane, from `entrances` asked for that application (or the whole map, unreached). */
export function expandedRows(lane: Lane, map: EntranceMap): readonly EntranceRow[] {
  if (lane.startup === null) {
    return map.unreached.entrances;
  }
  return map.applications.find((application) => application.startup === lane.startup)?.entrances ?? [];
}

/** Every chip of the shown projects, in reading order (for ↑ ↓ ← →). */
export function flatChips(projects: readonly ProjectMap[]): MapChip[] {
  return projects.flatMap((project) => (project.status === "ready" ? project.lanes.flatMap((lane) => lane.chips) : []));
}
