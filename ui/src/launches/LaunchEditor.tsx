/**
 * The launch editor (docs/proposals/configured-entrances.md, "Entrance editor"): a 1000×720 dialog
 * that adds or edits one `[[entrance]]` table in `unfold.toml`, either a launch ("Starts a
 * process": its command, folder, application, schedule, and Analysis details, beside a live preview
 * of what Unfold reads from it) or a handler entrance ("Handles a route, queue, or event"). It also
 * edits a detected launch's `[[entrance_override]]`, and steps through the launches a deploy-file
 * import found, one save each. Rust checks the whole file when the edit is applied and keeps its
 * comments and formatting; the graph picks the change up in the background.
 */

import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState, type SyntheticEvent } from "react";
import { Button, ConfirmDialog, Dialog, SegmentedControl, Switch, showToast } from "../components";
import {
  engineKey,
  engineQueryOptions,
  useEngineQuery,
  type LaunchCheck,
  type LaunchFill,
  type LaunchRow,
  type ProjectConfig,
  type ProjectId,
  type ProjectSummary,
  type SettingsEdit,
} from "../ipc";
import type { DialogRequest } from "../state/overlays";
import { applySettingsEdits, type SettingsProblemText } from "../settings/edits";
import { SettingsProblemBox } from "../settings/SettingsProblemBox";
import { AnalysisDetails } from "./AnalysisDetails";
import { ApplicationPicker } from "./ApplicationPicker";
import { findLaunch } from "./catalog";
import {
  checkSpecOf,
  draftOfImport,
  draftOfOverride,
  draftOfPrefill,
  draftOfTable,
  effectiveApplication,
  emptyTarget,
  entranceId,
  hasProblems,
  newDraft,
  overrideOf,
  sameMatch,
  specOf,
  targetDraftOf,
  uniqueId,
  validateDraft,
  type Intent,
  type LaunchDraft,
} from "./draft";
import { Field } from "./Field";
import { FolderPicker } from "./FolderPicker";
import { HandlerFields, LocatorForms } from "./HandlerFields";
import { locatorFor } from "./locators";
import { reportSaved, waitsForCompile } from "./outcome";
import { DetectedPreview, LaunchPreview, type Fill } from "./Preview";
import { REVIEW_START, advance, reviewSummary, type ReviewProgress } from "./review";
import "./launches.css";

/** How long typing settles before the preview asks again. */
const CHECK_DELAY = 250;

const INTENTS: readonly { readonly value: Intent; readonly label: string }[] = [
  { value: "process", label: "Starts a process" },
  { value: "handler", label: "Handles a route, queue, or event" },
];

type EditorRequest = Extract<DialogRequest, { readonly kind: "launchEditor" }>;

/** How a save writes the entrance. */
type SaveAs =
  /** A new `[[entrance]]`; its ID is generated from the draft when it has none yet. */
  | { readonly kind: "add"; readonly id: string | null }
  | { readonly kind: "replace"; readonly id: string; readonly line: number }
  /** A detected launch's `[[entrance_override]]`. */
  | { readonly kind: "override"; readonly row: LaunchRow };

function useDebounced<T>(value: T, delay: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setSettled(value);
    }, delay);
    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);
  return settled;
}

/** What Unfold reads from the launch as it's typed: `check_launch`, asked again once typing settles. */
function useLaunchCheck(project: ProjectId, draft: LaunchDraft, id: string | null, enabled: boolean) {
  const spec = useMemo(() => checkSpecOf(draft, id), [draft, id]);
  const settled = useDebounced(spec, CHECK_DELAY);
  // Keyed by the draft: a result for an older draft never shows once a newer one is asked for.
  return useQuery({ ...engineQueryOptions("check_launch", { project, draft: settled }), enabled, placeholderData: keepPreviousData, gcTime: 0 });
}

/** Whether the draft's target and paths are what the quoted part of `sh -c` fills in. */
function isFilledFrom(draft: LaunchDraft, suggestion: LaunchFill): boolean {
  const target = suggestion.target ? targetDraftOf(suggestion.target) : null;
  return JSON.stringify(draft.target) === JSON.stringify(target) && JSON.stringify(draft.pythonPaths ?? []) === JSON.stringify(suggestion.pythonPaths);
}

/** A fill from the quoted part of `sh -c`, with what it replaced (Undo puts that back). */
interface AppliedFill extends Fill {
  readonly before: Pick<LaunchDraft, "target" | "pythonPaths">;
}

interface FormProps {
  readonly project: ProjectId;
  readonly config: ProjectConfig;
  readonly initial: LaunchDraft;
  readonly saveAs: SaveAs;
  /** Where the command came from (`data-pipelines/Dockerfile.workers, CMD`). */
  readonly copiedFrom: string | null;
  /** A deploy-file import's note about the launch. */
  readonly note: string | null;
  /** The launch's place in a review ("1 of 14"). */
  readonly step: { readonly index: number; readonly total: number } | null;
  readonly onSaved: (saved: { readonly name: string; readonly launchId: string | null }) => void;
  /** Review only: leaves this launch out and goes to the next. */
  readonly onSkip?: () => void;
  readonly onClose: () => void;
}

function LaunchForm({ project, config, initial, saveAs, copiedFrom, note, step, onSaved, onSkip, onClose }: FormProps) {
  const client = useQueryClient();
  const id = useId();
  const command = useRef<HTMLTextAreaElement>(null);
  const trigger = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(initial);
  const [touched, setTouched] = useState<ReadonlySet<string>>(new Set());
  const [submitted, setSubmitted] = useState(false);
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<SettingsProblemText | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(initial.target !== null || initial.pythonPaths !== null || initial.env.length > 0);
  const [fill, setFill] = useState<AppliedFill | null>(null);
  const [seenCheck, setSeenCheck] = useState<LaunchCheck | undefined>(undefined);

  const override = saveAs.kind === "override" ? saveAs.row : null;
  const process = draft.intent === "process";
  const adding = saveAs.kind === "add";
  const declared = config.settings?.applications ?? [];
  const projectName = config.name.value;
  const problems = validateDraft(draft, override !== null);
  const shown = (field: string) => submitted || touched.has(field);
  const touch = (field: string) => () => {
    setTouched((previous) => new Set(previous).add(field));
  };
  const change = (next: Partial<LaunchDraft>) => {
    setDraft((previous) => ({ ...previous, ...next }));
    setProblem(null);
  };

  const check = useLaunchCheck(project, draft, saveAs.kind === "replace" ? saveAs.id : null, process && override === null);
  const result = process && override === null ? check.data : undefined;
  const fillFrom = (suggestion: LaunchFill) => {
    setFill({ quoted: suggestion.quoted, filled: true, before: { target: draft.target, pythonPaths: draft.pythonPaths } });
    change({
      target: suggestion.target ? targetDraftOf(suggestion.target) : draft.target,
      pythonPaths: suggestion.pythonPaths.length > 0 ? suggestion.pythonPaths : draft.pythonPaths,
    });
    setDetailsOpen(true);
  };
  // Opening a launch Unfold can't read fills its target and paths from the quoted part, once; a
  // saved launch whose target and paths are the quoted part's shows them as filled from it.
  if (check.data !== seenCheck && !check.isPlaceholderData) {
    setSeenCheck(check.data);
    const suggestion = seenCheck === undefined ? check.data?.suggestion : undefined;
    if (suggestion && draft.target === null && draft.pythonPaths === null) {
      fillFrom(suggestion);
    } else if (suggestion && isFilledFrom(draft, suggestion)) {
      setFill({ quoted: suggestion.quoted, filled: true, before: { target: null, pythonPaths: null } });
    }
  }
  const inferredPaths = result && draft.pythonPaths === null ? result.importPaths.map((row) => row.path) : [];

  const blocked: SettingsProblemText | null = config.error
    ? { code: "invalid_settings", message: `Fix unfold.toml first: ${config.error.message}`, line: null, column: null }
    : null;
  const shownProblem = problem ?? blocked;

  const save = async (event: SyntheticEvent) => {
    event.preventDefault();
    setSubmitted(true);
    if (blocked !== null || saving || hasProblems(problems) || override?.match === null) {
      return;
    }
    setSaving(true);
    const application = effectiveApplication(draft, declared);
    const edits: SettingsEdit[] =
      application && !declared.some((candidate) => candidate.id === application.id) ? [{ op: "set_application", application }] : [];
    let entrance: string | null = null;
    if (saveAs.kind === "override") {
      if (saveAs.row.match) {
        edits.push({ op: "set_override", override: overrideOf(draft, saveAs.row.match, declared) });
      }
    } else {
      const taken = (config.settings?.entrances ?? []).map((table) => table.id);
      entrance = saveAs.kind === "replace" ? saveAs.id : (saveAs.id ?? entranceId(draft, taken, declared));
      const spec = specOf(draft, entrance, declared);
      edits.push(saveAs.kind === "replace" ? { op: "replace_entrance", id: entrance, entrance: spec } : { op: "add_entrance", entrance: spec });
    }
    const outcome = await applySettingsEdits(project, edits);
    setSaving(false);
    if (!outcome.ok) {
      setProblem(outcome.problem);
      return;
    }
    client.setQueryData<ProjectConfig>(engineKey("project_config", { project }), outcome.config);
    // How toasts name it: what Home shows first.
    const name = [draft.label, override?.label, application?.name, draft.command, draft.trigger, override?.command, entrance]
      .map((text) => text?.trim() ?? "")
      .find((text) => text !== "");
    onSaved({ name: name ?? "The launch", launchId: process ? entrance : null });
  };

  const title = process ? (adding ? "Add launch" : "Edit launch") : adding ? "Add entrance" : "Edit entrance";
  const line = saveAs.kind === "replace" ? `:${saveAs.line}` : "";
  const footerNote = override
    ? "Saving writes an [[entrance_override]] table to unfold.toml and keeps your comments. Unfold never runs the command."
    : process
      ? adding
        ? "Saving writes one [[entrance]] table to unfold.toml and keeps your comments. Unfold never runs the command."
        : "Saving updates this launch in unfold.toml and keeps your comments. Unfold never runs the command."
      : adding
        ? "Saving writes one [[entrance]] table to unfold.toml and keeps your comments."
        : "Saving updates this entrance in unfold.toml and keeps your comments.";
  const formId = `${id}-form`;
  const applicationField = (
    <>
      <div className="launch-pair">
        <Field label="Application" htmlFor={`${id}-application`} error={shown("application") ? problems.application : undefined} className="launch-grow">
          <ApplicationPicker
            id={`${id}-application`}
            declared={declared}
            application={draft.application}
            newName={draft.newApplication}
            invalid={shown("application") && problems.application !== undefined}
            onChange={({ application, newName }) => {
              change({ application, newApplication: newName });
            }}
          />
        </Field>
        <Field label={process ? "Launch name" : "Name"} htmlFor={`${id}-label`} className="launch-name">
          <input
            id={`${id}-label`}
            className="launch-input"
            value={draft.label}
            placeholder={override?.label ?? undefined}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              change({ label: event.target.value });
            }}
          />
        </Field>
      </div>
      <p className="launch-hint launch-pair-hint">The application only groups launches on Home and in Add. It doesn't change how imports resolve.</p>
    </>
  );
  const description = (
    <Field label="Description" htmlFor={`${id}-description`} aside="optional">
      <textarea
        id={`${id}-description`}
        className="launch-textarea"
        rows={2}
        placeholder={process ? "What this launch is for. Shown on Home and in Add." : "What this entrance is for. Shown on Home and in Add."}
        value={draft.description}
        onChange={(event) => {
          change({ description: event.target.value });
        }}
      />
    </Field>
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title={title}
      description={<span className="launch-subtitle">{`${projectName} · unfold.toml${line} · shared with your team`}</span>}
      width={1000}
      height={720}
      placement="window"
      {...(override !== null ? {} : { initialFocus: process ? command : trigger })}
      headerAction={
        step ? (
          <span className="launch-step">{`${step.index + 1} of ${step.total}`}</span>
        ) : override ? null : (
          <SegmentedControl<Intent>
            label="What it does"
            variant="switch"
            options={INTENTS}
            value={draft.intent}
            onChange={(intent) => {
              change({ intent });
            }}
          />
        )
      }
      footer={
        <>
          <p className="launch-footer-note">{footerNote}</p>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          {onSkip ? (
            <Button variant="secondary" onClick={onSkip}>
              Skip
            </Button>
          ) : null}
          <Button variant="primary" type="submit" form={formId} busy={saving} disabled={saving || blocked !== null}>
            {adding ? (process ? "Save launch" : "Save entrance") : "Save"}
          </Button>
        </>
      }
    >
      <form id={formId} className="launch-form" onSubmit={(event) => void save(event)} noValidate>
        {!process ? (
          <HandlerFields draft={draft} problems={problems} shown={shown} touch={touch} change={change} triggerRef={trigger} />
        ) : override ? (
          <>
            <Field label="Command" hint={`Detected in ${override.declaredIn.path}. Only its name, application, schedule, and description change here.`}>
              <div className="launch-readonly launch-mono">{override.command ?? "—"}</div>
            </Field>
            <Field label="Runs in">
              <div className="launch-readonly launch-mono">{override.folder}</div>
            </Field>
          </>
        ) : (
          <>
            <Field
              label="Command"
              htmlFor={`${id}-command`}
              aside="as you deploy it"
              error={shown("command") ? problems.command : undefined}
              hint={[copiedFrom === null ? "Paste it from render.yaml, a Dockerfile CMD, or a Procfile." : `Copied from ${copiedFrom}.`, note].filter((text) => text !== null).join(" ")}
            >
              <textarea
                ref={command}
                id={`${id}-command`}
                className="launch-textarea launch-command launch-mono"
                rows={2}
                value={draft.command}
                aria-invalid={(shown("command") && problems.command !== undefined) || undefined}
                spellCheck={false}
                autoComplete="off"
                onBlur={touch("command")}
                onChange={(event) => {
                  change({ command: event.target.value });
                }}
              />
            </Field>
            <Field
              label="Runs in"
              labelId={`${id}-cwd`}
              hint={`The working folder, from the root of ${projectName}. For Docker, the folder that becomes WORKDIR.`}
            >
              <FolderPicker
                project={project}
                projectName={projectName}
                value={draft.cwd}
                labelledBy={`${id}-cwd`}
                onChange={(cwd) => {
                  change({ cwd });
                }}
              />
            </Field>
          </>
        )}
        {applicationField}
        {process ? (
          <>
            <div className="launch-switch-row">
              <Switch
                checked={draft.scheduled}
                label="Runs on a schedule"
                onChange={(scheduled) => {
                  change({ scheduled });
                }}
              />
              <span aria-hidden="true">Runs on a schedule</span>
            </div>
            {draft.scheduled ? (
              <Field
                label="Schedule"
                htmlFor={`${id}-schedule`}
                error={shown("schedule") ? problems.schedule : undefined}
                hint="As written, such as 30 7 * * *. Unfold shows it without a time zone."
              >
                <input
                  id={`${id}-schedule`}
                  className="launch-input launch-mono"
                  value={draft.schedule}
                  aria-invalid={(shown("schedule") && problems.schedule !== undefined) || undefined}
                  spellCheck={false}
                  autoComplete="off"
                  onBlur={touch("schedule")}
                  onChange={(event) => {
                    change({ schedule: event.target.value });
                  }}
                />
              </Field>
            ) : null}
          </>
        ) : null}
        {description}
        {process && override === null ? (
          <AnalysisDetails
            open={detailsOpen}
            onOpen={setDetailsOpen}
            target={draft.target}
            pythonPaths={draft.pythonPaths}
            env={draft.env}
            inferredPaths={inferredPaths}
            onTarget={(target) => {
              change({ target });
            }}
            onPythonPaths={(pythonPaths) => {
              change({ pythonPaths });
            }}
            onEnv={(env) => {
              change({ env });
            }}
            onSetByHand={() => {
              const starts = result?.starts ?? null;
              change({
                target: starts?.name ? emptyTarget("python_asgi") : { ...emptyTarget("file"), path: starts?.path ?? "" },
                pythonPaths: inferredPaths.length > 0 ? inferredPaths : null,
              });
              setDetailsOpen(true);
            }}
            onClear={() => {
              change({ target: null, pythonPaths: null });
              setFill((previous) => previous && { ...previous, filled: false });
            }}
            problems={submitted ? problems : {}}
          />
        ) : null}
        {override?.match === null ? (
          <SettingsProblemBox problem={{ code: "settings_conflict", message: "Unfold can't tell where this launch is declared, so it can't describe it.", line: null, column: null }} />
        ) : null}
        {shownProblem ? <SettingsProblemBox problem={shownProblem} /> : null}
      </form>
      {!process ? (
        <LocatorForms />
      ) : override ? (
        <DetectedPreview starts={override.starts} status={override.status} importPaths={override.importPaths} declaredIn={override.declaredIn.path} />
      ) : (
        <LaunchPreview
          check={result}
          application={effectiveApplication(draft, declared)?.name ?? null}
          pathsSetByHand={draft.pythonPaths !== null}
          fill={fill}
          onFill={() => {
            if (result?.suggestion) {
              fillFrom(result.suggestion);
            }
          }}
          onUndoFill={() => {
            if (fill) {
              change(fill.before);
              setFill({ ...fill, filled: false });
            }
          }}
        />
      )}
    </Dialog>
  );
}

function NotFound({ onClose }: { readonly onClose: () => void }) {
  return (
    <ConfirmDialog
      open
      title="Launch not found"
      message="It isn't in unfold.toml or the project's launches any more. The graph may not have caught up with an edit yet."
      confirmLabel="OK"
      placement="window"
      onConfirm={onClose}
      onCancel={onClose}
    />
  );
}

/** The review of a deploy-file import: one launch at a time, then a summary. */
function Review({
  request,
  config,
  summary,
  onClose,
}: {
  readonly request: Extract<EditorRequest, { readonly mode: "review" }>;
  readonly config: ProjectConfig;
  readonly summary: ProjectSummary | undefined;
  readonly onClose: () => void;
}) {
  const [progress, setProgress] = useState(REVIEW_START);
  const total = request.launches.length;
  const launch = request.launches[progress.index];
  const finish = (final: ReviewProgress) => {
    const title = reviewSummary(final);
    if (title !== null) {
      showToast({ title, body: waitsForCompile(summary) ? "Compile to see them in the graph." : "The graph updates in the background." });
    }
    onClose();
  };
  const step = (outcome: "saved" | "skipped") => {
    const next = advance(progress, outcome, total);
    if (next.done) {
      finish(next.progress);
    } else {
      setProgress(next.progress);
    }
  };
  if (!launch) {
    return null;
  }
  const taken = (config.settings?.entrances ?? []).map((table) => table.id);
  return (
    <LaunchForm
      key={progress.index}
      project={request.project}
      config={config}
      initial={draftOfImport(launch)}
      saveAs={{ kind: "add", id: uniqueId(launch.id, taken) }}
      copiedFrom={launch.copiedFrom}
      note={launch.note}
      step={{ index: progress.index, total }}
      onSaved={() => {
        step("saved");
      }}
      onSkip={() => {
        step("skipped");
      }}
      onClose={() => {
        finish(progress);
      }}
    />
  );
}

/** The launch editor for a `launchEditor` request. */
export function LaunchEditorDialog({ request, onClose }: { readonly request: EditorRequest; readonly onClose: () => void }) {
  const { project } = request;
  const config = useEngineQuery("project_config", { project });
  const summary = useEngineQuery("project_summary", { project });
  const catalog = useEngineQuery("entrances", { project }, { enabled: request.mode === "override" });
  if (!config.data || (request.mode === "override" && !catalog.data)) {
    return null;
  }
  const declared = config.data.settings?.applications ?? [];
  const taken = (config.data.settings?.entrances ?? []).map((table) => table.id);
  const saved = (result: { readonly name: string; readonly launchId: string | null }) => {
    reportSaved(project, summary.data, `${result.name} is saved in unfold.toml.`, result.launchId === null ? null : { id: result.launchId, name: result.name });
    onClose();
  };
  const form = { project, config: config.data, copiedFrom: null, note: null, step: null, onSaved: saved, onClose };
  switch (request.mode) {
    case "add": {
      const { prefill, handler } = request;
      const intent = request.intent ?? (handler ? "handler" : "process");
      const initial = prefill ? draftOfPrefill(prefill) : newDraft(intent, handler ? locatorFor(handler) : "");
      const id = prefill?.id === undefined ? null : uniqueId(prefill.id, taken);
      return <LaunchForm {...form} initial={initial} saveAs={{ kind: "add", id }} copiedFrom={prefill?.copiedFrom ?? null} />;
    }
    case "edit": {
      const table = config.data.settings?.entrances.find((candidate) => candidate.id === request.id);
      return table ? (
        <LaunchForm {...form} initial={draftOfTable(table, declared)} saveAs={{ kind: "replace", id: table.id, line: table.line }} />
      ) : (
        <NotFound onClose={onClose} />
      );
    }
    case "override": {
      const row = catalog.data ? findLaunch(catalog.data, (launch) => launch.key === request.key) : null;
      if (!row) {
        return <NotFound onClose={onClose} />;
      }
      const { match } = row;
      const existing = match ? (config.data.settings?.overrides.find((candidate) => sameMatch(candidate.match, match)) ?? null) : null;
      return <LaunchForm {...form} initial={draftOfOverride(existing, declared)} saveAs={{ kind: "override", row }} />;
    }
    case "review":
      return <Review request={request} config={config.data} summary={summary.data} onClose={onClose} />;
  }
}
