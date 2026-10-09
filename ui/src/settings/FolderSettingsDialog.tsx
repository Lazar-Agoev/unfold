/**
 * Folder Settings (architecture §9.2): the shared `.unfold/config.toml` of the open folder. Python
 * projects (root, source roots, grammar version), which detectors run, and the scope patterns with
 * remove buttons (adding them stays in the Files view's context menu). Every change is staged and
 * saved at once, all or nothing, through Rust, which keeps the file's comments and formatting and
 * writes it only when a setting differs from the defaults.
 */

import { useQueryClient } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, IconButton, SegmentedControl, TextField, showToast } from "../components";
import type { DetectorId, FolderConfig } from "../ipc/generated";
import { engineKey, useEngineQuery } from "../ipc/queries";
import { openInEditor } from "../views/actions";
import {
  DETECTOR_GROUPS,
  PYTHON_VERSIONS,
  draftOfSettings,
  editsBetween,
  newProject,
  projectProblem,
  type FolderSettingsDraft,
  type ProjectDraft,
  type PythonVersion,
} from "./folderSettings";
import { SettingsProblemBox } from "./SettingsProblemBox";
import { applySettingsEdits, closeFolderSettings, useSettingsDialogs, type SettingsProblemText } from "./store";
import "./settings.css";

const CONFIG_PATH = ".unfold/config.toml";
const VERSION_OPTIONS = PYTHON_VERSIONS.map((version) => ({ value: version, label: version }));

function FileNote({ config }: { readonly config: FolderConfig }) {
  return (
    <p className="settings-file">
      <code>{CONFIG_PATH}</code>
      <span>
        {config.exists
          ? "Shared with everyone through the repository."
          : "Not created yet: every setting is at its default. It's written, to share through the repository, once a setting differs."}
      </span>
    </p>
  );
}

interface ProjectRowProps {
  readonly project: ProjectDraft;
  readonly problem: string | null;
  readonly onChange: (next: ProjectDraft) => void;
  readonly onRemove: () => void;
}

function ProjectRow({ project, problem, onChange, onRemove }: ProjectRowProps) {
  const name = project.saved === null ? "New project" : project.root === "." ? "This folder" : project.root;
  return (
    <div className="settings-project" role="group" aria-label={`Python project ${name}`}>
      <div className="settings-project-head">
        {project.saved === null ? (
          <TextField
            label="Folder"
            placeholder=". for this folder, or py-orders"
            value={project.root}
            error={problem}
            onChange={(event) => {
              onChange({ ...project, root: event.target.value });
            }}
          />
        ) : (
          <span className="settings-project-root">
            <code>{project.root}</code>
            {problem ? <span className="text-field-error">{problem}</span> : null}
          </span>
        )}
        <IconButton icon={X} label={`Remove Python project ${name}`} size="small" onClick={onRemove} />
      </div>
      <div className="settings-pair">
        <TextField
          label="Source roots"
          placeholder="Default: src/, or the project folder"
          value={project.sourceRoots}
          hint="Folders imports start from, relative to the project, separated by commas."
          onChange={(event) => {
            onChange({ ...project, sourceRoots: event.target.value });
          }}
        />
        <div className="settings-field">
          <span className="settings-label">Grammar</span>
          <SegmentedControl<PythonVersion>
            label={`Python grammar for ${name}`}
            size="small"
            options={VERSION_OPTIONS}
            value={project.version}
            onChange={(version) => {
              onChange({ ...project, version });
            }}
          />
        </div>
      </div>
    </div>
  );
}

function PatternList({ title, patterns, onRemove }: { readonly title: string; readonly patterns: readonly string[]; readonly onRemove: (pattern: string) => void }) {
  return (
    <div className="settings-patterns">
      <span className="settings-label">{title}</span>
      {patterns.length === 0 ? (
        <p className="settings-note">None.</p>
      ) : (
        <ul>
          {patterns.map((pattern) => (
            <li key={pattern}>
              <code>{pattern}</code>
              <IconButton
                icon={X}
                label={`Remove ${pattern} from ${title.toLowerCase()}`}
                size="small"
                onClick={() => {
                  onRemove(pattern);
                }}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function SettingsForm({ config, initial }: { readonly config: FolderConfig; readonly initial: FolderSettingsDraft }) {
  const client = useQueryClient();
  // What the form opened with: only the changes made here become edits, even if the file changes
  // meanwhile.
  const [saved] = useState(initial);
  const [draft, setDraft] = useState<FolderSettingsDraft>(initial);
  const [saving, setSaving] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [problem, setProblem] = useState<SettingsProblemText | null>(null);
  const projectProblems = draft.projects.map((project) => projectProblem(project, draft.projects));
  const edits = editsBetween(saved, draft);
  const invalid = projectProblems.some((item) => item !== null);

  const update = (change: (previous: FolderSettingsDraft) => FolderSettingsDraft) => {
    setDraft(change);
    setProblem(null);
  };
  const setProject = (position: number, next: ProjectDraft) => {
    update((previous) => ({ ...previous, projects: previous.projects.map((project, at) => (at === position ? next : project)) }));
  };
  const toggleDetector = (id: DetectorId, enabled: boolean) => {
    update((previous) => {
      const disabled = new Set(previous.disabled);
      if (enabled) {
        disabled.delete(id);
      } else {
        disabled.add(id);
      }
      return { ...previous, disabled };
    });
  };

  const save = async () => {
    setSubmitted(true);
    if (invalid || saving) {
      return;
    }
    if (edits.length === 0) {
      closeFolderSettings();
      return;
    }
    setSaving(true);
    const outcome = await applySettingsEdits(edits);
    setSaving(false);
    if (!outcome.ok) {
      setProblem(outcome.problem);
      return;
    }
    client.setQueryData<FolderConfig>(engineKey("get_folder_config", {}), outcome.config);
    showToast({ message: "Saved the folder settings. The graph updates in the background.", tone: "success" });
    closeFolderSettings();
  };

  return (
    <Dialog
      open
      onClose={closeFolderSettings}
      title="Folder Settings"
      description={<FileNote config={config} />}
      width="medium"
      className="settings-dialog"
      footer={
        <>
          <Button variant="quiet" onClick={closeFolderSettings}>
            Cancel
          </Button>
          <Button variant="primary" busy={saving} disabled={saving} onClick={() => void save()}>
            Save
          </Button>
        </>
      }
    >
      <div className="settings-form">
        <section className="settings-section" aria-labelledby="settings-python">
          <h3 id="settings-python" className="settings-heading">
            Python projects
          </h3>
          {draft.projects.length === 0 ? (
            <p className="settings-note">None. Unfold uses src/ for a src layout, otherwise the folder that holds pyproject.toml.</p>
          ) : null}
          {draft.projects.map((project, position) => (
            <ProjectRow
              // Saved projects keep their root; new rows have only their position.
              key={project.saved ?? `new:${position}`}
              project={project}
              // A new row's empty folder is pointed out only once Save is tried.
              problem={submitted || project.root.trim() !== "" ? (projectProblems[position] ?? null) : null}
              onChange={(next) => {
                setProject(position, next);
              }}
              onRemove={() => {
                update((previous) => ({ ...previous, projects: previous.projects.filter((_, at) => at !== position) }));
              }}
            />
          ))}
          <Button
            size="small"
            variant="quiet"
            icon={Plus}
            onClick={() => {
              update((previous) => ({ ...previous, projects: [...previous.projects, newProject()] }));
            }}
          >
            Add Python project
          </Button>
        </section>

        <section className="settings-section" aria-labelledby="settings-detectors">
          <h3 id="settings-detectors" className="settings-heading">
            Detectors
          </h3>
          <div className="settings-detectors">
            {DETECTOR_GROUPS.map((group) => (
              <fieldset key={group.language}>
                <legend className="settings-label">{group.language}</legend>
                {group.detectors.map((detector) => (
                  <label key={detector.id} className="settings-check">
                    <input
                      type="checkbox"
                      checked={!draft.disabled.has(detector.id)}
                      onChange={(event) => {
                        toggleDetector(detector.id, event.target.checked);
                      }}
                    />
                    <span>
                      {detector.label} <code className="settings-id">{detector.id}</code>
                    </span>
                  </label>
                ))}
              </fieldset>
            ))}
          </div>
          <label className="settings-check settings-check-fixed">
            <input type="checkbox" checked disabled />
            <span>
              Manual entrances <code className="settings-id">manual</code>
              <span className="settings-note">Always on. Remove [[entrance]] tables to drop them.</span>
            </span>
          </label>
        </section>

        <section className="settings-section" aria-labelledby="settings-scope">
          <h3 id="settings-scope" className="settings-heading">
            Scope
          </h3>
          <p className="settings-note">Add rules from the Files view: Exclude from Graph, Include in Graph, Graph Only This Folder.</p>
          <div className="settings-pair">
            <PatternList
              title="Graph only"
              patterns={draft.include}
              onRemove={(pattern) => {
                update((previous) => ({ ...previous, include: previous.include.filter((item) => item !== pattern) }));
              }}
            />
            <PatternList
              title="Excluded"
              patterns={draft.exclude}
              onRemove={(pattern) => {
                update((previous) => ({ ...previous, exclude: previous.exclude.filter((item) => item !== pattern) }));
              }}
            />
          </div>
        </section>
        {problem ? <SettingsProblemBox problem={problem} /> : null}
      </div>
    </Dialog>
  );
}

function InvalidSettings({ config }: { readonly config: FolderConfig }) {
  const error = config.error;
  return (
    <Dialog
      open
      onClose={closeFolderSettings}
      title="Folder Settings"
      description={<FileNote config={config} />}
      width="medium"
      footer={
        <>
          <Button variant="quiet" onClick={closeFolderSettings}>
            Close
          </Button>
          <Button variant="primary" onClick={() => void openInEditor(CONFIG_PATH, error?.line ?? null)}>
            Open in Editor
          </Button>
        </>
      }
    >
      <SettingsProblemBox
        problem={{
          code: "invalid_settings",
          message: `Fix the file before changing settings here. ${error?.message ?? ""}`.trim(),
          line: null,
          column: null,
        }}
      />
    </Dialog>
  );
}

/** The dialog; mounted once per window. */
export function FolderSettingsDialog() {
  const open = useSettingsDialogs((state) => state.folderSettings);
  const config = useEngineQuery("get_folder_config", {}, { enabled: open });
  if (!open || !config.data) {
    return null;
  }
  if (!config.data.settings) {
    return <InvalidSettings config={config.data} />;
  }
  return <SettingsForm config={config.data} initial={draftOfSettings(config.data.settings)} />;
}
