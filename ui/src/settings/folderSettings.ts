/**
 * The folder settings dialog's model: Python projects as the form holds them, the detectors people
 * can switch, and the `SettingsEdit`s that turn the saved settings into the edited ones.
 */

import type { DetectorId, PythonProjectView, SettingsEdit, SettingsView } from "../ipc/generated";

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
export interface ProjectDraft {
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
export function normalizeFolder(path: string): string {
  const parts = path
    .trim()
    .split("/")
    .filter((part) => part !== "" && part !== ".");
  return parts.length === 0 ? "." : parts.join("/");
}

/** A source root as written in the table: relative to the project's root (`get_folder_config` joins them). */
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

export function projectDraft(view: PythonProjectView): ProjectDraft {
  const root = normalizeFolder(view.root);
  return {
    saved: root,
    root,
    sourceRoots: (view.sourceRoots ?? []).map((sourceRoot) => relativeToRoot(root, sourceRoot)).join(", "),
    version: asVersion(view.version),
  };
}

export function newProject(): ProjectDraft {
  return { saved: null, root: "", sourceRoots: "", version: "3.14" };
}

function sourceRootList(text: string): string[] | null {
  const roots = text
    .split(",")
    .map((root) => root.trim())
    .filter((root) => root !== "");
  return roots.length === 0 ? null : roots;
}

/** Why a project row can't be saved, or null. */
export function projectProblem(project: ProjectDraft, all: readonly ProjectDraft[]): string | null {
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

/** The form's state. */
export interface FolderSettingsDraft {
  readonly projects: readonly ProjectDraft[];
  readonly disabled: ReadonlySet<DetectorId>;
  readonly include: readonly string[];
  readonly exclude: readonly string[];
}

export function draftOfSettings(settings: SettingsView): FolderSettingsDraft {
  return {
    projects: settings.pythonProjects.map(projectDraft),
    disabled: new Set(settings.disabledDetectors),
    include: settings.include,
    exclude: settings.exclude,
  };
}

function sameProject(a: ProjectDraft, b: ProjectDraft): boolean {
  return (
    normalizeFolder(a.root) === normalizeFolder(b.root) &&
    JSON.stringify(sourceRootList(a.sourceRoots)) === JSON.stringify(sourceRootList(b.sourceRoots)) &&
    a.version === b.version
  );
}

/** The edits that turn `saved` into `draft`, in an order Rust applies cleanly. */
export function editsBetween(saved: FolderSettingsDraft, draft: FolderSettingsDraft): SettingsEdit[] {
  const edits: SettingsEdit[] = [];
  for (const pattern of saved.include.filter((item) => !draft.include.includes(item))) {
    edits.push({ op: "remove_scope_pattern", list: "include", pattern });
  }
  for (const pattern of saved.exclude.filter((item) => !draft.exclude.includes(item))) {
    edits.push({ op: "remove_scope_pattern", list: "exclude", pattern });
  }
  for (const group of DETECTOR_GROUPS) {
    for (const { id } of group.detectors) {
      if (saved.disabled.has(id) !== draft.disabled.has(id)) {
        edits.push({ op: "set_detector", detector: id, enabled: !draft.disabled.has(id) });
      }
    }
  }
  // A saved project's root is fixed in the form: changing it is removing and adding a project.
  const kept = new Set(draft.projects.flatMap((project) => (project.saved === null ? [] : [project.saved])));
  for (const project of saved.projects) {
    if (project.saved !== null && !kept.has(project.saved)) {
      edits.push({ op: "remove_python_project", root: project.saved });
    }
  }
  for (const project of draft.projects) {
    const before = saved.projects.find((row) => row.saved !== null && row.saved === project.saved);
    if (!before || !sameProject(before, project)) {
      edits.push({
        op: "set_python_project",
        project: { root: normalizeFolder(project.root), sourceRoots: sourceRootList(project.sourceRoots), version: project.version },
      });
    }
  }
  return edits;
}
