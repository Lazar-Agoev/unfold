/**
 * Declare apps and entrances (mcp-capabilities.md § Apps and entrances; entrance-definitions.md):
 * `set_app`, `set_entrance`, and `remove_entrance` write the `[[app]]` tables of a project's
 * `unfold.toml` through `update_project_config`, and return the change as a diff.
 *
 * Each sends what the agent gave: the engine finds the app or entrance by its name or key, keeps a
 * field left out, removes one given null, and says whether it added, changed, or removed the entry.
 * Writing them never recompiles: Unfold looks up the code keys again. `unfold.toml` is shared, so
 * agents write it only when the person asks.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { EntryEdit, ProjectRow, SettingsEdit } from "../engine-types";
import { textResult, type ToolResult } from "../results";
import { defineTool } from "../tool";

const SHARED = "unfold.toml is shared through the repository: write apps and entrances only when the person asks, and tell them the file changed.";

/** The fields the agent gave, without those left out (which the engine keeps as the file says). */
function given<T extends object>(fields: T): { [K in keyof T]?: Exclude<T[K], undefined> } {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined)) as { [K in keyof T]?: Exclude<T[K], undefined> };
}

/** What the engine says the edit did, in a sentence. */
function said({ change, app, entrance, newApp }: EntryEdit): string {
  if (entrance === undefined) {
    switch (change) {
      case "added":
        return `Added the app ${app}.`;
      case "changed":
        return `Changed the app ${app}.`;
      case "removed":
        return `Removed the app ${app} and its entrances.`;
    }
  }
  switch (change) {
    case "added":
      return `Added the entrance ${entrance} to ${newApp ? `the new app ${app}` : app}.`;
    case "changed":
      return `Changed the entrance ${entrance} of ${app}.`;
    case "removed":
      return `Removed the entrance ${entrance} from ${app}.`;
  }
}

/** Applies one edit to `unfold.toml` and answers with what it did and its diff. */
async function write(context: ToolContext, project: ProjectRow, edit: SettingsEdit): Promise<ToolResult> {
  const { diff, entries } = await context.engine("update_project_config", { project: project.id, edits: [edit] });
  const done = entries.map(said).join(" ");
  return textResult(
    context.header({ project: project.name }),
    diff.length > 0 ? `${done} Changed unfold.toml of ${project.name}:\n${["```diff", diff.trimEnd(), "```"].join("\n")}` : "Nothing in unfold.toml changed: it already said that.",
    diff.length > 0 ? `${SHARED} Unfold looks up the code keys again without compiling.` : null,
    "Next: list_entrances shows whether each code is found, with a fix when one is missing.",
  );
}

const projectArg = z.string().min(1).describe("The project's name or id, or a link to one of its boards");
const appArg = z.string().trim().min(1).max(200).describe("The app's name or key (list_entrances lists them)");
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
    return write(context, await context.project(project), { op: "set_app", app: { name: app, ...given({ run, code, about }) } });
  },
});

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
    const keys = typeof code === "string" ? [code] : code;
    return write(context, await context.project(project), { op: "set_entrance", app, entrance: { name, ...given({ on, code: keys, about }) } });
  },
});

export const removeEntrance = defineTool({
  name: "remove_entrance",
  title: "Remove entrance",
  description: `Removes one entrance from an app in a project's unfold.toml, or, without entrance, the whole app with its entrances. app and entrance are names or keys (list_entrances lists them). Boards that show a removed entrance find it missing, and offer the entrance with the same code if there is one. ${SHARED} It never recompiles. Returns the change as a diff.`,
  input: z.object({
    project: projectArg,
    app: appArg,
    entrance: z.string().trim().min(1).max(200).optional().describe("The entrance's name or key (absent: the whole app)"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ project, app, entrance }, context) {
    return write(context, await context.project(project), { op: "remove_entrance", app, entrance: entrance ?? null });
  },
});
