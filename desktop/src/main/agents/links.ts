/**
 * Links: `unfold://<project>/<board id>?page=<page id>&item=<item id>`, which the window's Copy link
 * builds and a person pastes into an agent's chat. `<project>` is the project's folder name, or its
 * id when another project's folder has the same name (two worktrees of one repository). Only the
 * main process reads links: it parses the text, then resolves the project against the workspace,
 * by id, else by a folder name exactly one project has. A name several projects share is refused,
 * never guessed. A link names things and carries no code.
 */

import type { AgentLink } from "../../shared/agent-api";
import { clip } from "../mcp/format";

const SCHEME = "unfold://";
/** The query parameters a link may carry. */
const PARTS = new Set(["page", "item"]);

/** A parsed link, or why the text isn't one. */
export type LinkParse = { readonly ok: true; readonly link: AgentLink } | { readonly ok: false; readonly message: string };

/** A workspace project as links and tools name it. */
export interface NamedProject {
  readonly id: string;
  /** Its folder's name. */
  readonly name: string;
  /** Its folder. */
  readonly path: string;
}

/** A link whose project is a workspace project, by id. */
export interface ResolvedLink {
  /** The project's id. */
  readonly project: string;
  readonly board: string;
  readonly page?: string;
  readonly item?: string;
}

function refused(text: string, why: string): LinkParse {
  return { ok: false, message: `“${clip(text)}” isn't an Unfold link: ${why}. Links look like unfold://<project>/<board>?page=<page>&item=<item>.` };
}

/** Each `/`-separated part decoded; null when a `%` escape is malformed. */
function decodeParts(parts: readonly string[]): string[] | null {
  try {
    return parts.map((part) => decodeURIComponent(part));
  } catch {
    return null;
  }
}

/** Reads a link pasted into a chat (surrounding space is ignored). */
export function parseLink(input: string): LinkParse {
  const text = input.trim();
  if (text.slice(0, SCHEME.length).toLowerCase() !== SCHEME) {
    return refused(text, "it doesn't start with unfold://");
  }
  const rest = text.slice(SCHEME.length);
  const queryAt = rest.indexOf("?");
  const path = queryAt < 0 ? rest : rest.slice(0, queryAt);
  const query = queryAt < 0 ? "" : rest.slice(queryAt + 1);
  const segments = decodeParts(path.split("/"));
  if (segments === null) {
    return refused(text, "it has a malformed % escape");
  }
  const [project = "", ...board] = segments;
  if (project.length === 0) {
    return refused(text, "it names no project");
  }
  if (board.length === 0 || board.some((segment) => segment.length === 0)) {
    return refused(text, "it names no board");
  }
  const parts = new Map<string, string>();
  for (const pair of query.length > 0 ? query.split("&") : []) {
    const equals = pair.indexOf("=");
    const decoded = decodeParts(equals < 0 ? [pair] : [pair.slice(0, equals), pair.slice(equals + 1)]);
    if (decoded === null) {
      return refused(text, "it has a malformed % escape");
    }
    const [name = "", value = ""] = decoded;
    if (!PARTS.has(name) || value.length === 0 || parts.has(name)) {
      return refused(text, `“${clip(pair, 60)}” isn't page=<id> or item=<id>, once each`);
    }
    parts.set(name, value);
  }
  const page = parts.get("page");
  const item = parts.get("item");
  if (item !== undefined && page === undefined) {
    return refused(text, "it names an item but not its page");
  }
  return {
    ok: true,
    link: { project, board: board.join("/"), ...(page !== undefined ? { page } : {}), ...(item !== undefined ? { item } : {}) },
  };
}

/**
 * The projects `reference` names: the project with that id, else every project whose folder has
 * that name (more than one when worktrees share it).
 */
export function projectsNamed<T extends NamedProject>(projects: readonly T[], reference: string): readonly T[] {
  const byId = projects.find((row) => row.id === reference);
  return byId ? [byId] : projects.filter((row) => row.name === reference);
}

/** Why `name` names none of `projects`, which share it: each one's id and folder. */
export function sharedName(name: string, projects: readonly NamedProject[]): string {
  return `${projects.length} projects have a folder named “${clip(name, 60)}”: ${projects.map((row) => `${row.id} (${row.path})`).join(", ")}`;
}

/**
 * Resolves a link's project among the workspace's projects: by id, else by the folder name only
 * one of them has. Fails with a message an agent can act on.
 */
export function resolveLink(link: AgentLink, projects: readonly NamedProject[]): { readonly ok: true; readonly link: ResolvedLink } | { readonly ok: false; readonly message: string } {
  const [project, ...others] = projectsNamed(projects, link.project);
  if (!project) {
    return {
      ok: false,
      message: `No project named or with the id “${clip(link.project, 60)}” is in the workspace. The link may come from another machine, or the project's folder was renamed; list_projects shows the names.`,
    };
  }
  if (others.length > 0) {
    return {
      ok: false,
      message: `The link can't name one project: ${sharedName(link.project, [project, ...others])}. Ask the person to copy the link again (it then names the project by its id), or pass project (its id) with board and page.`,
    };
  }
  return { ok: true, link: { ...link, project: project.id } };
}
