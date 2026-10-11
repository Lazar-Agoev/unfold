/**
 * How results name things (design §6.1): Unfold's names with `file:line`, counts in words, written
 * types with the declarations they link to, and line labels as the board draws them. Shared by the
 * tools so every result reads the same.
 */

import type { AgentCardResult } from "../../shared/agent-api";
import type { LineLabel, ProjectState, TypeText } from "./engine-types";

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
    case "paused":
      return `paused (${state.changed} files changed)`;
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
  const hash = key.indexOf("#");
  return hash < 0 ? key : key.slice(0, hash);
}

/**
 * A readable name for a key, as boards show one: `OrderService.place`, `createOrder`, `POST /orders`
 * for an HTTP entrance, the path for a file.
 */
export function keyName(key: string): string {
  if (key.startsWith("entrance:")) {
    try {
      const parts: unknown = JSON.parse(key.slice("entrance:".length));
      const words = Array.isArray(parts) ? parts.slice(2, 4).filter((part): part is string => typeof part === "string" && part.length > 0) : [];
      return words.length > 0 ? words.join(" ") : "entrance";
    } catch {
      return "entrance";
    }
  }
  const local = key.slice(key.indexOf("#") + 1);
  const names = local
    .split("/")
    .map((step) => step.slice(step.indexOf(":") + 1).replace(/\[\d+\]$/, ""))
    .filter((name) => name.length > 0);
  return names.slice(-2).join(".") || (keyFile(key) ?? key);
}

/** `src/orders.ts:12`, or `src/orders.ts:12–20` for a range; the path alone without a line. */
export function place(path: string, line?: number | null, end?: number | null): string {
  if (line === undefined || line === null) {
    return path;
  }
  return end !== undefined && end !== null && end !== line ? `${path}:${line}–${end}` : `${path}:${line}`;
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

/**
 * A line's label in full, as the board draws it unfolded: `(Cart, User) → Promise<Charge>`; a type the
 * code doesn't write is the parameter's name, else `…`; a missing output is `?`.
 */
export function labelText(label: LineLabel): string {
  const inputs = label.inputsUnknown
    ? "…"
    : label.inputs
        .map((input) => {
          if (input.type !== undefined) {
            return input.keyword !== undefined ? `${input.keyword}=${input.type.text}` : input.type.text;
          }
          return input.keyword ?? input.name ?? "…";
        })
        .join(", ");
  return `(${inputs}) → ${label.output?.text ?? "?"}`;
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

/**
 * A card an edit placed, as a result line: `createOrder (card id create-order, created)`, with where
 * following stopped at it.
 */
export function cardText(card: AgentCardResult): string {
  const stops = card.stops ?? [];
  return `${card.name} (card id ${card.id}, ${card.status})${stops.length > 0 ? `; stopped: ${stops.map(stopText).join("; ")}` : ""}`;
}
