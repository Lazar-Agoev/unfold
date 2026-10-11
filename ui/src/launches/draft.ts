/**
 * The launch editor's model: one `[[entrance]]` table (a launch, or a handler entrance) as the
 * editor holds it, where it starts from (a saved table, a prefill, a deploy-file import), what it
 * checks before saving, and what it becomes: the `EntranceSpec` saved, the `LaunchDraftSpec`
 * `check_launch` previews, and the IDs new launches and applications get. Rust stays the judge of
 * the file; this only says what to fix before asking it.
 */

import type {
  ApplicationSpec,
  EntranceKind,
  EntranceSpec,
  HttpMethod,
  ImportedLaunch,
  LaunchDraftSpec,
  LaunchTargetSpec,
  ManualEntranceView,
  MatchSpec,
  OverrideSpec,
} from "../ipc/generated";
import type { LaunchPrefill } from "../state/overlays";
import { locatorLanguage, locatorProblem } from "./locators";

export type Intent = "process" | "handler";

export type HandlerKind = Exclude<EntranceKind, "process">;

export const HANDLER_KINDS: readonly { readonly value: HandlerKind; readonly label: string; readonly trigger: string }[] = [
  { value: "http", label: "HTTP", trigger: "The path, such as /orders/:id" },
  { value: "worker", label: "Queue worker", trigger: "The queue, such as emails" },
  { value: "schedule", label: "Schedule", trigger: "When it runs, such as 0 3 * * *" },
  { value: "command", label: "CLI command", trigger: "The command name, such as report" },
  { value: "event", label: "Event", trigger: "The event or signal, such as SIGTERM" },
];

export const HTTP_METHODS: readonly HttpMethod[] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "ALL"];

export type TargetType = LaunchTargetSpec["type"];

export const TARGET_TYPES: readonly { readonly value: TargetType; readonly label: string }[] = [
  { value: "file", label: "Script file" },
  { value: "python_module", label: "Module" },
  { value: "python_callable", label: "Callable" },
  { value: "python_asgi", label: "ASGI app" },
];

/** A target set by hand, with every type's fields so switching type keeps what was typed. */
export interface TargetDraft {
  readonly type: TargetType;
  readonly path: string;
  readonly module: string;
  readonly attribute: string;
  readonly factory: boolean;
}

export interface EnvRow {
  readonly key: string;
  readonly value: string;
}

/** One entrance as the editor holds it while it's being edited. */
export interface LaunchDraft {
  readonly intent: Intent;
  readonly label: string;
  readonly description: string;
  /** The chosen application: a declared one, or a new one the save also declares. */
  readonly application: ApplicationSpec | null;
  /** The name typed for a new application, while "New application…" is chosen. */
  readonly newApplication: string | null;
  /** Shown for handlers; a launch keeps the one it was saved with. */
  readonly trigger: string;
  // A launch's fields.
  readonly command: string;
  /** The working folder, project-relative; `.` for the project's folder. */
  readonly cwd: string;
  readonly scheduled: boolean;
  readonly schedule: string;
  /** Set by hand under Analysis details; null when it's read from the command. */
  readonly target: TargetDraft | null;
  /** Set by hand under Analysis details, in order; null when they're read from the command. */
  readonly pythonPaths: readonly string[] | null;
  readonly env: readonly EnvRow[];
  /** A saved launch's startup locator, which the editor keeps as it was. */
  readonly startup: string | null;
  // A handler's fields.
  readonly kind: HandlerKind;
  /** Empty for "any or unknown". Only HTTP keeps it. */
  readonly method: HttpMethod | "";
  /** Handler locators in order, middleware first. */
  readonly handlers: readonly string[];
}

const EMPTY: LaunchDraft = {
  intent: "process",
  label: "",
  description: "",
  application: null,
  newApplication: null,
  trigger: "",
  command: "",
  cwd: ".",
  scheduled: false,
  schedule: "",
  target: null,
  pythonPaths: null,
  env: [],
  startup: null,
  kind: "command",
  method: "",
  handlers: [""],
};

export function emptyTarget(type: TargetType = "python_asgi"): TargetDraft {
  return { type, path: "", module: "", attribute: "", factory: false };
}

export function targetDraftOf(target: LaunchTargetSpec): TargetDraft {
  switch (target.type) {
    case "file":
      return { ...emptyTarget("file"), path: target.path };
    case "python_module":
      return { ...emptyTarget("python_module"), module: target.module };
    case "python_callable":
      return { ...emptyTarget("python_callable"), module: target.module, attribute: target.attribute };
    case "python_asgi":
      return { ...emptyTarget("python_asgi"), module: target.module, attribute: target.attribute, factory: target.factory };
  }
}

function envRows(env: Readonly<Record<string, string>>): EnvRow[] {
  return Object.entries(env).map(([key, value]) => ({ key, value }));
}

/** A new entrance: a launch, or a handler starting from `handler` (a card's definition). */
export function newDraft(intent: Intent, handler = ""): LaunchDraft {
  return { ...EMPTY, intent, handlers: [handler] };
}

/** A new launch from a suggestion or "Add launch". */
export function draftOfPrefill(prefill: LaunchPrefill): LaunchDraft {
  return {
    ...EMPTY,
    command: prefill.command ?? "",
    cwd: prefill.cwd ?? ".",
    label: prefill.label ?? "",
    application: prefill.application ?? null,
    scheduled: prefill.schedule !== undefined,
    schedule: prefill.schedule ?? "",
    description: prefill.description ?? "",
    env: envRows(prefill.env ?? {}),
  };
}

/** A launch a deploy file describes, for review. */
export function draftOfImport(launch: ImportedLaunch): LaunchDraft {
  return {
    ...EMPTY,
    command: launch.command ?? "",
    cwd: launch.cwd,
    label: launch.label,
    application: launch.application,
    scheduled: launch.schedule !== null,
    schedule: launch.schedule ?? "",
    description: launch.description ?? "",
    env: envRows(launch.env),
  };
}

function applicationNamed(id: string | null, applications: readonly ApplicationSpec[]): ApplicationSpec | null {
  return id === null ? null : (applications.find((candidate) => candidate.id === id) ?? { id, name: id, description: null });
}

/** A detected launch's metadata, from its `[[entrance_override]]` when it has one. */
export function draftOfOverride(existing: OverrideSpec | null, applications: readonly ApplicationSpec[]): LaunchDraft {
  return {
    ...EMPTY,
    label: existing?.label ?? "",
    description: existing?.description ?? "",
    application: applicationNamed(existing?.application ?? null, applications),
    scheduled: (existing?.schedule ?? null) !== null,
    schedule: existing?.schedule ?? "",
  };
}

/** A saved table; `applications` gives its application's name. */
export function draftOfTable(table: ManualEntranceView, applications: readonly ApplicationSpec[]): LaunchDraft {
  const application = applicationNamed(table.application, applications);
  const shared = { ...EMPTY, label: table.label ?? "", description: table.description ?? "", application, trigger: table.trigger ?? "" };
  if (table.kind === "process") {
    return {
      ...shared,
      intent: "process",
      command: table.command ?? "",
      cwd: table.cwd ?? ".",
      scheduled: table.schedule !== null,
      schedule: table.schedule ?? "",
      target: table.target ? targetDraftOf(table.target) : null,
      pythonPaths: table.pythonPaths,
      env: envRows(table.env),
      startup: table.startup,
    };
  }
  return { ...shared, intent: "handler", kind: table.kind, method: table.method ?? "", handlers: table.handler.length > 0 ? table.handler : [""] };
}

/** Lowercase words joined by `-`, at most 56 characters (room for a `-N` suffix within 64). */
function kebab(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 56)
    .replace(/-+$/, "");
}

/** `base`, or `base-2`, `base-3`, … until it differs from every ID in `taken`. */
export function uniqueId(base: string, taken: readonly string[]): string {
  let id = base;
  for (let suffix = 2; taken.includes(id); suffix += 1) {
    id = `${base}-${suffix}`;
  }
  return id;
}

/**
 * The ID of a new entrance, unique among `taken`: a launch's application and label
 * (`recipe-cost-start-api`), a handler's trigger or label, in kebab case; else `launch` or the
 * handler's kind. `declared` are the applications a new one's ID must differ from.
 */
export function entranceId(draft: LaunchDraft, taken: readonly string[], declared: readonly ApplicationSpec[]): string {
  const application = effectiveApplication(draft, declared);
  const base =
    draft.intent === "process"
      ? kebab([application?.id ?? "", draft.label].filter((part) => part !== "").join(" ")) || "launch"
      : kebab(draft.trigger) || kebab(draft.label) || draft.kind;
  return uniqueId(base, taken);
}

/** The application a save names: the chosen one, or the new one named so far (null while unnamed). */
export function effectiveApplication(draft: LaunchDraft, declared: readonly ApplicationSpec[]): ApplicationSpec | null {
  if (draft.newApplication === null) {
    return draft.application;
  }
  const name = draft.newApplication.trim();
  if (name === "") {
    return null;
  }
  const taken = declared.map((application) => application.id);
  return { id: uniqueId(kebab(name) || "application", taken), name, description: null };
}

function targetSpec(target: TargetDraft): LaunchTargetSpec {
  switch (target.type) {
    case "file":
      return { type: "file", path: target.path.trim() };
    case "python_module":
      return { type: "python_module", module: target.module.trim() };
    case "python_callable":
      return { type: "python_callable", module: target.module.trim(), attribute: target.attribute.trim() };
    case "python_asgi":
      return { type: "python_asgi", module: target.module.trim(), attribute: target.attribute.trim(), factory: target.factory };
  }
}

function envOf(rows: readonly EnvRow[]): Record<string, string> {
  return Object.fromEntries(rows.filter((row) => row.key.trim() !== "").map((row) => [row.key.trim(), row.value]));
}

function optional(text: string): string | null {
  const trimmed = text.trim();
  return trimmed === "" ? null : trimmed;
}

/** The working folder as saved: none for the project's folder. */
function cwdOf(cwd: string): string | null {
  const trimmed = cwd.trim();
  return trimmed === "" || trimmed === "." ? null : trimmed;
}

/** The `EntranceSpec` a valid draft becomes, with ID `id`; blanks are left out. */
export function specOf(draft: LaunchDraft, id: string, declared: readonly ApplicationSpec[]): EntranceSpec {
  const shared = {
    id,
    application: effectiveApplication(draft, declared)?.id ?? null,
    trigger: optional(draft.trigger),
    label: optional(draft.label),
    description: optional(draft.description),
  };
  if (draft.intent === "process") {
    return {
      ...shared,
      kind: "process",
      method: null,
      handler: [],
      cwd: cwdOf(draft.cwd),
      command: optional(draft.command),
      startup: draft.startup,
      target: draft.target ? targetSpec(draft.target) : null,
      pythonPaths: draft.pythonPaths ? draft.pythonPaths.map((path) => path.trim()) : null,
      env: envOf(draft.env),
      schedule: draft.scheduled ? optional(draft.schedule) : null,
    };
  }
  return {
    ...shared,
    kind: draft.kind,
    method: draft.kind === "http" && draft.method !== "" ? draft.method : null,
    handler: draft.handlers.map((handler) => handler.trim()).filter((handler) => handler !== ""),
    cwd: null,
    command: null,
    startup: null,
    target: null,
    pythonPaths: null,
    env: {},
    schedule: null,
  };
}

/** The `[[entrance_override]]` a detected launch's draft becomes. */
export function overrideOf(draft: LaunchDraft, match: MatchSpec, declared: readonly ApplicationSpec[]): OverrideSpec {
  return {
    match,
    application: effectiveApplication(draft, declared)?.id ?? null,
    label: optional(draft.label),
    description: optional(draft.description),
    schedule: draft.scheduled ? optional(draft.schedule) : null,
  };
}

export function sameMatch(a: MatchSpec, b: MatchSpec): boolean {
  return a.detector === b.detector && a.path === b.path && a.pointer === b.pointer;
}

/** What `check_launch` previews for a launch draft (`id` when it's saved, so it's compared with itself). */
export function checkSpecOf(draft: LaunchDraft, id: string | null): LaunchDraftSpec {
  const command = optional(draft.command);
  return {
    ...(id === null ? {} : { id }),
    ...(command === null ? {} : { command }),
    cwd: cwdOf(draft.cwd) ?? ".",
    ...(draft.target ? { target: targetSpec(draft.target) } : {}),
    ...(draft.pythonPaths ? { pythonPaths: draft.pythonPaths.map((path) => path.trim()) } : {}),
    env: envOf(draft.env),
  };
}

/** What's wrong with a draft, by field. `handlers[i]` matches the i-th handler field. */
export interface DraftProblems {
  readonly command?: string;
  readonly schedule?: string;
  readonly application?: string;
  readonly target?: string;
  readonly pythonPaths?: string;
  readonly env?: string;
  readonly trigger?: string;
  readonly handlers: readonly (string | null)[];
  /** A problem with the handlers as a whole (none given, mixed languages). */
  readonly handlerList?: string;
}

export function hasProblems({ handlers, ...fields }: DraftProblems): boolean {
  return handlers.some((problem) => problem !== null) || Object.keys(fields).length > 0;
}

function targetProblem(target: TargetDraft): string | undefined {
  const missing =
    target.type === "file"
      ? target.path.trim() === ""
      : target.module.trim() === "" || (target.type !== "python_module" && target.attribute.trim() === "");
  return missing ? (target.type === "file" ? "Give the file the launch runs." : "Give the module and, for a callable or app, its attribute.") : undefined;
}

function handlerProblems(draft: LaunchDraft): Pick<DraftProblems, "handlers" | "handlerList"> {
  const handlers = draft.handlers.map((handler) => (handler.trim() === "" ? null : locatorProblem(handler.trim())));
  const given = draft.handlers.map((handler) => handler.trim()).filter((handler) => handler !== "");
  const languages = new Set(given.map(locatorLanguage).filter((language) => language !== null));
  const handlerList =
    given.length === 0 ? "Add at least one handler." : languages.size > 1 ? "One entrance's handlers can't mix TypeScript and Python files." : undefined;
  return { handlers, ...(handlerList === undefined ? {} : { handlerList }) };
}

/**
 * Checks a draft as Rust will, so the form can say what to fix before saving. A `detected` launch
 * keeps its command, so only what an override sets is checked.
 */
export function validateDraft(draft: LaunchDraft, detected = false): DraftProblems {
  const problems: { -readonly [K in keyof DraftProblems]: DraftProblems[K] } = { handlers: draft.handlers.map(() => null) };
  if (draft.newApplication !== null && draft.newApplication.trim() === "") {
    problems.application = "Name the new application.";
  }
  if (draft.trigger !== "" && draft.trigger.trim() === "") {
    problems.trigger = "A trigger can't be blank. Leave it empty if it's unknown.";
  }
  if (draft.intent === "handler") {
    return { ...problems, ...handlerProblems(draft) };
  }
  if (!detected && draft.command.trim() === "" && draft.target === null && draft.startup === null) {
    problems.command = "Paste the command, or set the target by hand under Analysis details.";
  }
  if (draft.scheduled && draft.schedule.trim() === "") {
    problems.schedule = "Give the schedule, or turn it off.";
  }
  const target = draft.target ? targetProblem(draft.target) : undefined;
  if (target !== undefined) {
    problems.target = target;
  }
  if (draft.pythonPaths?.some((path) => path.trim() === "")) {
    problems.pythonPaths = "Fill in or remove the empty import path.";
  }
  const keys = draft.env.map((row) => row.key.trim());
  if (keys.some((key) => key === "")) {
    problems.env = "Each variable needs a name.";
  } else if (new Set(keys).size < keys.length) {
    problems.env = "Each variable can be set once.";
  }
  return problems;
}
