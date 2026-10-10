/**
 * The not-compiled Home: what Unfold will read, what it skips and why, settings or discovery
 * problems as Health rows, the top-level scope with check boxes staged until Compile, the
 * promise that nothing runs, and Compile (⇧⌘B). Once the state leaves not-compiled, Home shows the
 * compiled overview filling in as the compile finds entrances.
 */

import { useState } from "react";
import { Button, Checkbox, showToast } from "../../components";
import { Icon } from "../../icons";
import { toUnfoldError, useEngineQuery, type CompilePreview, type ProjectId, type ProjectRow, type ProjectSummary, type ScopeFolder } from "../../ipc";
import { ActionIds, useRegisterActions, useShortcut } from "../../keyboard";
import { formatCount, plural } from "../../lib/format";
import { HealthRowView, runHealthAction } from "../health/HealthRow";
import { settingsSection } from "../health/problems";
import { ProjectHeader } from "../main/ProjectHeader";
import { fallbackReasonText } from "../stateLine";
import { compileCaption, effectiveChoice, languageSentence, scopeCounts, scopeRows, skippedRows, skipWords } from "./model";
import { compileWithScope, stageFolder, useStagedScope } from "./staging";
import "./notCompiled.css";

function Skipped({ preview }: { readonly preview: CompilePreview }) {
  const rows = skippedRows(preview.skipped);
  if (rows.length === 0) {
    return null;
  }
  return (
    <section className="home-skipped" aria-label="Skipped">
      <h2 className="home-caps home-skipped-label">Skipped</h2>
      {rows.map((row) => (
        <div key={row.reason} className="home-skipped-row">
          <span className="home-skipped-reason">{row.reason}</span>
          <span className="home-skipped-examples">{row.examples}</span>
          <span className="home-skipped-count">{row.count}</span>
        </div>
      ))}
    </section>
  );
}

/** Settings and discovery problems the preview found, as Health rows. */
function PreviewProblems({ project, preview }: { readonly project: ProjectId; readonly preview: CompilePreview }) {
  const rows = [
    ...(preview.settingsError === null
      ? []
      : [
          {
            id: "settings",
            title: `Settings can't be used — ${preview.settingsError}`,
            explanation: "The Compile uses the defaults until the settings file is fixed.",
            section: settingsSection(preview.settingsError),
          },
        ]),
    ...preview.problems.map((problem, index) => ({
      id: `discovery-${index}`,
      title: problem,
      explanation: "Files there won't be on the graph.",
      section: "scope" as const,
    })),
  ];
  if (rows.length === 0) {
    return null;
  }
  return (
    <div className="home-health-list" aria-label="Problems">
      {rows.map((row) => (
        <HealthRowView
          key={row.id}
          row={{ title: row.title, explanation: row.explanation, location: row.id === "settings" ? ".unfold/config.toml" : "" }}
          action={{ label: "Settings", run: () => { runHealthAction(project, { kind: "settings", section: row.section }); } }}
        />
      ))}
    </div>
  );
}

function ScopeRow({ project, folder, choice }: { readonly project: ProjectId; readonly folder: ScopeFolder; readonly choice: ReturnType<typeof effectiveChoice> }) {
  const disabled = choice === "disabled";
  return (
    <label className={disabled ? "home-scope-row home-scope-row-disabled" : "home-scope-row"}>
      <Checkbox
        checked={choice === "checked"}
        disabled={disabled}
        label={folder.path}
        onChange={(checked) => {
          stageFolder(project, folder, checked);
        }}
      />
      <Icon name="folder" className="home-scope-icon" />
      <span className="home-scope-path">{folder.path}</span>
      <span className="home-scope-count">{disabled && folder.skipped !== null ? skipWords(folder.skipped) : plural(folder.sources, "file")}</span>
    </label>
  );
}

function ScopeCard({ project, preview, folders, truncated }: {
  readonly project: ProjectId;
  readonly preview: CompilePreview;
  readonly folders: readonly ScopeFolder[];
  readonly truncated: boolean;
}) {
  const staged = useStagedScope(project);
  const { chosen, total } = scopeCounts(preview.files, folders, staged);
  const rows = scopeRows(folders);
  return (
    <section className="home-scope" aria-label="What to graph">
      <header className="home-scope-header">
        <h2 className="home-section-title">What to graph</h2>
        <p className="home-scope-hint">Top-level folders. Uncheck what you don't want on the graph.</p>
        <p className="home-scope-total">
          {formatCount(chosen)} of {plural(total, "file")}
        </p>
      </header>
      <div className="home-scope-list">
        {rows.length === 0 ? <p className="home-scope-note">Every file is at the top level of the folder.</p> : null}
        {rows.map((folder) => (
          <ScopeRow key={folder.path} project={project} folder={folder} choice={effectiveChoice(folder, staged)} />
        ))}
        {truncated ? <p className="home-scope-note">Only the first {formatCount(folders.length)} folders are listed; change the rest in Settings.</p> : null}
      </div>
    </section>
  );
}

function PromiseNote({ preview }: { readonly preview: CompilePreview }) {
  const where = preview.fallback
    ? `Keeps the graph and your drafts in ${preview.localPath}. ${fallbackReasonText(preview.fallback)}`
    : "Creates .unfold/ in this folder for the graph and your boards.";
  return (
    <div className="home-promise">
      <span className="home-promise-icon">
        <Icon name="shield" />
      </span>
      <p>Reads files only. Runs nothing and installs nothing. {where} After this, the graph updates itself as files change.</p>
    </div>
  );
}

function Setup({ project, preview, folders, truncated }: {
  readonly project: ProjectId;
  readonly preview: CompilePreview;
  readonly folders: readonly ScopeFolder[];
  readonly truncated: boolean;
}) {
  const [starting, setStarting] = useState(false);
  const shortcut = useShortcut(ActionIds.compile) ?? undefined;
  const staged = useStagedScope(project);
  const nothing = scopeCounts(preview.files, folders, staged).chosen === 0;
  const compile = async () => {
    if (nothing || starting) {
      return;
    }
    setStarting(true);
    try {
      await compileWithScope(project, folders);
    } catch (error) {
      showToast({ title: "Couldn't compile", body: toUnfoldError(error).message, tone: "danger" });
    } finally {
      setStarting(false);
    }
  };
  // ⇧⌘B here writes the staged scope first, as the button does (a mounted view's action wins).
  useRegisterActions([{ id: ActionIds.compile, title: "Compile", category: "Project", enabled: () => !nothing, run: compile }]);

  return (
    <div className="home-first-compile">
      <p className="home-compile-summary">{languageSentence(preview)}</p>
      <Skipped preview={preview} />
      <PreviewProblems project={project} preview={preview} />
      <ScopeCard project={project} preview={preview} folders={folders} truncated={truncated} />
      <PromiseNote preview={preview} />
      <div className="home-compile-row">
        <Button className="home-compile-button" variant="primary" busy={starting} disabled={nothing} {...(shortcut === undefined ? {} : { shortcut })} onClick={() => void compile()}>
          Compile
        </Button>
        <p className="home-compile-caption">{nothing ? "Check a folder to compile." : compileCaption(preview.files)}</p>
      </div>
    </div>
  );
}

export function NotCompiled({ row, summary }: { readonly row: ProjectRow; readonly summary: ProjectSummary | undefined }) {
  const preview = useEngineQuery("compile_preview", { project: row.id });
  const scope = useEngineQuery("scope_dir", { project: row.id, path: "" });
  const error = preview.error ?? scope.error;
  return (
    <div className="home-page home-page-setup">
      <ProjectHeader row={row} summary={summary} compact />
      {error ? <p className="home-compile-error">{error.message}</p> : null}
      {!error && !(preview.data && scope.data) ? <p className="home-compile-caption">Reading the folder…</p> : null}
      {preview.data && scope.data ? <Setup project={row.id} preview={preview.data} folders={scope.data.folders} truncated={scope.data.truncated} /> : null}
    </div>
  );
}
