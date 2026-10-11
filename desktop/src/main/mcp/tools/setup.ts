/**
 * Set up (design §6.3): `add_project`, `compile_project`, and `update_project_settings`.
 *
 * Adding a project is the one trusted request an agent can make, and only after the person confirms
 * it in a native dialog; it reports what a first Compile would read and skip. A compile is waited on
 * for at most 50 seconds, and goes on in Unfold after that. Settings edits apply all or nothing and
 * come back as the change to `unfold.toml`, read before and after.
 */

import { homedir } from "node:os";
import { isAbsolute, join, resolve, sep } from "node:path";
import { z } from "zod";
import type { ToolContext } from "../context";
import { lineDiff } from "../diff";
import type { CompileOutcome, CompilePreview, Language, ProjectConfig, ProjectRow, SettingsEdit } from "../engine-types";
import { count, stateText } from "../format";
import { cant, textResult, ToolError } from "../results";
import { defineTool } from "../tool";

const LANGUAGES: Readonly<Record<Language, string>> = { typescript: "TypeScript", python: "Python" };

/** The longest a tool call waits for a compile (design §6.11). */
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

/** What a first Compile would read and skip, in a few lines. */
function previewText(preview: CompilePreview): string[] {
  const languages = preview.languages.map((entry) => `${entry.files} ${LANGUAGES[entry.language]}`).join(", ");
  const skipped = preview.skipped.map((folder) => `${folder.folder || "."} (${folder.reasonText}${folder.wholeFolder ? "" : `, ${count(folder.files, "file")}`})`);
  return [
    `A first Compile would read ${count(preview.files, "file")}${languages.length > 0 ? ` (${languages})` : ""}${preview.unindexed > 0 ? ` and skip ${count(preview.unindexed, "source-like file")}` : ""}.`,
    skipped.length > 0 ? `Skipped: ${skipped.join("; ")}.` : null,
    preview.configExists ? (preview.settingsError ? `Its unfold.toml is invalid: ${preview.settingsError}` : "It has an unfold.toml.") : "It has no unfold.toml yet: every setting is the default.",
    preview.fallback ? `Its graph would live outside the folder (${preview.fallback.replaceAll("_", " ")}): ${preview.localPath}.` : null,
    ...preview.problems.map((problem) => `Problem: ${problem}`),
  ].filter((line): line is string => line !== null);
}

export const addProject = defineTool({
  name: "add_project",
  title: "Add project",
  description:
    'Adds a folder on this machine to the Unfold workspace as a project, after the person confirms it in Unfold: a dialog asks "Claude wants to add ~/repos/payments to your workspace." Call it only when the person asked for that folder. path is absolute or starts with ~/. Unfold reads the folder without running anything and writes only inside its .unfold folder. The result names the project with its id, and what a first Compile would read and skip: files by language, skipped folders and why, the settings file. If the person cancels, nothing is added; don\'t ask again unless they bring it up. A folder that already is a project (or another worktree of its repository) answers with that project. Next: compile_project builds its graph; update_project_settings changes what it reads first. Agents can\'t remove projects: that is the person\'s, from Unfold\'s File menu.',
  input: z.object({ path: z.string().min(1).describe("The folder: an absolute path, or one starting with ~/") }),
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
    const preview = await context.engine("compile_preview", { project: added.project });
    return textResult(
      context.header({ project: name }),
      `Added ${shownPath(folder)} as the project ${name} (id ${added.project}). Nothing is compiled yet.`,
      ...previewText(preview),
      "Next: compile_project builds its graph (ask the person first, unless they asked you to set it up). To leave folders out or turn detectors off, update_project_settings first.",
    );
  },
});

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

/** What a finished compile did, and the applications it found. */
async function compiledText(context: ToolContext, row: ProjectRow, outcome: CompileOutcome): Promise<string[]> {
  const view = await context.engine("entrances", { project: row.id }).catch(() => null);
  const applications = view?.view === "overview" ? view.categories.flatMap((group) => group.applications.map((application) => application.name)) : [];
  return [
    `Compiled ${row.name} in ${(outcome.durationMs / 1000).toFixed(1)} s: ${count(outcome.files, "file")} (${outcome.changedFiles} added or changed, ${outcome.removedFiles} removed), ${count(outcome.changedEntrances, "entrance")} changed${outcome.complete ? "" : ", incomplete (some files don't parse)"}; ${count(outcome.problems, "problem")}.`,
    applications.length > 0 ? `${count(applications.length, "application")}: ${applications.join(", ")}.` : "No applications: nothing Unfold detects starts it.",
    outcome.problems > 0 ? "get_project lists the problems." : null,
    "Next: list_entrances to see how it starts, or search to find code.",
  ].filter((line): line is string => line !== null);
}

export const compileProject = defineTool({
  name: "compile_project",
  title: "Compile project",
  description:
    'Compiles a project: Unfold reads its TypeScript and Python without running anything, builds the graph of entrances, functions, types, and calls, and keeps it current afterwards. mode "compile" (the default) updates what changed; "rebuild" starts from scratch (only when the person asks, or the graph looks wrong). It waits up to waitSeconds (default and at most 50), then returns the outcome: files, changes, problems, and the applications found. A longer compile goes on in Unfold: the result gives its progress, and get_project reads the state (compiling, then current) to keep waiting. Compile a project that isn\'t compiled or current before reading it; the first Compile creates its .unfold folder. Settings changes (update_project_settings) apply at the next compile. When a compile fails, the error says why.',
  input: z.object({
    project: z.string().min(1).describe("The project's name or id (from list_projects), or a link to one of its boards"),
    mode: z.enum(["compile", "rebuild"]).default("compile").describe("compile updates what changed; rebuild starts from scratch"),
    waitSeconds: z.number().int().min(0).max(MAX_WAIT_SECONDS).default(MAX_WAIT_SECONDS).describe("How long to wait for the outcome, 0 to 50 seconds"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ project, mode, waitSeconds }, context) {
    const row = await context.project(project);
    const compiling = context.engine("compile", { project: row.id, mode: mode === "rebuild" ? "from_scratch" : "check" });
    const outcome = await within(compiling, waitSeconds, context.signal);
    if (outcome !== "waited") {
      return textResult(context.header({ project: row.name }), ...(await compiledText(context, row, outcome)));
    }
    // It goes on in Unfold; whatever it ends with, get_project reads it.
    compiling.catch(() => undefined);
    const summary = await context.engine("project_summary", { project: row.id });
    return textResult(
      context.header({ project: row.name }),
      `${row.name} is ${summary.state.kind === "compiling" ? stateText(summary.state) : `still compiling (${stateText(summary.state)})`}; ${mode === "rebuild" ? "the rebuild" : "the compile"} goes on in Unfold.`,
      "Next: call get_project to keep waiting: its state reads compiling until the graph is current.",
    );
  },
});

// ---------------------------------------------------------------------------------------------
// Settings

const folderPath = z.string().min(1).describe("A folder or file relative to the project's folder, / separated");

const entranceId = z.string().min(1).describe("The manual entrance's id, as get_project lists it");

const entranceSpec = z.object({
  id: z.string().min(1).describe("A short id, unique among the manual entrances, such as import-orders: letters, digits, ., _, or -"),
  kind: z.enum(["process", "command", "http", "worker", "schedule", "event"]).describe("What starts it"),
  trigger: z.string().min(1).optional().describe("The path, queue, schedule, command, or event name"),
  method: z.string().min(1).optional().describe("The HTTP method (http only)"),
  label: z.string().min(1).optional().describe("A display label"),
  handler: z.array(z.string().min(1)).default([]).describe("Handler locators, middleware first: src/jobs/import.ts#importOrders or src/cli.ts:12 (every kind but process)"),
  startup: z.string().min(1).optional().describe("The startup locator (process only)"),
});

const settingsEdit = z.discriminatedUnion("op", [
  z.object({ op: z.literal("exclude_from_graph"), path: folderPath }),
  z.object({ op: z.literal("include_in_graph"), path: folderPath }),
  z.object({ op: z.literal("graph_only"), path: folderPath.describe("The one folder to graph") }),
  z.object({ op: z.literal("remove_scope_pattern"), list: z.enum(["include", "exclude"]), pattern: z.string().min(1).describe("The pattern exactly as get_project lists it") }),
  z.object({ op: z.literal("add_entrance"), entrance: entranceSpec }),
  z.object({ op: z.literal("replace_entrance"), id: entranceId, entrance: entranceSpec }),
  z.object({ op: z.literal("remove_entrance"), id: entranceId }),
  z.object({
    op: z.literal("set_python_project"),
    root: folderPath.describe("The Python project's folder"),
    sourceRoots: z.array(z.string().min(1)).optional().describe("Import roots relative to root"),
    version: z.string().min(1).optional().describe("The Python version, such as 3.12"),
  }),
  z.object({ op: z.literal("remove_python_project"), root: folderPath }),
  z.object({ op: z.literal("set_detector"), detector: z.string().min(1).describe("The detector's id, such as node-cron.schedule"), enabled: z.boolean() }),
  z.object({ op: z.literal("set_project_name"), name: z.string().min(1).nullable().describe("The shared name; null for the folder's name") }),
  z.object({ op: z.literal("add_evidence_pattern"), pattern: z.string().min(1).describe("A file, folder, or glob relative to the project's folder") }),
  z.object({ op: z.literal("remove_evidence_pattern"), pattern: z.string().min(1).describe("The pattern exactly as get_project lists it") }),
  z.object({ op: z.literal("set_background_updates"), on: z.boolean().describe("Whether the graph updates itself as files change") }),
]);

type Edit = z.output<typeof settingsEdit>;

/** An agent's edit as the engine takes it (`set_background_updates` isn't one: it lives outside the file). */
function engineEdit(edit: Exclude<Edit, { op: "set_background_updates" }>): SettingsEdit {
  switch (edit.op) {
    case "add_entrance":
    case "replace_entrance": {
      const { id, kind, trigger, method, label, handler, startup } = edit.entrance;
      const entrance = {
        id,
        kind,
        application: null,
        trigger: trigger ?? null,
        method: method ?? null,
        label: label ?? null,
        description: null,
        handler,
        cwd: null,
        command: null,
        startup: startup ?? null,
        target: null,
        pythonPaths: null,
        env: {},
        schedule: null,
      };
      return edit.op === "add_entrance" ? { op: edit.op, entrance } : { op: edit.op, id: edit.id, entrance };
    }
    case "set_python_project":
      return { op: edit.op, project: { root: edit.root, sourceRoots: edit.sourceRoots ?? null, version: edit.version ?? null } };
    default:
      return edit;
  }
}

function configText(config: ProjectConfig): string {
  return config.exists ? (config.text ?? "") : "";
}

export const updateProjectSettings = defineTool({
  name: "update_project_settings",
  title: "Update project settings",
  description:
    "Changes a project's settings, all or nothing, and returns the change to unfold.toml as a diff. Settings shape the graph and are shared through the repository, so change them only when the person asks, and say in chat that the file changed (it shows in their git diff). edits apply in order: scope (exclude_from_graph, include_in_graph, graph_only one folder, remove_scope_pattern exactly as listed); detectors (set_detector by id, such as node-cron.schedule); Python projects (set_python_project with its root, optional sourceRoots and version; remove_python_project); manual entrances (add_entrance with a new id; replace_entrance and remove_entrance by the id get_project lists); the project's shared name (set_project_name, null for the folder's name); evidence files (add_evidence_pattern, remove_evidence_pattern: files Unfold doesn't compile but boards may cite by lines, such as a migration or a Prisma schema; then read_file reads them); and set_background_updates (on or off, kept outside the file). An edit that would make the file invalid, or that needs a broader rule changed, is refused with the reason, and nothing changes. The graph follows at the next compile_project or background update. get_project shows the settings now. Agents can't change the app's preferences. Read get_guide(\"setup\") first.",
  input: z.object({
    project: z.string().min(1).describe("The project's name or id, or a link to one of its boards"),
    edits: z.array(settingsEdit).min(1).describe("The changes, in order"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ project, edits }, context) {
    const row = await context.project(project);
    const background = edits.flatMap((edit) => (edit.op === "set_background_updates" ? [edit.on] : []));
    const fileEdits = edits.flatMap((edit) => (edit.op === "set_background_updates" ? [] : [engineEdit(edit)]));
    if (background.length > 1) {
      throw cant("change the settings", "edits sets background updates more than once", "Pass set_background_updates once.");
    }
    const before = await context.engine("project_config", { project: row.id });
    const after = fileEdits.length > 0 ? await context.engine("update_project_config", { project: row.id, edits: fileEdits }) : before;
    const [on] = background;
    const updates = on === undefined ? null : await context.engine("set_background_updates", { project: row.id, on }).catch((error: unknown) => {
      throw error instanceof ToolError
        ? new ToolError(`${error.message} The settings file changed${fileEdits.length > 0 ? " as asked" : " (nothing)"}, but background updates didn't.`, error.code)
        : error;
    });
    const diff = lineDiff(configText(before), configText(after));
    return textResult(
      context.header({ project: after.name.value }),
      fileEdits.length === 0
        ? null
        : diff.length > 0
          ? `Changed unfold.toml of ${row.name}:\n${["```diff", ...diff, "```"].join("\n")}`
          : "Nothing in unfold.toml changed: the settings already said that.",
      updates === null ? null : `Background updates are ${updates.backgroundUpdates ? "on" : "off"} (kept in Unfold, not in the file).`,
      after.name.value !== row.name ? `The project is now named ${after.name.value}; links and other tools use the new name.` : null,
      diff.length > 0 ? "The file is shared through the repository: tell the person it changed. The graph follows at the next compile_project or background update." : null,
    );
  },
});
