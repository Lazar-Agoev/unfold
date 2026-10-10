/**
 * The manual entrance editor (architecture §9.2, docs/entrances.md): add or edit one `[[entrance]]`
 * table in `.unfold/config.toml` (kind, trigger, method for HTTP, label, and handlers or, for a
 * process, its startup), and confirm removing one. Locators are checked while typing; Rust checks
 * the whole file again when the edit is applied and keeps its comments and formatting. The graph
 * picks the change up in the background.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useId, useRef, useState, type SyntheticEvent } from "react";
import { Button, ConfirmDialog, Dialog, IconButton, SegmentedControl, TextField, showToast } from "../components";
import {
  engineKey,
  useEngineQuery,
  type EntranceKind,
  type HttpMethod,
  type ManualEntranceView,
  type ProjectConfig,
  type ProjectId,
  type SettingsEdit,
} from "../ipc";
import type { DialogRequest } from "../state/overlays";
import { applySettingsEdits, removeManualEntrance, type SettingsProblemText } from "./edits";
import {
  ENTRANCE_KINDS,
  HTTP_METHODS,
  draftOf,
  emptyDraft,
  hasProblems,
  manualTableOf,
  specOf,
  validateDraft,
  type EntranceDraft,
} from "./entrance";
import { LOCATOR_FORMS, locatorFor } from "./locators";
import { SettingsProblemBox } from "./SettingsProblemBox";
import "./settings.css";

const KIND_OPTIONS = ENTRANCE_KINDS.map(({ value, label }) => ({ value, label }));

/** The locator forms, for the help under the handler fields. */
function LocatorHelp() {
  return (
    <details className="settings-help">
      <summary>Locator forms</summary>
      <dl>
        {LOCATOR_FORMS.map(({ form, selects }) => (
          <div key={form} className="settings-help-row">
            <dt>
              <code>{form}</code>
            </dt>
            <dd>{selects}</dd>
          </div>
        ))}
      </dl>
      <p>Paths are relative to the folder. A name keeps working after edits; a line number doesn't.</p>
    </details>
  );
}

type EditorRequest = Extract<DialogRequest, { readonly kind: "entranceEditor" }>;
type RemoveRequest = Extract<DialogRequest, { readonly kind: "removeEntrance" }>;

interface FormProps {
  readonly project: ProjectId;
  readonly initial: EntranceDraft;
  /** The table being edited, or null for a new one. */
  readonly index: number | null;
  readonly blocked: SettingsProblemText | null;
  readonly onDone: () => void;
}

function EntranceForm({ project, initial, index, blocked, onDone }: FormProps) {
  const client = useQueryClient();
  const summary = useEngineQuery("project_summary", { project });
  const formId = useId();
  const firstField = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState<EntranceDraft>(initial);
  const [touched, setTouched] = useState<ReadonlySet<string>>(new Set());
  const [submitted, setSubmitted] = useState(false);
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<SettingsProblemText | null>(blocked);
  const problems = validateDraft(draft);
  const shown = (field: string) => submitted || touched.has(field);
  const touch = (field: string) => () => {
    setTouched((previous) => new Set(previous).add(field));
  };
  const change = (next: Partial<EntranceDraft>) => {
    setDraft((previous) => ({ ...previous, ...next }));
    setProblem(null);
  };
  const setHandler = (position: number, value: string) => {
    change({ handlers: draft.handlers.map((handler, at) => (at === position ? value : handler)) });
  };
  const kindInfo = ENTRANCE_KINDS.find((kind) => kind.value === draft.kind);

  const save = async (event: SyntheticEvent) => {
    event.preventDefault();
    setSubmitted(true);
    if (hasProblems(problems) || saving) {
      return;
    }
    setSaving(true);
    const entrance = specOf(draft);
    const edit: SettingsEdit = index === null ? { op: "add_entrance", entrance } : { op: "replace_entrance", index, entrance };
    const outcome = await applySettingsEdits(project, [edit]);
    setSaving(false);
    if (!outcome.ok) {
      setProblem(outcome.problem);
      return;
    }
    client.setQueryData<ProjectConfig>(engineKey("project_config", { project }), outcome.config);
    const paused = summary.data?.status.backgroundUpdates === false;
    showToast({
      title: `${index === null ? "Added" : "Saved"} the entrance in .unfold/config.toml`,
      body: paused ? "Background updates are paused: compile to see it in the graph." : "The graph updates in the background.",
    });
    onDone();
  };

  return (
    <Dialog
      open
      onClose={onDone}
      title={index === null ? "Add entrance" : "Edit entrance"}
      description="For entrances the detectors can't see, such as CLI dispatch or a signal handler. Saved in .unfold/config.toml, keeping its comments and formatting."
      width={520}
      placement="window"
      initialFocus={firstField}
      footer={
        <>
          <Button variant="secondary" onClick={onDone}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form={formId} busy={saving} disabled={saving || blocked !== null}>
            {index === null ? "Add entrance" : "Save"}
          </Button>
        </>
      }
    >
      <form id={formId} className="settings-form" onSubmit={(event) => void save(event)} noValidate>
        <div className="settings-row">
          <span className="settings-label" id={`${formId}-kind`}>
            Kind
          </span>
          <SegmentedControl<EntranceKind>
            label="Kind"
            variant="switch"
            options={KIND_OPTIONS}
            value={draft.kind}
            onChange={(kind) => {
              change({ kind });
            }}
          />
        </div>
        <div className={draft.kind === "http" ? "settings-pair" : undefined}>
          {draft.kind === "http" ? (
            <label className="settings-select">
              <span className="settings-label">Method</span>
              <select
                value={draft.method}
                onChange={(event) => {
                  change({ method: event.target.value as HttpMethod | "" });
                }}
              >
                <option value="">Any or unknown</option>
                {HTTP_METHODS.map((method) => (
                  <option key={method} value={method}>
                    {method}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <TextField
            ref={firstField}
            label="Trigger"
            placeholder={kindInfo?.trigger}
            value={draft.trigger}
            error={shown("trigger") ? (problems.trigger ?? null) : null}
            hint="Optional. Leave it empty when it isn't known."
            onBlur={touch("trigger")}
            onChange={(event) => {
              change({ trigger: event.target.value });
            }}
          />
        </div>
        <TextField
          label="Label"
          placeholder="Shown in place of the trigger"
          value={draft.label}
          hint="Optional."
          onChange={(event) => {
            change({ label: event.target.value });
          }}
        />
        {draft.kind === "process" ? (
          <TextField
            label="Startup"
            placeholder="src/server.ts"
            className="settings-locator"
            value={draft.startup}
            error={shown("startup") ? (problems.startup ?? null) : null}
            hint="The file the process starts from."
            onBlur={touch("startup")}
            onChange={(event) => {
              change({ startup: event.target.value });
            }}
          />
        ) : (
          <fieldset className="settings-handlers">
            <legend className="settings-label">Handlers</legend>
            <p className="settings-note">In the order they run, middleware first.</p>
            {draft.handlers.map((handler, position) => (
              // Handlers have no identity besides their position while they're typed.
              <div key={position} className="settings-handler">
                <TextField
                  label={`Handler ${position + 1}`}
                  hideLabel
                  placeholder="src/cli.ts#runReport"
                  className="settings-locator"
                  value={handler}
                  error={shown(`handler:${position}`) ? (problems.handlers[position] ?? null) : null}
                  onBlur={touch(`handler:${position}`)}
                  onChange={(event) => {
                    setHandler(position, event.target.value);
                  }}
                />
                <IconButton
                  icon="back"
                  className="settings-handler-earlier"
                  label={`Run handler ${position + 1} earlier`}
                  size={24}
                  disabled={position === 0}
                  onClick={() => {
                    const next = [...draft.handlers];
                    [next[position - 1], next[position]] = [next[position] ?? "", next[position - 1] ?? ""];
                    change({ handlers: next });
                  }}
                />
                <IconButton
                  icon="close"
                  label={`Remove handler ${position + 1}`}
                  size={24}
                  disabled={draft.handlers.length === 1}
                  onClick={() => {
                    change({ handlers: draft.handlers.filter((_, at) => at !== position) });
                  }}
                />
              </div>
            ))}
            {submitted && problems.handlerList ? <p className="text-field-error">{problems.handlerList}</p> : null}
            <Button
              size="sm"
              variant="ghost"
              icon="plus"
              onClick={() => {
                change({ handlers: [...draft.handlers, ""] });
              }}
            >
              Add handler
            </Button>
          </fieldset>
        )}
        <LocatorHelp />
        {problem ? <SettingsProblemBox problem={problem} /> : null}
      </form>
    </Dialog>
  );
}

/** The table a request edits, once the settings are loaded. */
function resolveTarget(request: EditorRequest, config: ProjectConfig): { draft: EntranceDraft; index: number | null } | null {
  if (request.mode === "add") {
    return { draft: emptyDraft(request.handler ? locatorFor(request.handler) : undefined), index: null };
  }
  const table: ManualEntranceView | null = manualTableOf(request.entrance, config.settings?.entrances ?? []);
  return table ? { draft: draftOf(table), index: table.index } : null;
}

function blockedBy(config: ProjectConfig): SettingsProblemText | null {
  return config.error
    ? { code: "invalid_settings", message: `Fix .unfold/config.toml first: ${config.error.message}`, line: null, column: null }
    : null;
}

/** The manual entrance editor for a project: a new entrance, or one of its `[[entrance]]` tables. */
export function EntranceEditorDialog({ request, onClose }: { readonly request: EditorRequest; readonly onClose: () => void }) {
  const config = useEngineQuery("project_config", { project: request.project });
  if (!config.data) {
    return null;
  }
  const resolved = resolveTarget(request, config.data);
  return resolved ? (
    <EntranceForm project={request.project} initial={resolved.draft} index={resolved.index} blocked={blockedBy(config.data)} onDone={onClose} />
  ) : (
    <ConfirmDialog
      open
      title="Entrance not found"
      message="This entrance isn't in .unfold/config.toml any more. The graph may not have caught up with an edit yet."
      confirmLabel="OK"
      placement="window"
      onConfirm={onClose}
      onCancel={onClose}
    />
  );
}

/** Asks before removing a manual entrance from a project's `.unfold/config.toml`. */
export function RemoveEntranceDialog({ request, onClose }: { readonly request: RemoveRequest; readonly onClose: () => void }) {
  const { project, entrance } = request;
  return (
    <ConfirmDialog
      open
      danger
      title="Remove entrance"
      message={`Remove ${entrance.title} from .unfold/config.toml? Everyone who shares the repository loses it too.`}
      confirmLabel="Remove"
      placement="window"
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        void removeManualEntrance(project, entrance, (loaded) => manualTableOf(entrance, loaded.settings?.entrances ?? [])?.index ?? null);
      }}
    />
  );
}
