/** Display text for library and unknown links, paths, and authors. */

import type { LinkEnd, Reason } from "../ipc/generated";

const REASONS: Readonly<Record<Reason, string>> = {
  unknown_name: "unknown name",
  unknown_receiver: "receiver type unknown",
  parameter: "parameter",
  mutable_binding: "mutable binding",
  not_concrete: "not a concrete function",
  accessor: "accessor",
  ambiguous: "ambiguous target",
  overridden: "overridden by a subclass",
  unbound_this: "unbound this",
  not_indexed: "not indexed",
  not_found: "not found",
  dynamic: "dynamic call",
  wrapper: "wrapper",
  unsupported: "unsupported form",
};

export function reasonText(reason: Reason | null | undefined): string {
  return reason ? REASONS[reason] : "unresolved";
}

/** What a link's tooltip says when it can't be followed: its module, or why it is unknown. */
export function linkNote(to: LinkEnd): string | null {
  switch (to.outcome) {
    case "library":
      return `library · ${to.module ?? to.note ?? "module"}`;
    case "unknown":
      return `unknown · ${to.note ?? reasonText(to.reason)}`;
    case "project":
    case "builtin":
      return null;
  }
}

/** `src/routes/orders.ts` → `routes/orders.ts`: the last two path segments, for card headers. */
export function shortPath(path: string): string {
  const parts = path.split("/");
  return parts.length <= 2 ? path : parts.slice(-2).join("/");
}

/** Who wrote a piece of text, as the app shows it: a name, and whether an agent wrote it. */
interface AuthorView {
  readonly name: string;
  readonly agent: boolean;
}

/** `person:lazar` → Lazar; `agent:codex` → Codex, an agent. */
export function authorOf(author: string): AuthorView {
  const name = author.replace(/^(agent|person):/, "");
  return { name: name.charAt(0).toUpperCase() + name.slice(1), agent: author.startsWith("agent:") };
}

/** The name of the agent that wrote a piece of text (`agent:claude` → Claude), for its mark; null for a person. */
export function agentName(author: string): string | null {
  const { name, agent } = authorOf(author);
  return agent ? name : null;
}

/**
 * Who wrote a piece of text, as plain text: `person:lazar` → "Lazar", `agent:codex` → "Codex · agent",
 * so an agent never reads as a person. Where the agent mark can be drawn, draw it (`<Author>`).
 */
export function authorName(author: string): string {
  const { name, agent } = authorOf(author);
  return agent ? `${name} · agent` : name;
}
