/**
 * Project settings (redesign §4.11, artboard 08, V-16, V-70, V-71): an 880×700 dialog over the window
 * with four sections. General (the shared name, the folder, background updates, where the graph is
 * kept), Scope (the folder tree and its rules), Detectors (a switch each, by language), and Python
 * projects. Every change but background updates is staged and saved together with
 * `update_project_config`, all or nothing; Rust keeps the file's comments and formatting. Closing with
 * staged changes asks first.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { Badge, Button, ConfirmDialog, Dialog, IconButton, SegmentedControl, Switch, TextField, showToast } from "../components";
import {
  engineKey,
  toUnfoldError,
  useEngineQuery,
  type DetectorId,
  type FallbackReason,
  type GraphStatus,
  type ProjectConfig,
  type ProjectId,
} from "../ipc";
import { displayPath } from "../lib/format";
import { openInEditor, revealInFinder } from "../lib/files";
import { setBackgroundUpdates } from "../state/compile";
import type { ProjectSettingsSection } from "../state/overlays";
import { useProjectRow } from "../state/workspace";
import { applySettingsEdits, type SettingsProblemText } from "./edits";
import {
  DETECTOR_GROUPS,
  PYTHON_VERSIONS,
  draftOf,
  draftProblems,
  editsBetween,
  newPythonProject,
  type PythonDraft,
  type PythonVersion,
  type SettingsDraft,
} from "./projectSettings";
import { ScopeSection } from "./ScopeSection";
import { SettingsProblemBox } from "./SettingsProblemBox";
import "./projectSettings.css";

const CONFIG_PATH = ".unfold/config.toml";

const SECTIONS: readonly { readonly id: ProjectSettingsSection; readonly label: string }[] = [
  { id: "general", label: "General" },
  { id: "scope", label: "Scope" },
  { id: "detectors", label: "Detectors" },
  { id: "python", label: "Python projects" },
];

const VERSION_OPTIONS = PYTHON_VERSIONS.map((version) => ({ value: version, label: version }));

const FALLBACK_NOTES: Readonly<Record<FallbackReason, string>> = {
  read_only: "The folder is read-only, so the graph and drafts are kept in Unfold's app data.",
  network_volume: "The folder is on a network volume, so the graph and drafts are kept in Unfold's app data.",
  wal_unavailable: "The folder's disk can't hold the graph database safely, so it is kept in Unfold's app data.",
};

/** Where the graph is kept and how the folder is watched, in words (V-71). */
function storageNotes(status: GraphStatus): string[] {
  const notes = [status.fallback ? FALLBACK_NOTES[status.fallback] : "The graph and drafts are kept in the project's .unfold/local/ folder."];
  if (status.watching === "scanning") {
    notes.push("File events aren't available for this folder, so Unfold checks it for changes every few seconds.");
  } else if (status.watching === "off") {
    notes.push("The folder isn't watched while background updates are off.");
  }
  return notes;
}

function SectionHeading({ title, children }: { readonly title: string; readonly children: ReactNode }) {
  return (
    <div className="settings-heading">
      <h3 className="settings-heading-title">{title}</h3>
      <p className="settings-heading-text">{children}</p>
    </div>
  );
}

/** Rows of a labeled form: a label column and the control. */
function Row({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="settings-field">
      <span className="settings-field-label">{label}</span>
      <div className="settings-field-value">{children}</div>
    </div>
  );
}

function BackgroundSwitch({ project, status }: { readonly project: ProjectId; readonly status: GraphStatus | undefined }) {
  // Shown at once; the project's summary catches up through its events.
  const [pending, setPending] = useState<boolean | null>(null);
  const [seen, setSeen] = useState(status);
  if (seen !== status) {
    setSeen(status);
    setPending(null);
  }
  const on = pending ?? status?.backgroundUpdates ?? false;
  return (
    <div className="settings-switch-row">
      <Switch
        checked={on}
        disabled={status === undefined}
        label="Background updates"
        onChange={(next) => {
          setPending(next);
          setBackgroundUpdates(project, next).catch((error: unknown) => {
            setPending(null);
            showToast({ title: "Couldn't change background updates", body: toUnfoldError(error).message, tone: "danger" });
          });
        }}
      />
      <span>
        Update the graph as files change
        <span className="settings-hint settings-block">Applies now. When it's off, changes wait for Compile again.</span>
      </span>
    </div>
  );
}

interface GeneralProps {
  readonly project: ProjectId;
  readonly config: ProjectConfig;
  readonly draft: SettingsDraft;
  readonly nameError: string | null;
  readonly onName: (name: string) => void;
}

function GeneralSection({ project, config, draft, nameError, onName }: GeneralProps) {
  const summary = useEngineQuery("project_summary", { project });
  const row = useProjectRow(project);
  const path = summary.data?.path ?? row?.path ?? "";
  return (
    <>
      <SectionHeading title="General">The project's name, where it lives, and how its graph stays current.</SectionHeading>
      <div className="settings-fields">
        <Row label="Name">
          <TextField
            label="Name"
            hideLabel
            className="settings-grow"
            value={draft.name}
            disabled={config.settings === null}
            placeholder={draft.defaultName ?? undefined}
            error={nameError}
            hint="Boards in other projects name cards from this one by it. Empty uses the folder's name."
            onChange={(event) => {
              onName(event.target.value);
            }}
          />
        </Row>
        <Row label="Folder">
          <span className="settings-path" title={path}>
            {displayPath(path)}
          </span>
          <Button variant="secondary" size="sm" icon="reveal" onClick={() => void revealInFinder(project, "")}>
            Reveal in Finder
          </Button>
        </Row>
        <Row label="Background updates">
          <BackgroundSwitch project={project} status={summary.data?.status} />
        </Row>
        <Row label="Storage">
          <div className="settings-notes">
            {summary.data ? (
              storageNotes(summary.data.status).map((note) => <p key={note}>{note}</p>)
            ) : (
              <p>{summary.error ? summary.error.message : "Loading…"}</p>
            )}
          </div>
        </Row>
      </div>
    </>
  );
}

function DetectorsSection({ disabled, onToggle }: { readonly disabled: ReadonlySet<DetectorId>; readonly onToggle: (id: DetectorId, on: boolean) => void }) {
  return (
    <>
      <SectionHeading title="Detectors">What finds this project's entrances: routes, scripts, workers, and schedules.</SectionHeading>
      {DETECTOR_GROUPS.map((group) => (
        <section key={group.language} className="settings-group" aria-label={group.language}>
          <h4 className="settings-caps">{group.language}</h4>
          <ul className="settings-list">
            {group.detectors.map((detector) => (
              <li key={detector.id} className="settings-list-row">
                <span className="settings-list-label">{detector.label}</span>
                <code className="settings-list-id">{detector.id}</code>
                <Switch
                  checked={!disabled.has(detector.id)}
                  label={detector.label}
                  onChange={(on) => {
                    onToggle(detector.id, on);
                  }}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
      <section className="settings-group" aria-label="Always on">
        <h4 className="settings-caps">Always on</h4>
        <ul className="settings-list">
          <li className="settings-list-row">
            <span className="settings-list-label">
              Manual entrances
              <span className="settings-hint"> · remove an [[entrance]] table to drop one</span>
            </span>
            <code className="settings-list-id">manual</code>
            <Switch checked disabled label="Manual entrances" onChange={() => undefined} />
          </li>
        </ul>
      </section>
    </>
  );
}

interface PythonRowProps {
  readonly project: PythonDraft;
  readonly problem: string | null;
  readonly onChange: (next: PythonDraft) => void;
  readonly onRemove: () => void;
}

function PythonRow({ project, problem, onChange, onRemove }: PythonRowProps) {
  const name = project.saved === null ? "New project" : project.root === "." ? "This folder" : project.root;
  return (
    <div className="settings-python" role="group" aria-label={`Python project ${name}`}>
      <div className="settings-python-head">
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
          <span className="settings-python-root">
            <code>{project.root}</code>
            {problem ? <span className="text-field-error">{problem}</span> : null}
          </span>
        )}
        <IconButton icon="close" label={`Remove Python project ${name}`} size={24} onClick={onRemove} />
      </div>
      <div className="settings-python-fields">
        <TextField
          label="Source roots"
          placeholder="Default: src/, or the project folder"
          value={project.sourceRoots}
          hint="Folders imports start from, relative to the project, separated by commas."
          onChange={(event) => {
            onChange({ ...project, sourceRoots: event.target.value });
          }}
        />
        <div className="settings-python-version">
          <span className="text-field-label">Grammar</span>
          <SegmentedControl<PythonVersion>
            label={`Python grammar for ${name}`}
            variant="switch"
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

interface PythonProps {
  readonly projects: readonly PythonDraft[];
  readonly problems: readonly (string | null)[];
  readonly submitted: boolean;
  readonly onChange: (projects: readonly PythonDraft[]) => void;
}

function PythonSection({ projects, problems, submitted, onChange }: PythonProps) {
  return (
    <>
      <SectionHeading title="Python projects">
        Where each Python project's imports start. Unfold uses src/ for a src layout, otherwise the folder that holds
        pyproject.toml.
      </SectionHeading>
      {projects.length === 0 ? <p className="settings-empty">None listed: Unfold finds them on its own.</p> : null}
      {projects.map((project, position) => (
        <PythonRow
          // Saved projects keep their root; new rows have only their position.
          key={project.saved ?? `new:${position}`}
          project={project}
          // A new row's empty folder is pointed out only once Save is tried.
          problem={submitted || project.root.trim() !== "" ? (problems[position] ?? null) : null}
          onChange={(next) => {
            onChange(projects.map((item, at) => (at === position ? next : item)));
          }}
          onRemove={() => {
            onChange(projects.filter((_, at) => at !== position));
          }}
        />
      ))}
      <Button
        variant="ghost"
        icon="plus"
        className="settings-add"
        onClick={() => {
          onChange([...projects, newPythonProject()]);
        }}
      >
        Add Python project
      </Button>
    </>
  );
}

/** Shown in place of a section the file's error keeps from loading. */
function InvalidFile({ project, config }: { readonly project: ProjectId; readonly config: ProjectConfig }) {
  return (
    <div className="settings-invalid">
      <SettingsProblemBox
        problem={{
          code: "invalid_settings",
          message: `Fix the file before changing these settings here. ${config.error?.message ?? ""}`.trim(),
          line: null,
          column: null,
        }}
      />
      <Button variant="secondary" icon="external-link" onClick={() => void openInEditor(project, CONFIG_PATH, config.error?.line ?? null)}>
        Open in editor
      </Button>
    </div>
  );
}

interface FormProps {
  readonly project: ProjectId;
  readonly name: string;
  readonly config: ProjectConfig;
  readonly initialSection: ProjectSettingsSection;
  readonly onClose: () => void;
}

function SettingsForm({ project, name, config, initialSection, onClose }: FormProps) {
  const client = useQueryClient();
  // What the dialog opened with: only the changes made here become edits, even if the file changes.
  const [saved, setSaved] = useState(() => draftOf(config));
  const [draft, setDraft] = useState(saved);
  const [section, setSection] = useState(initialSection);
  const [problem, setProblem] = useState<SettingsProblemText | null>(null);
  const [saving, setSaving] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [changedOnDisk, setChangedOnDisk] = useState(false);
  const edits = editsBetween(saved, draft);
  const dirty = edits.length > 0;
  const problems = draftProblems(draft);

  // `config_changed` refetched the file: adopt it when nothing is staged, else say so (V-71).
  const [seenConfig, setSeenConfig] = useState(config);
  if (seenConfig !== config) {
    setSeenConfig(config);
    if (dirty) {
      setChangedOnDisk(true);
    } else {
      const next = draftOf(config);
      setSaved(next);
      setDraft(next);
    }
  }

  const update = (change: (previous: SettingsDraft) => SettingsDraft) => {
    setDraft(change);
    setProblem(null);
  };

  const close = () => {
    if (dirty && !saving) {
      setConfirming(true);
    } else {
      onClose();
    }
  };

  const save = async () => {
    setSubmitted(true);
    if (problems.name !== null) {
      setSection("general");
      return;
    }
    if (problems.python.some((item) => item !== null)) {
      setSection("python");
      return;
    }
    if (!dirty || saving) {
      return;
    }
    setSaving(true);
    const outcome = await applySettingsEdits(project, edits);
    setSaving(false);
    if (!outcome.ok) {
      setProblem(outcome.problem);
      return;
    }
    client.setQueryData<ProjectConfig>(engineKey("project_config", { project }), outcome.config);
    showToast({ title: `Saved ${outcome.config.name.value} settings`, body: "The graph updates in the background." });
    onClose();
  };

  const invalid = config.settings === null;
  let body: ReactNode;
  switch (section) {
    case "general":
      body = (
        <GeneralSection
          project={project}
          config={config}
          draft={draft}
          nameError={problems.name}
          onName={(text) => {
            update((previous) => ({ ...previous, name: text }));
          }}
        />
      );
      break;
    case "scope":
      body = (
        <>
          <SectionHeading title="Scope">What gets graphed. Ignored files are skipped as your .gitignore says.</SectionHeading>
          {invalid ? (
            <InvalidFile project={project} config={config} />
          ) : (
            <ScopeSection
              project={project}
              name={name}
              saved={saved.scope}
              draft={draft.scope}
              onChange={(scope) => {
                update((previous) => ({ ...previous, scope }));
              }}
            />
          )}
        </>
      );
      break;
    case "detectors":
      body = invalid ? (
        <InvalidFile project={project} config={config} />
      ) : (
        <DetectorsSection
          disabled={draft.disabled}
          onToggle={(id, on) => {
            update((previous) => {
              const disabled = new Set(previous.disabled);
              if (on) {
                disabled.delete(id);
              } else {
                disabled.add(id);
              }
              return { ...previous, disabled };
            });
          }}
        />
      );
      break;
    case "python":
      body = invalid ? (
        <InvalidFile project={project} config={config} />
      ) : (
        <PythonSection
          projects={draft.python}
          problems={problems.python}
          submitted={submitted}
          onChange={(python) => {
            update((previous) => ({ ...previous, python }));
          }}
        />
      );
      break;
  }

  return (
    <>
      <Dialog
        open
        onClose={close}
        title={`${name} settings`}
        description={
          <span className="settings-file">
            <code>{CONFIG_PATH} · shared with your team</code>
            {config.exists ? <Badge icon="check">File exists</Badge> : <Badge>Not created yet</Badge>}
          </span>
        }
        width={880}
        height={700}
        placement="window"
        className="project-settings"
        footer={
          <>
            <span className="project-settings-note">
              {changedOnDisk ? "The file changed on disk. Saving applies your changes to it." : "Changes are saved together. Comments in the file are kept."}
            </span>
            <Button variant="secondary" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" busy={saving} disabled={!dirty || saving} onClick={() => void save()}>
              Save
            </Button>
          </>
        }
      >
        <nav className="project-settings-nav" aria-label="Sections">
          {SECTIONS.map((item) => (
            <button
              key={item.id}
              type="button"
              className="project-settings-nav-item"
              aria-current={item.id === section ? "page" : undefined}
              onClick={() => {
                setSection(item.id);
              }}
            >
              {item.label}
            </button>
          ))}
        </nav>
        <div className="project-settings-pane" role="region" aria-label={SECTIONS.find((item) => item.id === section)?.label}>
          {problem ? <SettingsProblemBox problem={problem} /> : null}
          {body}
        </div>
      </Dialog>
      <ConfirmDialog
        open={confirming}
        danger
        title="Discard changes?"
        message={`Your changes to ${name} settings aren't saved.`}
        confirmLabel="Discard changes"
        cancelLabel="Keep editing"
        placement="window"
        onCancel={() => {
          setConfirming(false);
        }}
        onConfirm={onClose}
      />
    </>
  );
}

interface ProjectSettingsDialogProps {
  readonly project: ProjectId;
  readonly section?: ProjectSettingsSection | undefined;
  readonly onClose: () => void;
}

/** The project's settings, once its `project_config` is loaded. */
export function ProjectSettingsDialog({ project, section = "general", onClose }: ProjectSettingsDialogProps) {
  const config = useEngineQuery("project_config", { project });
  const row = useProjectRow(project);
  if (config.error) {
    return (
      <ConfirmDialog
        open
        title="Couldn't open the settings"
        message={config.error.message}
        confirmLabel="OK"
        placement="window"
        onConfirm={onClose}
        onCancel={onClose}
      />
    );
  }
  if (!config.data) {
    return null;
  }
  return <SettingsForm project={project} name={row?.name ?? config.data.name.value} config={config.data} initialSection={section} onClose={onClose} />;
}
