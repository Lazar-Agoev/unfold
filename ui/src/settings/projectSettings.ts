/**
 * Project settings' model: the form's staged state (the shared name, scope
 * lists, detectors, Python projects), the detectors people can switch, and the `SettingsEdit`s that
 * turn the settings the dialog opened with into the staged ones. Rust applies them all or none.
 */

import type { DetectorId, ProjectConfig, PythonProjectView, SettingsEdit } from "../ipc";
import { literalOf, type ScopeLists } from "./scope";

export const PYTHON_VERSIONS = ["3.12", "3.13", "3.14"] as const;
export type PythonVersion = (typeof PYTHON_VERSIONS)[number];

/** The detectors, by language. `manual` is listed apart: it can't be turned off. */
export const DETECTOR_GROUPS: readonly { readonly language: string; readonly detectors: readonly { readonly id: DetectorId; readonly label: string }[] }[] = [
  {
    language: "TypeScript",
    detectors: [
      { id: "package.script", label: "package.json scripts" },
      { id: "package.bin", label: "package.json bin" },
      { id: "express.route", label: "Express routes" },
      { id: "bullmq.worker", label: "BullMQ workers" },
      { id: "node-cron.schedule", label: "node-cron schedules" },
    ],
  },
  {
    language: "Python",
    detectors: [
      { id: "pyproject.script", label: "pyproject.toml scripts" },
      { id: "fastapi.route", label: "FastAPI routes" },
      { id: "celery.task", label: "Celery tasks" },
      { id: "apscheduler.job", label: "APScheduler jobs" },
    ],
  },
];

/** One `[[python.project]]` row in the form. */
export interface PythonDraft {
  /** The saved root this row edits, or null for a new project. */
  readonly saved: string | null;
  readonly root: string;
  /** Comma-separated folders relative to the root; empty for the front end's default. */
  readonly sourceRoots: string;
  readonly version: PythonVersion;
}

function asVersion(version: string): PythonVersion {
  return (PYTHON_VERSIONS as readonly string[]).includes(version) ? (version as PythonVersion) : "3.14";
}

/** `.` (or empty) and `./x/` both name folders; compare them normalized. */
function normalizeFolder(path: string): string {
  const parts = path
    .trim()
    .split("/")
    .filter((part) => part !== "" && part !== ".");
  return parts.length === 0 ? "." : parts.join("/");
}

/** A source root as written in the table: relative to the project's root (`project_config` joins them). */
export function relativeToRoot(root: string, sourceRoot: string): string {
  const base = normalizeFolder(root);
  const full = normalizeFolder(sourceRoot);
  if (base === ".") {
    return full;
  }
  if (full === base) {
    return ".";
  }
  return full.startsWith(`${base}/`) ? full.slice(base.length + 1) : full;
}

function pythonDraft(view: PythonProjectView): PythonDraft {
  const root = normalizeFolder(view.root);
  return {
    saved: root,
    root,
    sourceRoots: (view.sourceRoots ?? []).map((sourceRoot) => relativeToRoot(root, sourceRoot)).join(", "),
    version: asVersion(view.version),
  };
}

export function newPythonProject(): PythonDraft {
  return { saved: null, root: "", sourceRoots: "", version: "3.14" };
}

function sourceRootList(text: string): string[] | null {
  const roots = text
    .split(",")
    .map((root) => root.trim())
    .filter((root) => root !== "");
  return roots.length === 0 ? null : roots;
}

/** Why a Python project row can't be saved, or null. */
function pythonProblem(project: PythonDraft, all: readonly PythonDraft[]): string | null {
  if (project.root.trim() === "") {
    return "Name the project's folder, relative to this one (. for this folder).";
  }
  const root = normalizeFolder(project.root);
  if (root.split("/").includes("..") || project.root.trim().startsWith("/")) {
    return "The folder must be inside this one.";
  }
  if (all.filter((other) => other.root.trim() !== "" && normalizeFolder(other.root) === root).length > 1) {
    return "Two projects can't share a folder.";
  }
  return null;
}

const PROJECT_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;

/** Why a shared name can't be saved, or null. Empty means "the default". */
function nameProblem(name: string): string | null {
  const trimmed = name.trim();
  return trimmed === "" || PROJECT_NAME.test(trimmed)
    ? null
    : "Use up to 64 letters, digits, “.”, “_”, and “-”, starting with a letter or digit.";
}

/** The form's state. */
export interface SettingsDraft {
  /** The shared name as typed; empty means the default made from the folder. */
  readonly name: string;
  /** The name when it is the folder's default (no `[project] name`), else null. */
  readonly defaultName: string | null;
  readonly scope: ScopeLists;
  readonly disabled: ReadonlySet<DetectorId>;
  readonly python: readonly PythonDraft[];
}

/** The form's starting state from a project's config (an invalid file starts from the defaults). */
export function draftOf(config: ProjectConfig): SettingsDraft {
  const settings = config.settings;
  return {
    name: config.name.value,
    defaultName: config.name.isDefault ? config.name.value : null,
    scope: { include: settings?.include ?? [], exclude: settings?.exclude ?? [] },
    disabled: new Set(settings?.disabledDetectors ?? []),
    python: (settings?.pythonProjects ?? []).map(pythonDraft),
  };
}

function samePython(a: PythonDraft, b: PythonDraft): boolean {
  return (
    normalizeFolder(a.root) === normalizeFolder(b.root) &&
    JSON.stringify(sourceRootList(a.sourceRoots)) === JSON.stringify(sourceRootList(b.sourceRoots)) &&
    a.version === b.version
  );
}

function scopeEdits(saved: ScopeLists, draft: ScopeLists): SettingsEdit[] {
  const edits: SettingsEdit[] = [];
  for (const list of ["include", "exclude"] as const) {
    for (const pattern of saved[list].filter((item) => !draft[list].includes(item))) {
      edits.push({ op: "remove_scope_pattern", list, pattern });
    }
  }
  // The dialog only adds exact folder paths, written by Rust as it writes the Files actions'.
  for (const [list, op] of [
    ["exclude", "exclude_from_graph"],
    ["include", "include_in_graph"],
  ] as const) {
    for (const pattern of draft[list].filter((item) => !saved[list].includes(item))) {
      edits.push({ op, path: literalOf(pattern) ?? pattern });
    }
  }
  return edits;
}

/** The edits that turn `saved` into `draft`, in an order Rust applies cleanly. */
export function editsBetween(saved: SettingsDraft, draft: SettingsDraft): SettingsEdit[] {
  const edits = scopeEdits(saved.scope, draft.scope);
  for (const group of DETECTOR_GROUPS) {
    for (const { id } of group.detectors) {
      if (saved.disabled.has(id) !== draft.disabled.has(id)) {
        edits.push({ op: "set_detector", detector: id, enabled: !draft.disabled.has(id) });
      }
    }
  }
  // A saved project's root is fixed in the form: changing it is removing and adding a project.
  const kept = new Set(draft.python.flatMap((project) => (project.saved === null ? [] : [project.saved])));
  for (const project of saved.python) {
    if (project.saved !== null && !kept.has(project.saved)) {
      edits.push({ op: "remove_python_project", root: project.saved });
    }
  }
  for (const project of draft.python) {
    const before = saved.python.find((row) => row.saved !== null && row.saved === project.saved);
    if (!before || !samePython(before, project)) {
      edits.push({
        op: "set_python_project",
        project: { root: normalizeFolder(project.root), sourceRoots: sourceRootList(project.sourceRoots), version: project.version },
      });
    }
  }
  // An emptied field means the default name, which is the saved one when the file sets none.
  const name = draft.name.trim() === "" ? (saved.defaultName ?? "") : draft.name.trim();
  if (name !== saved.name.trim()) {
    edits.push({ op: "set_project_name", name: name === "" ? null : name });
  }
  return edits;
}

/** Why the draft can't be saved yet: the name's problem and each Python row's (null when fine). */
export function draftProblems(draft: SettingsDraft): { readonly name: string | null; readonly python: readonly (string | null)[] } {
  return { name: nameProblem(draft.name), python: draft.python.map((project) => pythonProblem(project, draft.python)) };
}
