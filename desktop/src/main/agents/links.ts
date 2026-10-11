/**
 * Links (D9): `unfold://<project's shared name>/<board id>?page=<page id>&item=<item id>`, which a
 * window's Copy link builds and a person pastes into an agent's chat. Only the main process reads
 * them: it parses the text, then resolves the shared name against the workspace (the first project
 * with that name, shared project names). A link names things and carries no code.
 */

import type { AgentLink } from "../../shared/agent-api";

const SCHEME = "unfold://";
/** The query parameters a link may carry. */
const PARTS = new Set(["page", "item"]);

/** A parsed link, or why the text isn't one. */
export type LinkParse = { readonly ok: true; readonly link: AgentLink } | { readonly ok: false; readonly message: string };

/** A link whose project is a workspace project, by id. */
export interface ResolvedLink {
  /** The project's id. */
  readonly project: string;
  readonly board: string;
  readonly page?: string;
  readonly item?: string;
}

function refused(text: string, why: string): LinkParse {
  return { ok: false, message: `“${text}” isn't an Unfold link: ${why}. Links look like unfold://<project>/<board>?page=<page>&item=<item>.` };
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
      return refused(text, `“${pair}” isn't page=<id> or item=<id>, once each`);
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
 * Resolves a link's project by shared name among the workspace's projects (in workspace order: the
 * first with the name wins). Fails with a message an agent can act on.
 */
export function resolveLink(
  link: AgentLink,
  projects: readonly { readonly id: string; readonly name: string }[],
): { readonly ok: true; readonly link: ResolvedLink } | { readonly ok: false; readonly message: string } {
  const project = projects.find((row) => row.name === link.project);
  if (!project) {
    return {
      ok: false,
      message: `No project named “${link.project}” is in the workspace. The link may come from another machine, or the project was renamed; list_projects shows the names.`,
    };
  }
  return { ok: true, link: { ...link, project: project.id } };
}
