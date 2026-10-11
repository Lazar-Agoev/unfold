/**
 * The manual entrance editor's model: a draft of one `[[entrance]]` table, its validation (the same
 * rules Rust applies, docs/engine.md "Declarations"), the `EntranceSpec` it becomes, and how an
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
interface DraftProblems {
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

/**
 * The `EntranceSpec` a valid draft becomes, with ID `id`: blanks are left out, `method` only for
 * `http`. The fields the editor doesn't show are kept from `table`, the table being edited.
 */
export function specOf(draft: EntranceDraft, id: string, table: ManualEntranceView | null): EntranceSpec {
  const trigger = draft.trigger.trim();
  const label = draft.label.trim();
  const process = draft.kind === "process";
  return {
    id,
    kind: draft.kind,
    application: table?.application ?? null,
    trigger: trigger === "" ? null : trigger,
    method: draft.kind === "http" && draft.method !== "" ? draft.method : null,
    label: label === "" ? null : label,
    description: table?.description ?? null,
    handler: process ? [] : draft.handlers.map((handler) => handler.trim()).filter((handler) => handler !== ""),
    cwd: process ? (table?.cwd ?? null) : null,
    command: process ? (table?.command ?? null) : null,
    startup: process ? draft.startup.trim() : null,
    target: null,
    pythonPaths: process ? (table?.pythonPaths ?? null) : null,
    env: process ? (table?.env ?? {}) : {},
    schedule: process ? (table?.schedule ?? null) : null,
  };
}

/**
 * The ID of a new entrance: its trigger, else its label, else its kind, in kebab case, with `-2`,
 * `-3`, … until it differs from every ID in `taken`.
 */
export function entranceId(draft: EntranceDraft, taken: readonly string[]): string {
  const words = (text: string) =>
    text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 56)
      .replace(/-+$/, "");
  const base = words(draft.trigger) || words(draft.label) || draft.kind;
  let id = base;
  for (let suffix = 2; taken.includes(id); suffix += 1) {
    id = `${base}-${suffix}`;
  }
  return id;
}

/**
 * The `[[entrance]]` table an Entrances-view row came from, or null when it isn't a manual entrance
 * (or the settings changed since). A manual entrance's key is `entrance:configured:<id>`.
 */
export function manualTableOf(row: Pick<EntranceRow, "key" | "detector">, tables: readonly ManualEntranceView[]): ManualEntranceView | null {
  if (row.detector !== "manual") {
    return null;
  }
  return tables.find((table) => row.key === `entrance:configured:${table.id}`) ?? null;
}
