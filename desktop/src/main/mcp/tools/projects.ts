/**
 * Orient (design §6.2): `list_projects` and `get_project`. Listing never wakes a resting project
 * (architecture §11.2): it reads the workspace and each project's boards from disk, and asks a live
 * project for its languages. `get_project` wakes the project it names.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { ApplicationEntrances, BoardSummary, GraphStatus, Language, ProjectConfig, ProjectRow, ProjectState, ProjectSummary } from "../engine-types";
import { textResult } from "../results";
import { defineTool } from "../tool";

/** The most boards `get_project` lists; `list_boards` lists them all. */
const MAX_BOARDS = 40;

const LANGUAGES: Readonly<Record<Language, string>> = { typescript: "TypeScript", python: "Python" };

/** A project's state in words: `current`, `compiling (120 of 400 files)`, `resting (last: current)`. */
function stateText(state: ProjectState): string {
  switch (state.kind) {
    case "current":
      return "current";
    case "compiling":
      return `compiling (${state.done} of ${state.total})`;
    case "updating":
      return state.files === null ? "updating" : `updating (${state.files} files)`;
    case "problems":
      return `problems (${state.count})`;
    case "paused":
      return `paused (${state.changed} files changed)`;
    case "not_compiled":
      return "not compiled";
    case "failed":
      return `failed: ${state.message}`;
    case "resting":
      return `resting (last: ${stateText(state.last)})`;
    case "unavailable":
      return `unavailable: ${state.reason}`;
  }
}

function branchText(git: { readonly branch: string | null; readonly detached: string | null } | null): string | null {
  if (git?.branch) {
    return `branch ${git.branch}`;
  }
  return git?.detached ? `detached at ${git.detached}` : null;
}

function languagesText(languages: readonly Language[]): string {
  return languages.length > 0 ? languages.map((language) => LANGUAGES[language]).join(", ") : "no source files";
}

/** `1 board`, `3 boards`. */
function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

/** One project's row in `list_projects`. */
async function projectLine(context: ToolContext, row: ProjectRow): Promise<string> {
  const [boards, summary] = await Promise.all([
    context.engine("list_boards", { project: row.id }).then(
      (list) => count(list.boards.length, "board"),
      () => null,
    ),
    row.live
      ? context.engine("project_summary", { project: row.id }).then(
          (found) => languagesText(found.languages),
          () => null,
        )
      : Promise.resolve(null),
  ]);
  const facts = [stateText(row.state), summary, branchText(row.git), boards].filter((fact) => fact !== null);
  return `- ${row.name} (id ${row.id}): ${facts.join(" · ")}`;
}

export const listProjects = defineTool({
  name: "list_projects",
  title: "List projects",
  description:
    "Every project in the Unfold workspace: its name, id, state (current, compiling, not compiled, resting, problems, unavailable), languages, branch, and number of boards. Other tools take a project's id or name. A resting project isn't open right now and its languages aren't listed: get_project opens it and tells you everything else.",
  input: z.object({}),
  annotations: { readOnlyHint: true },
  async run(_args, context) {
    const rows = await context.projects();
    if (rows.length === 0) {
      return textResult(context.header(), "The workspace has no projects. Ask the person which folder to add, then call add_project.");
    }
    const lines = await Promise.all(rows.map((row) => projectLine(context, row)));
    return textResult(context.header(), `${count(rows.length, "project")}:`, ...lines, "Next: get_project for one project's applications, settings, and boards.");
  },
});

function graphText(status: GraphStatus): string {
  const watching = { events: "watching for changes", scanning: "watching by scanning", off: "not watching" }[status.watching];
  return [
    status.complete ? "complete" : "incomplete (files that don't parse)",
    watching,
    `background updates ${status.backgroundUpdates ? "on" : "off"}`,
    ...(status.fallback ? [`cache in the fallback location (${status.fallback.replaceAll("_", " ")})`] : []),
  ].join(", ");
}

function applicationLine(application: ApplicationEntrances): string {
  return `- ${application.name} (${application.path}, ${LANGUAGES[application.language]}): ${count(application.total, "entrance")}`;
}

function healthText(health: ProjectSummary["health"]): string[] {
  const facts = [
    count(health.problems, "problem"),
    ...(health.doesntParse > 0 ? [`${count(health.doesntParse, "file")} that don't parse`] : []),
    ...(health.settingsError ? [`settings error: ${health.settingsError}`] : []),
    ...(health.unreachedEntrances > 0 ? [`${count(health.unreachedEntrances, "entrance")} no launch reaches`] : []),
    ...(health.unindexedLaunches > 0 ? [`${count(health.unindexedLaunches, "launch")} into code that isn't indexed`] : []),
  ];
  const boards = health.boards.map((board) => `- ${board.title} (id ${board.id}): ${count(board.items, "item")} to look at`);
  return [`Health: ${facts.join(" · ")}.`, ...(boards.length > 0 ? [`Boards that need review (${health.boardsNeedingReview}):`, ...boards] : [])];
}

/** `none`, or the items joined. */
function list(items: readonly string[]): string {
  return items.length > 0 ? items.join(", ") : "none";
}

function settingsText(config: ProjectConfig): string[] {
  const name = `name ${config.name.value}${config.name.isDefault ? " (the default)" : ""}`;
  if (config.error) {
    const at = config.error.line === null ? "" : ` (line ${config.error.line})`;
    return [`Settings (.unfold/config.toml): ${name}. The file is invalid${at}: ${config.error.message}`];
  }
  const settings = config.settings;
  if (!settings) {
    return [`Settings: ${name}. No .unfold/config.toml yet: everything is the default.`];
  }
  const entrances = settings.entrances.map(
    (entrance) => `- manual entrance ${entrance.label ?? entrance.trigger ?? entrance.kind} → ${entrance.handler.join(", ")} (line ${entrance.line})`,
  );
  return [
    `Settings (.unfold/config.toml): ${name}; languages ${settings.languages ? languagesText(settings.languages) : "all"}; include ${list(settings.include)}; exclude ${list(settings.exclude)}; detectors off ${list(settings.disabledDetectors)}; Python projects ${list(settings.pythonProjects.map((python) => python.root || "."))}; evidence files ${list(settings.evidence)}; ${count(settings.entrances.length, "manual entrance")}.`,
    ...entrances,
  ];
}

function boardLine(board: BoardSummary): string {
  const facts = [
    board.place === "draft" ? "draft" : board.changes > 0 ? `published, ${count(board.changes, "unpublished change")}` : "published",
    count(board.pages, "page"),
    count(board.cards, "card"),
    ...(board.label ? [board.label] : []),
    ...(board.error ? [`can't be read: ${board.error}`] : []),
  ];
  return `- ${board.title ?? board.name} (id ${board.id}): ${facts.join(", ")}`;
}

export const getProject = defineTool({
  name: "get_project",
  title: "Get project",
  description:
    "One project: its state and compile progress, files and languages, applications with their entrance counts, health, settings (scope, detectors, Python projects, manual entrances, evidence files), and boards with their ids. project is a project's id or name (from list_projects) or a link the person pasted. It opens a resting project. Call it again to wait for a compile: the state reads compiling, with its progress, until the project is current.",
  input: z.object({ project: z.string().min(1).describe("The project's id or name, or a link to one of its boards") }),
  annotations: { readOnlyHint: true },
  async run({ project }, context) {
    const row = await context.project(project);
    const [summary, config, boards] = await Promise.all([
      context.engine("project_summary", { project: row.id }),
      context.engine("project_config", { project: row.id }),
      context.engine("list_boards", { project: row.id }),
    ]);
    const compiled = summary.status.state.kind !== "not_compiled";
    const applications = compiled
      ? await context.engine("entrances", { project: row.id, perApplication: 1 }).then(
          (map) => map.applications,
          () => null,
        )
      : null;
    const shown = boards.boards.slice(0, MAX_BOARDS);
    return textResult(
      context.header({ project: summary.name }),
      `${summary.name} (id ${row.id}): ${[stateText(summary.state), branchText(summary.git)].filter((fact) => fact !== null).join(" · ")}`,
      `Folder: ${summary.path}`,
      compiled
        ? `Files: ${summary.files} (${languagesText(summary.languages)}). Graph: ${graphText(summary.status)}. Last committed ${summary.updatedAtMs === null ? "never" : new Date(summary.updatedAtMs).toISOString()}.`
        : "Not compiled yet: compile_project builds its graph (ask the person first, unless they asked you to set things up).",
      applications === null
        ? null
        : `${count(applications.length, "application")}, ${count(summary.entrances, "entrance")} in all:`,
      ...(applications ?? []).map(applicationLine),
      ...healthText(summary.health),
      ...settingsText(config),
      `${count(boards.boards.length, "board")}${shown.length < boards.boards.length ? ` (the first ${shown.length}; list_boards lists them all)` : ""}:`,
      ...shown.map(boardLine),
      "Next: list_entrances to see how it starts, search to find code, or read_page to read a board.",
    );
  },
});
