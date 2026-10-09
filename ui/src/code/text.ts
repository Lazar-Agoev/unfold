/** Display text for external and unresolved occurrences (reader.md "External and unresolved text"). */

import type { CallLink, Reason } from "../ipc/generated";

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

/** What a link's tooltip says when it can't be followed: its module, or why it is unresolved. */
export function linkNote(link: Pick<CallLink, "resolution" | "note" | "reason" | "module">): string | null {
  if (link.resolution === "external") {
    return `external · ${link.module ?? link.note ?? "module"}`;
  }
  if (link.resolution === "unresolved") {
    return `unresolved · ${link.note ?? reasonText(link.reason)}`;
  }
  return null;
}

/** `src/routes/orders.ts` → `routes/orders.ts`: the last two path segments, for card headers. */
export function shortPath(path: string): string {
  const parts = path.split("/");
  return parts.length <= 2 ? path : parts.slice(-2).join("/");
}

/** `agent:codex` → `Codex`, for author marks. */
export function authorName(author: string): string {
  const name = author.replace(/^(agent|person):/, "");
  return name.charAt(0).toUpperCase() + name.slice(1);
}
