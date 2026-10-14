/**
 * Declare apps and entrances (mcp-capabilities.md § Apps and entrances; entrance-definitions.md):
 * `set_app`, `set_entrance`, and `remove_entrance` write the `[[app]]` tables of a project's
 * `unfold.toml` through `update_project_config`, and return the change as a diff.
 *
 * Apps and entrances are named by their names or keys, found in what `entrances` lists. A field left
 * out keeps what the table says; null removes it. Writing them never recompiles: Unfold looks up the
 * code keys again. `unfold.toml` is shared, so agents write it only when the person asks.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { AppView, EntranceView, ProjectRow, SettingsEdit } from "../engine-types";
import { cant, textResult, type ToolResult } from "../results";
import { defineTool } from "../tool";

const SHARED = "unfold.toml is shared through the repository: write apps and entrances only when the person asks, and tell them the file changed.";

/** Whether `name` names an entry: its key, or its name in any case. */
function names(entry: { readonly key?: string; readonly name?: string }, name: string): boolean {
  return entry.key === name || entry.name?.toLowerCase() === name.trim().toLowerCase();
}

/** The project's apps, as its `unfold.toml` declares them. */
async function appsOf(context: ToolContext, project: ProjectRow): Promise<readonly AppView[]> {
  return (await context.engine("entrances", { project: project.id })).apps;
}

/** The app `name` names (its name or key), if the file declares it. */
function findApp(apps: readonly AppView[], name: string): AppView | undefined {
  return apps.find((app) => names(app, name));
}

/** A field as an edit writes it: the value given, null to remove it, or (left out) what the table says. */
function kept(given: string | null | undefined, written: string | undefined): string | null {
  return given === undefined ? (written ?? null) : given;
}

/** Applies one edit to `unfold.toml` and answers with its diff. */
async function write(context: ToolContext, project: ProjectRow, edit: SettingsEdit, done: string): Promise<ToolResult> {
  const { diff } = await context.engine("update_project_config", { project: project.id, edits: [edit] });
  return textResult(
    context.header({ project: project.name }),
    diff.length > 0 ? `${done} Changed unfold.toml of ${project.name}:\n${["```diff", diff.trimEnd(), "```"].join("\n")}` : "Nothing in unfold.toml changed: it already said that.",
    diff.length > 0 ? `${SHARED} Unfold looks up the code keys again without compiling.` : null,
    "Next: list_entrances shows whether each code is found, with a fix when one is missing.",
  );
}

const projectArg = z.string().min(1).describe("The project's name or id, or a link to one of its boards");
const appArg = z.string().min(1).describe("The app's name or key (list_entrances lists them)");
const field = (what: string) => z.string().trim().min(1).max(400).nullable().optional().describe(`${what}; null removes it; left out keeps it`);

export const setApp = defineTool({
  name: "set_app",
  title: "Set app",
  description: `Adds an app to a project's unfold.toml (an [[app]] table), or changes an existing app's run, code, or about. app is its name (a new app's heading on Home) or the key of an existing one. run is the command that starts it, as written in package.json, pyproject.toml, a Procfile, or a Dockerfile (Unfold never runs it); code is the key of the block where that command starts, usually a module (src/server.ts#), from search or read_code; about is one sentence for its card. A field left out keeps what the file says; null removes it. Its entrances stay (set_entrance writes them). ${SHARED} It never recompiles: Unfold looks up the keys again; list_entrances then shows whether code is found. Returns the change to unfold.toml as a diff. Read get_guide("setup") first.`,
  input: z.object({
    project: projectArg,
    app: appArg,
    run: field("The command that starts it"),
    code: field("The key of the block where that command starts"),
    about: field("One sentence for its card"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ project, app, run, code, about }, context) {
    const row = await context.project(project);
    const existing = findApp(await appsOf(context, row), app);
    const name = existing?.name ?? app.trim();
    const edit: SettingsEdit = {
      op: "set_app",
      app: { name, run: kept(run, existing?.run), code: kept(code, existing?.code?.key), about: kept(about, existing?.about) },
    };
    return write(context, row, edit, existing ? `Changed the app ${name}.` : `Added the app ${name}.`);
  },
});

/** The entrance `name` names in `app` (its name or key), if the file declares it. */
function findEntrance(app: AppView | undefined, name: string): EntranceView | undefined {
  return app?.entrances.find((entrance) => names(entrance, name));
}

export const setEntrance = defineTool({
  name: "set_entrance",
  title: "Set entrance",
  description: `Adds one entrance to an app in a project's unfold.toml, creating the app when it is new, or changes an entrance the app has. app is the app's name or key; name is the entrance's name, what a person calls it ("Place an order"), or an existing one's key. on is what starts it, as written: POST /orders/{id}, queue emails, every 5 minutes (display text; Unfold never parses it). code is the block it starts at, or a list of blocks in the order they run: a module, a definition, or an inline handler, by key (search, read_code), never a call, branch, or exit; a new entrance needs it. about is one sentence for its card. A field left out keeps what the file says; null removes on or about. ${SHARED} It never recompiles: Unfold looks up the keys again; list_entrances then shows whether each code is found. Returns the change as a diff. Read get_guide("setup") first.`,
  input: z.object({
    project: projectArg,
    app: appArg,
    name: z.string().trim().min(1).max(200).describe("The entrance's name, or an existing one's key"),
    on: field("What starts it, as written"),
    code: z
      .union([z.string().min(1), z.array(z.string().min(1)).min(1)])
      .optional()
      .describe("The key of the block it starts at, or a list of keys in the order they run (needed for a new entrance)"),
    about: field("One sentence for its card"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ project, app, name, on, code, about }, context) {
    const row = await context.project(project);
    const apps = await appsOf(context, row);
    const owner = findApp(apps, app);
    const existing = findEntrance(owner, name);
    const keys = code === undefined ? (existing?.code.map((target) => target.key) ?? []) : typeof code === "string" ? [code] : code;
    if (keys.length === 0) {
      throw cant("add the entrance", `${name} is new, and an entrance needs code: the block it starts at`, "Pass code: a module's, definition's, or inline handler's key, from search or read_code.");
    }
    const entranceName = existing?.name ?? name;
    const appName = owner?.name ?? app.trim();
    const edit: SettingsEdit = {
      op: "set_entrance",
      app: appName,
      entrance: { name: entranceName, on: kept(on, existing?.on), code: keys, about: kept(about, existing?.about) },
    };
    const done = existing ? `Changed the entrance ${entranceName} of ${appName}.` : `Added the entrance ${entranceName} to ${owner ? appName : `the new app ${appName}`}.`;
    return write(context, row, edit, done);
  },
});

export const removeEntrance = defineTool({
  name: "remove_entrance",
  title: "Remove entrance",
  description: `Removes one entrance from an app in a project's unfold.toml, or, without entrance, the whole app with its entrances. app and entrance are names or keys (list_entrances lists them). Boards that show a removed entrance find it missing, and offer the entrance with the same code if there is one. ${SHARED} It never recompiles. Returns the change as a diff.`,
  input: z.object({
    project: projectArg,
    app: appArg,
    entrance: z.string().min(1).optional().describe("The entrance's name or key (absent: the whole app)"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ project, app, entrance }, context) {
    const row = await context.project(project);
    const apps = await appsOf(context, row);
    const owner = findApp(apps, app);
    if (!owner?.name) {
      throw cant("remove it", `${row.name} declares no app “${app}”`, `Its apps: ${apps.map((item) => item.name ?? item.key ?? "(no name)").join(", ") || "none"}.`);
    }
    if (entrance === undefined) {
      return write(context, row, { op: "remove_entrance", app: owner.name, entrance: null }, `Removed the app ${owner.name} and its entrances.`);
    }
    const found = findEntrance(owner, entrance);
    if (!found?.name) {
      throw cant(
        "remove the entrance",
        `the app ${owner.name} has no entrance “${entrance}”`,
        `Its entrances: ${owner.entrances.map((item) => item.name ?? item.key ?? "(no name)").join(", ") || "none"}.`,
      );
    }
    return write(context, row, { op: "remove_entrance", app: owner.name, entrance: found.name }, `Removed the entrance ${found.name} from ${owner.name}.`);
  },
});
