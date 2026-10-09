/**
 * The manual entrance editor's model: a draft of one `[[entrance]]` table, its validation (the same
 * rules Rust applies, docs/entrances.md "Declarations"), the `EntranceSpec` it becomes, and how an
 * Entrances-view row finds the table it came from.
 */

import type { EntranceKind, EntranceRow, EntranceSpec, HttpMethod, ManualEntranceView } from "../ipc/generated";
import { locatorLanguage, locatorProblem } from "./locators";

export const ENTRANCE_KINDS: readonly { readonly value: EntranceKind; readonly label: string; readonly trigger: string }[] = [
  { value: "command", label: "Command", trigger: "The command name, such as report" },
  { value: "http", label: "HTTP", trigger: "The path, such as /orders/:id" },
  { value: "worker", label: "Worker", trigger: "The queue, such as emails" },
  { value: "schedule", label: "Schedule", trigger: "When it runs, such as 0 3 * * *" },
  { value: "event", label: "Event", trigger: "The event or signal, such as SIGTERM" },
  { value: "process", label: "Process", trigger: "How it starts, such as serve" },
];

export const HTTP_METHODS: readonly HttpMethod[] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "ALL"];

/** One entrance as the editor holds it while it's being edited. */
export interface EntranceDraft {
  readonly kind: EntranceKind;
  readonly trigger: string;
  /** Empty for "any or unknown". Only `http` entrances keep it. */
  readonly method: HttpMethod | "";
  readonly label: string;
  /** Handler locators in order, middleware first (every kind but `process`). */
  readonly handlers: readonly string[];
  /** The startup locator (`process` only). */
  readonly startup: string;
}

export function emptyDraft(handler = ""): EntranceDraft {
  return { kind: "command", trigger: "", method: "", label: "", handlers: [handler], startup: "" };
}

/** A draft of an existing table. */
export function draftOf(view: ManualEntranceView): EntranceDraft {
  return {
    kind: view.kind,
    trigger: view.trigger ?? "",
    method: view.method ?? "",
    label: view.label ?? "",
    handlers: view.handler.length > 0 ? view.handler : [""],
    startup: view.startup ?? "",
  };
}

/** What's wrong with a draft, by field. `handlers[i]` matches the i-th handler field. */
export interface DraftProblems {
  readonly trigger?: string;
  readonly startup?: string;
  readonly handlers: readonly (string | null)[];
  /** A problem with the handlers as a whole (none given, mixed languages). */
  readonly handlerList?: string;
}

export function hasProblems(problems: DraftProblems): boolean {
  return (
    problems.trigger !== undefined ||
    problems.startup !== undefined ||
    problems.handlerList !== undefined ||
    problems.handlers.some((problem) => problem !== null)
  );
}

/** Checks a draft as Rust will, so the form can say what to fix before saving. */
export function validateDraft(draft: EntranceDraft): DraftProblems {
  const trigger =
    draft.trigger !== "" && draft.trigger.trim() === ""
      ? "A trigger can't be blank. Leave it empty if it's unknown."
      : undefined;
  if (draft.kind === "process") {
    const startup = draft.startup.trim();
    const startupProblem = startup === "" ? "A process needs the file it starts from, such as src/server.ts." : locatorProblem(startup);
    return {
      ...(trigger === undefined ? {} : { trigger }),
      ...(startupProblem === null ? {} : { startup: startupProblem }),
      handlers: draft.handlers.map(() => null),
    };
  }
  const handlers = draft.handlers.map((handler) => (handler.trim() === "" ? null : locatorProblem(handler.trim())));
  const given = draft.handlers.map((handler) => handler.trim()).filter((handler) => handler !== "");
  const languages = new Set(given.map(locatorLanguage).filter((language) => language !== null));
  const handlerList =
    given.length === 0
      ? "Add at least one handler."
      : languages.size > 1
        ? "One entrance's handlers can't mix TypeScript and Python files."
        : undefined;
  return {
    ...(trigger === undefined ? {} : { trigger }),
    handlers,
    ...(handlerList === undefined ? {} : { handlerList }),
  };
}

/** The `EntranceSpec` a valid draft becomes: blanks are left out, `method` only for `http`. */
export function specOf(draft: EntranceDraft): EntranceSpec {
  const trigger = draft.trigger.trim();
  const label = draft.label.trim();
  const process = draft.kind === "process";
  return {
    kind: draft.kind,
    trigger: trigger === "" ? null : trigger,
    method: draft.kind === "http" && draft.method !== "" ? draft.method : null,
    label: label === "" ? null : label,
    handler: process ? [] : draft.handlers.map((handler) => handler.trim()).filter((handler) => handler !== ""),
    startup: process ? draft.startup.trim() : null,
  };
}

/**
 * The `[[entrance]]` table an Entrances-view row came from, or null when it isn't a manual entrance
 * (or the settings changed since). A manual entrance's key is `entrance:` and the JSON array
 * `[detector, kind, method, trigger, first target]`, with `#n` for the n-th repeat of the same
 * array, so the row is the n-th table with that kind, method, trigger, and first locator.
 */
export function manualTableOf(row: Pick<EntranceRow, "key" | "detector">, tables: readonly ManualEntranceView[]): ManualEntranceView | null {
  if (row.detector !== "manual" || !row.key.startsWith("entrance:")) {
    return null;
  }
  let body = row.key.slice("entrance:".length);
  let repeat = 0;
  const suffix = /\]#([1-9][0-9]*)$/.exec(body);
  if (suffix?.[1] !== undefined) {
    repeat = Number(suffix[1]);
    body = body.slice(0, suffix.index + 1);
  }
  let parts: unknown;
  try {
    parts = JSON.parse(body);
  } catch {
    return null;
  }
  if (!Array.isArray(parts) || parts.length !== 5 || !parts.every((part) => typeof part === "string")) {
    return null;
  }
  const [, kind, method, trigger, first] = parts as [string, string, string, string, string];
  const matches = tables.filter(
    (table) =>
      table.kind === kind &&
      (table.method ?? "") === method &&
      (table.trigger ?? "") === trigger &&
      (table.handler[0] ?? table.startup ?? "") === first,
  );
  return matches[repeat] ?? null;
}
