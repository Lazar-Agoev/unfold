/**
 * Set up: `add_project`, `compile_project`, and `update_project_settings`.
 *
 * Adding a project is the one trusted request an agent can make, and only after the person confirms
 * it in a native dialog; the project compiles as it is added. A compile is waited on for at most 50
 * seconds, and goes on in Unfold after that. Settings edits apply all or nothing and come back as the
 * change to `unfold.toml`, as the engine's diff.
 */

import { homedir } from "node:os";
import { isAbsolute, join, resolve, sep } from "node:path";
import { z } from "zod";
import type { ToolContext } from "../context";
import type { CompileOutcome, ProjectRow, SettingsEdit } from "../engine-types";
import { count, stateText } from "../format";
import { cant, textResult, ToolError } from "../results";
import { defineTool } from "../tool";

/** The longest a tool call waits for a compile. */
const MAX_WAIT_SECONDS = 50;

/** A folder as the person would write it: `~/repos/payments` under their home. */
function shownPath(path: string): string {
  const home = homedir();
  return path === home || path.startsWith(`${home}${sep}`) ? `~${path.slice(home.length)}` : path;
}

/** `~`, `~/…`, or an absolute path, made absolute; null for anything else. */
function absoluteFolder(path: string): string | null {
  const trimmed = path.trim();
  const expanded = trimmed === "~" ? homedir() : trimmed.startsWith("~/") ? join(homedir(), trimmed.slice(2)) : trimmed;
  return isAbsolute(expanded) ? resolve(expanded) : null;
}

/** `work`'s outcome, or `"waited"` after `seconds` or when the call is cancelled; the timer goes once either decides. */
async function within<T>(work: Promise<T>, seconds: number, signal: AbortSignal): Promise<T | "waited"> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let cancelled: (() => void) | undefined;
  const waiting = new Promise<"waited">((done) => {
    timer = setTimeout(() => {
      done("waited");
    }, seconds * 1000);
    cancelled = () => {
      done("waited");
    };
    signal.addEventListener("abort", cancelled, { once: true });
  });
  try {
    return await Promise.race([work, waiting]);
  } finally {
    clearTimeout(timer);
    if (cancelled) {
      signal.removeEventListener("abort", cancelled);
    }
  }
}

/**
 * Compiles `project`, waiting up to `seconds`: what the compile did (`outcomeLine`) and the apps its
 * `unfold.toml` declares, or its progress while it goes on in Unfold.
 */
async function compileText(
  context: ToolContext,
  project: Pick<ProjectRow, "id" | "name">,
  seconds: number,
  outcomeLine: (outcome: CompileOutcome) => string,
): Promise<string[]> {
  const compiling = context.engine("compile", { project: project.id });
  const outcome = await within(compiling, seconds, context.signal);
  if (outcome === "waited") {
    // It goes on in Unfold; whatever it ends with, get_project reads it.
    compiling.catch(() => undefined);
    const summary = await context.engine("project_summary", { project: project.id });
    return [
      `${project.name} is ${summary.state.kind === "compiling" ? stateText(summary.state) : `still compiling (${stateText(summary.state)})`}; the compile goes on in Unfold.`,
      "Next: call get_project to keep waiting: its state reads compiling until the graph is current.",
    ];
  }
  const view = await context.engine("entrances", { project: project.id }).catch(() => null);
  const apps = (view?.apps ?? []).flatMap((app) => (app.name !== undefined ? [app.name] : []));
  return [
    outcomeLine(outcome),
    apps.length > 0 ? `${count(apps.length, "app")}: ${apps.join(", ")}.` : "No apps: its unfold.toml declares none.",
    outcome.problems > 0 ? "get_project lists the problems." : null,
    "Next: list_entrances to see how it starts, or search to find code.",
  ].filter((line): line is string => line !== null);
}

export const addProject = defineTool({
  name: "add_project",
  title: "Add project",
  description:
    'Adds a folder on this machine to the Unfold workspace as a project, after the person confirms it in Unfold: a dialog asks "Claude wants to add ~/repos/payments to your workspace." Call it only when the person asked for that folder. path is absolute or starts with ~/. Unfold reads the folder without running anything and writes only inside its .unfold folder; a folder it can\'t write that into (read-only, or on a network volume) is refused, and the error says so. The project compiles as it is added: the result names the project with its id, and waits up to 50 seconds for the compile, then gives its outcome and the apps its unfold.toml declares, or its progress (get_project keeps waiting). If the person cancels, nothing is added; don\'t ask again unless they bring it up. A folder that already is a project answers with that project. Next: update_project_settings changes what it reads, and compile_project applies that. Agents can\'t remove projects: that is the person\'s, from the project menu\'s Remove from workspace….',
  input: z.strictObject({ path: z.string().min(1).describe("The folder: an absolute path, or one starting with ~/") }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ path }, context) {
    // The confirmation shows the path: a line break in it could make the dialog say something else.
    if (/\p{Cc}/u.test(path)) {
      throw cant("add the folder", "its path holds a control character, such as a line break", "Pass the folder's path as the person wrote it.");
    }
    const folder = absoluteFolder(path);
    if (folder === null) {
      throw cant("add the folder", `“${path}” isn't an absolute path`, "Pass the folder's full path, such as ~/repos/payments or /Users/me/repos/payments.");
    }
    const added = await context.addProject(folder);
    if (!added) {
      return textResult(context.header(), `The person chose not to add ${shownPath(folder)}: nothing was added. Don't ask again unless they bring it up.`);
    }
    const row = (await context.engine("workspace", {})).projects.find((project) => project.id === added.project);
    const name = row?.name ?? shownPath(folder);
    if (added.status === "exists") {
      return textResult(
        context.header({ project: name }),
        `${shownPath(folder)} is already in the workspace as ${name} (id ${added.project}), ${row ? stateText(row.state) : "as it was"}.`,
        "Next: get_project for its state and settings.",
      );
    }
    const addedLine = `Added ${shownPath(folder)} as the project ${name} (id ${added.project})`;
    // The engine compiles the project as it adds it; a compile queued behind that one waits for it
    // and finds nothing left to do, so its outcome counts the files without the changes.
    const compiled = await compileText(context, { id: added.project, name }, MAX_WAIT_SECONDS, (outcome) =>
      `Compiled ${name}: ${count(outcome.files, "file")}${outcome.complete ? "" : ", incomplete (some files don't parse)"}; ${count(outcome.problems, "problem")}.`,
    ).catch((error: unknown) => {
      throw error instanceof ToolError ? new ToolError(`${addedLine}, but compiling it failed. ${error.message}`, error.code) : error;
    });
    return textResult(context.header({ project: name }), `${addedLine}; Unfold compiles a project as it is added.`, ...compiled);
  },
});

export const compileProject = defineTool({
  name: "compile_project",
  title: "Compile project",
  description:
    "Compiles a project: Unfold reads its TypeScript and Python without running anything, brings the graph of entrances, functions, types, and calls up to date with what changed, and keeps it current afterwards. It waits up to waitSeconds (default and at most 50), then returns the outcome: files, changes, problems, and the apps its unfold.toml declares. A longer compile goes on in Unfold: the result gives its progress, and get_project reads the state (compiling, then current) to keep waiting. Compile a project that isn't current before reading it. Settings changes (update_project_settings) apply at the next compile. When a compile fails, the error says why.",
  input: z.strictObject({
    project: z.string().min(1).describe("The project's name or id (from list_projects), or a link to one of its boards"),
    waitSeconds: z.number().int().min(0).max(MAX_WAIT_SECONDS).default(MAX_WAIT_SECONDS).describe("How long to wait for the outcome, 0 to 50 seconds"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ project, waitSeconds }, context) {
    const row = await context.project(project);
    const compiled = await compileText(context, row, waitSeconds, (outcome) =>
      `Compiled ${row.name} in ${(outcome.durationMs / 1000).toFixed(1)} s: ${count(outcome.files, "file")} (${outcome.changedFiles} added or changed, ${outcome.removedFiles} removed)${outcome.entrancesChanged ? ", entrances changed" : ""}${outcome.complete ? "" : ", incomplete (some files don't parse)"}; ${count(outcome.problems, "problem")}.`,
    );
    return textResult(context.header({ project: row.name }), ...compiled);
  },
});

// ---------------------------------------------------------------------------------------------
// Settings

const folderPath = z.string().min(1).describe("A folder or file relative to the project's folder, / separated");

const settingsEdit = z.discriminatedUnion("op", [
  z.strictObject({ op: z.literal("exclude_from_graph"), path: folderPath }),
  z.strictObject({ op: z.literal("include_in_graph"), path: folderPath }),
  z.strictObject({ op: z.literal("graph_only"), path: folderPath.describe("The one folder to graph") }),
  z.strictObject({ op: z.literal("remove_scope_pattern"), list: z.enum(["include", "exclude"]), pattern: z.string().min(1).describe("The pattern exactly as get_project lists it") }),
  z.strictObject({
    op: z.literal("set_python_project"),
    root: folderPath.describe("The Python project's folder"),
    sourceRoots: z.array(z.string().min(1)).optional().describe("Import roots relative to root"),
    version: z.string().min(1).optional().describe("The Python version, such as 3.12"),
  }),
  z.strictObject({ op: z.literal("remove_python_project"), root: folderPath }),
  z.strictObject({ op: z.literal("add_evidence_pattern"), pattern: z.string().min(1).describe("A file, folder, or glob relative to the project's folder") }),
  z.strictObject({ op: z.literal("remove_evidence_pattern"), pattern: z.string().min(1).describe("The pattern exactly as get_project lists it") }),
]);

type Edit = z.output<typeof settingsEdit>;

/** An agent's edit as the engine takes it. */
function engineEdit(edit: Edit): SettingsEdit {
  return edit.op === "set_python_project" ? { op: edit.op, project: { root: edit.root, sourceRoots: edit.sourceRoots ?? null, version: edit.version ?? null } } : edit;
}

export const updateProjectSettings = defineTool({
  name: "update_project_settings",
  title: "Update project settings",
  description:
    "Changes a project's settings, all or nothing, and returns the change to unfold.toml as a diff. Settings shape the graph and are shared through the repository, so change them only when the person asks, and say in chat that the file changed (it shows in their git diff). edits apply in order: scope (exclude_from_graph, include_in_graph, graph_only one folder, remove_scope_pattern exactly as listed); Python projects (set_python_project with its root, optional sourceRoots and version; remove_python_project); evidence files (add_evidence_pattern, remove_evidence_pattern: files Unfold doesn't compile but boards may cite by lines, such as a migration or a Prisma schema; then read_file reads them). Apps and entrances have their own tools: set_app, set_entrance, remove_entrance. An edit that would make the file invalid, or that needs a broader rule changed, is refused with the reason, and nothing changes. The graph follows at the next compile_project or background update. get_project shows the settings now. Agents can't change Unfold's Settings. Read get_guide(\"setup\") first.",
  input: z.strictObject({
    project: z.string().min(1).describe("The project's name or id, or a link to one of its boards"),
    edits: z.array(settingsEdit).min(1).describe("The changes, in order"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ project, edits }, context) {
    const row = await context.project(project);
    const { diff } = await context.engine("update_project_config", { project: row.id, edits: edits.map(engineEdit) });
    return textResult(
      context.header({ project: row.name }),
      diff.length > 0 ? `Changed unfold.toml of ${row.name}:\n${["```diff", diff.trimEnd(), "```"].join("\n")}` : "Nothing in unfold.toml changed: the settings already said that.",
      diff.length > 0 ? "The file is shared through the repository: tell the person it changed. The graph follows at the next compile_project or background update." : null,
    );
  },
});
