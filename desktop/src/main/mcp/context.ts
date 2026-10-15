/**
 * What a tool works with: who is calling, the agent bridge (edits, reads, and opens go
 * to the window that holds the board), the window's view, links, page revisions, the render window
 * (page images and checks), and the engine. The engine's commands go untrusted, as a window's do: an
 * agent reaches only the fixed actions its tools take, and nothing that takes a path outside a
 * project, except adding a project after the person confirms it in Unfold.
 */

import type { AgentBoardRef, AgentIdentity, AgentPage } from "../../shared/agent-api";
import type { AddedProject } from "../../shared/unfold-api";
import type { AgentBridge } from "../agents/bridge";
import { parseLink, projectsNamed, resolveLink, sharedName, type ResolvedLink } from "../agents/links";
import type { PageRenderer } from "../agents/render";
import { pageRevision } from "../../shared/revision";
import type { ViewTracker } from "../agents/views";
import { EngineRequestError } from "../engine-host";
import type { CommandArgs, CommandName, CommandResult, ProjectRow } from "./engine-types";
import { clip } from "./format";
import { cant, header, ToolError, windowRefusal, type ResultPlace } from "./results";

/** What tools ask of the render window: page images and page checks. */
export type PageImages = Pick<PageRenderer, "screenshot" | "check" | "checkBoard">;

/** What the MCP server needs of the app. */
export interface McpServices {
  /** Routes reads, edits, opens, and finishes to the window. */
  readonly bridge: AgentBridge;
  /** Draws pages for images and runs the page checks. */
  readonly renderer: PageImages;
  /** Runs one engine command for an agent: untrusted, for no window. Rejects with an `EngineRequestError`. */
  readonly engine: (command: string, args: Readonly<Record<string, unknown>>) => Promise<unknown>;
  /**
   * Runs one engine command for an agent as window `windowId` (untrusted, as that window's would be):
   * a command on a board that window holds. Rejects with an `EngineRequestError`.
   */
  readonly engineAs: (windowId: number, command: string, args: Readonly<Record<string, unknown>>) => Promise<unknown>;
  /** Asks the person whether `agent` may add the folder `path`; resolves with whether they agreed. */
  readonly confirmAddProject: (agent: AgentIdentity, path: string) => Promise<boolean>;
  /** Adds a folder the person agreed to as a project (the trusted `add_project`). */
  readonly addProject: (path: string) => Promise<AddedProject>;
}

/** Everything a tool's `run` gets. */
export interface ToolContext {
  /** Who is calling. Edits are signed with it. */
  readonly agent: AgentIdentity;
  /** Edits, reads of held boards, background opens, and finishes, through the window that holds the board. */
  readonly bridge: AgentBridge;
  /** What the window shows, as it last reported it. */
  readonly views: ViewTracker;
  /** Page images and page checks, drawn in the render window. */
  readonly renderer: PageImages;
  /** Aborted when the agent cancels the call. */
  readonly signal: AbortSignal;
  /** Runs an engine command, untrusted. A failure is a {@link ToolError} that says what to do. */
  engine<C extends CommandName>(command: C, args: CommandArgs<C>): Promise<CommandResult<C>>;
  /**
   * Runs an engine command that needs the board's session (`board_health`) as the window that holds
   * the board, loading its session there first.
   */
  held<C extends CommandName>(board: AgentBoardRef, command: C, args: CommandArgs<C>): Promise<CommandResult<C>>;
  /** The workspace's projects, in its order (read once per call). */
  projects(): Promise<readonly ProjectRow[]>;
  /**
   * A project by id, by its folder's name (refused when several projects' folders have it), or by a
   * link to one of its boards.
   */
  project(reference: string): Promise<ProjectRow>;
  /** A link the person pasted, with its project resolved to an id. */
  link(text: string): Promise<ResolvedLink>;
  /** A page's revision, for result headers and `expectRevision`. */
  revision(page: AgentPage): string;
  /**
   * Asks the person to confirm in Unfold ("Claude wants to add `~/repos/payments` to your
   * workspace."), then adds the folder. Null when they cancel.
   */
  addProject(path: string): Promise<AddedProject | null>;
  /** The result header for this agent. */
  header(place?: ResultPlace): string;
}

/** What to do about an engine failure, by its code. */
const ENGINE_HINTS: Readonly<Record<string, string>> = {
  engine_unavailable: "Unfold's engine isn't running. Ask the person to check Unfold, then try again.",
  disconnected: "Unfold's engine restarted. Try again.",
  unknown_project: "list_projects shows each project's name and id.",
  not_compiled: "Call compile_project first (ask the person, unless they asked you to set things up).",
  untrusted: "Agents can't do that.",
};

/** An engine failure as a refusal that teaches: the engine's reason, then what to do. */
function engineError(error: unknown): unknown {
  if (!(error instanceof EngineRequestError)) {
    return error;
  }
  const { code, message } = error.error;
  const reason = message.charAt(0).toUpperCase() + message.slice(1).replace(/\.?$/, ".");
  const hint = ENGINE_HINTS[code];
  return new ToolError(hint === undefined ? reason : `${reason} ${hint}`, code);
}

/** The context of one tool call. */
export function toolContext(services: McpServices, agent: AgentIdentity, signal: AbortSignal): ToolContext {
  let projects: Promise<readonly ProjectRow[]> | null = null;
  const context: ToolContext = {
    agent,
    bridge: services.bridge,
    views: services.bridge.views,
    renderer: services.renderer,
    signal,
    async engine(command, args) {
      try {
        return (await services.engine(command, args)) as CommandResult<typeof command>;
      } catch (error) {
        throw engineError(error);
      }
    },
    async held(board, command, args) {
      const opened = await services.bridge.open({ board });
      if (!opened.ok) {
        throw windowRefusal(opened);
      }
      try {
        return (await services.engineAs(opened.windowId, command, args)) as CommandResult<typeof command>;
      } catch (error) {
        throw engineError(error);
      }
    },
    projects() {
      projects ??= context.engine("workspace", {}).then((view) => view.projects);
      return projects;
    },
    async project(reference) {
      const rows = await context.projects();
      const named = reference.trim().toLowerCase().startsWith("unfold://") ? (await context.link(reference)).project : reference.trim();
      const [row, ...others] = projectsNamed(rows, named);
      if (!row) {
        throw cant("find the project", `no project named or with the id “${clip(named, 60)}” is in the workspace`, "list_projects shows each project's name and id.");
      }
      if (others.length > 0) {
        throw cant("tell which project you mean", sharedName(named, [row, ...others]), "Pass the project's id.");
      }
      return row;
    },
    async link(text) {
      const parsed = parseLink(text);
      if (!parsed.ok) {
        throw new ToolError(parsed.message);
      }
      const resolved = resolveLink(parsed.link, await context.projects());
      if (!resolved.ok) {
        throw new ToolError(resolved.message);
      }
      return resolved.link;
    },
    revision: pageRevision,
    async addProject(path) {
      if (!(await services.confirmAddProject(agent, path))) {
        return null;
      }
      try {
        return await services.addProject(path);
      } catch (error) {
        throw engineError(error);
      }
    },
    header: (place) => header(agent, place),
  };
  return context;
}
