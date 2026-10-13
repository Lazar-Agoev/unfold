/**
 * How results name things (design §6.1): Unfold's names with `file:line`, counts in words, and written
 * types with the declarations they link to. Shared by the tools so every result reads the same.
 */

import type { AgentCardResult, AgentIdentity } from "../../shared/agent-api";
import type { ProjectState, TypeText } from "./engine-types";

/** `1 board`, `3 boards`; `plural` when adding an `s` isn't it. */
export function count(n: number, noun: string, plural = `${noun}s`): string {
  return `${n} ${n === 1 ? noun : plural}`;
}

/** A project's state in words: `current`, `compiling (120 of 400)`, `resting (last: current)`. */
export function stateText(state: ProjectState): string {
  switch (state.kind) {
    case "current":
      return "current";
    case "compiling":
      return `compiling (${state.done} of ${state.total})`;
    case "updating":
      return state.files === null ? "updating" : `updating (${state.files} files)`;
    case "problems":
      return `problems (${state.count})`;
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

/** The file a block key is in (`src/routes/orders.ts`); undefined for an entrance key. */
export function keyFile(key: string): string | undefined {
  if (key.startsWith("entrance:")) {
    return undefined;
  }
  // Names escape `#` (`%23`), so the last one ends the path, which may hold one.
  const hash = key.lastIndexOf("#");
  return hash < 0 ? key : key.slice(0, hash);
}

/**
 * A readable name for a key, as boards show one: `OrderService.place`, `createOrder`, `POST /orders`
 * for an HTTP entrance, a configured entrance's id, the path for a file.
 */
export function keyName(key: string): string {
  if (key.startsWith("entrance:configured:")) {
    return key.slice("entrance:configured:".length);
  }
  if (key.startsWith("entrance:")) {
    try {
      const parts: unknown = JSON.parse(key.slice("entrance:".length));
      const words = Array.isArray(parts) ? parts.slice(2, 4).filter((part): part is string => typeof part === "string" && part.length > 0) : [];
      return words.length > 0 ? words.join(" ") : "entrance";
    } catch {
      return "entrance";
    }
  }
  const local = key.slice(key.lastIndexOf("#") + 1);
  const names = local
    .split("/")
    .map((step) => step.slice(step.indexOf(":") + 1).replace(/\[\d+\]$/, ""))
    .filter((name) => name.length > 0);
  return names.slice(-2).join(".") || (keyFile(key) ?? key);
}

/** The block kind a key ends in (`function`, `if`, `interface`); `entrance` for an entrance key. */
export function keyKind(key: string): string {
  if (key.startsWith("entrance:")) {
    return "entrance";
  }
  const step = key.slice(key.lastIndexOf("#") + 1).split("/").at(-1) ?? "";
  return step.replace(/[:[].*$/, "");
}

/**
 * Who wrote something, for the agent: `you` (the calling agent), `Codex (an agent)`, `Lazar (a
 * person)`, or `a person` when no author is recorded (a step group titled before titles were signed).
 */
export function authorText(author: string | undefined, agent: AgentIdentity): string {
  if (author === undefined) {
    return "a person";
  }
  if (author === agent.author) {
    return "you";
  }
  const name = author.replace(/^(agent|person):/, "");
  const shown = name.charAt(0).toUpperCase() + name.slice(1);
  return author.startsWith("agent:") ? `${shown} (an agent)` : `${shown} (a person)`;
}

/** `src/orders.ts:12`, or `src/orders.ts:12–20` for a range; the path alone without a line. */
export function place(path: string, line?: number | null, end?: number | null): string {
  if (line === undefined || line === null) {
    return path;
  }
  return end !== undefined && end !== null && end !== line ? `${path}:${line}–${end}` : `${path}:${line}`;
}

/** Facts joined with `; `, leaving out the ones that are absent or empty. */
export function joined(parts: readonly (string | null | undefined | false)[]): string {
  return parts.filter((part): part is string => typeof part === "string" && part.length > 0).join("; ");
}

/** Text cut to `max` characters, with `…` where it was cut. */
export function clip(text: string, max = 120): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/**
 * A written type with what each name in it links to: `CartItem[] (CartItem: src/cart.ts#interface:CartItem)`,
 * `Money (Money: unresolved, a variable, not a type)`. Builtin names read as written.
 */
export function typeText(type: TypeText): string {
  const links = type.spans.flatMap((span) => {
    const name = type.text.slice(span.start, span.end);
    switch (span.style) {
      case "declared":
        return span.target === undefined ? [] : [`${name}: ${span.target}`];
      case "external":
        return [`${name}: from ${span.note ?? span.module ?? "a library"}`];
      case "unresolved":
        return [`${name}: unresolved, ${span.note ?? "not found"}`];
      case "builtin":
        return [];
    }
  });
  return links.length > 0 ? `${type.text} (${links.join("; ")})` : type.text;
}

/** Where following stopped at a card, in words. */
function stopText(stop: NonNullable<AgentCardResult["stops"]>[number]): string {
  const where = stop.line === undefined ? "" : ` (line ${stop.line})`;
  switch (stop.reason) {
    case "external":
      return `${stop.call ?? stop.type ?? "a call"}${where} is external${stop.note ? `, ${stop.note}` : ""}`;
    case "unresolved":
      return `${stop.call ?? stop.type ?? "a call"}${where} is unresolved${stop.note ? `: ${stop.note}` : ""}`;
    case "builtin":
      return `${stop.type ?? "a type"}${stop.field ? ` (${stop.field})` : ""} is built in`;
    case "depth":
      return `${stop.count ?? 0} more not followed, past the depth asked`;
    case "limit":
      return `${stop.count ?? 0} more not added, past the limit`;
  }
}

/** A call path expanded in place, as results write it: its keys, outermost first, joined with › . */
export function callPathText(path: readonly string[]): string {
  return `[${path.join(" › ")}]`;
}

/** A card's expansion, as `set_view` reports it: `expanded, calls expanded in place [a], [a › b]`, or `compact`. */
function expansionText(card: AgentCardResult): string | null {
  if (card.expanded === undefined) {
    return null;
  }
  const calls = card.expandedCalls ?? [];
  return `${card.expanded ? "expanded" : "compact"}${calls.length > 0 ? `, calls expanded in place ${calls.map(callPathText).join(", ")}` : ""}`;
}

/**
 * A card an edit placed, as a result line: `createOrder (card id create-order, created)`, with the
 * frame it went into, its expansion when an edit set it, and where following stopped at it.
 */
export function cardText(card: AgentCardResult): string {
  const stops = card.stops ?? [];
  const frame = card.frame === undefined ? "" : `, in frame ${card.frame}`;
  const expansion = expansionText(card);
  return `${card.name} (card id ${card.id}, ${card.status}${frame})${expansion !== null ? `: ${expansion}` : ""}${stops.length > 0 ? `; stopped: ${stops.map(stopText).join("; ")}` : ""}`;
}
