/**
 * The first-Compile screen (architecture §11.6), shown for a folder that has never been compiled:
 * what would be graphed by language (`folder_summary`), what is skipped and why, Choose folders…
 * to narrow the scope first, and Compile, with the promise that nothing runs.
 */

import { useState } from "react";
import { Button, ProgressBar } from "../components";
import type { FolderSummary, SkippedFolder } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { ActionIds, actions, useShortcut } from "../keyboard";
import { cancelCompile, useCompileState } from "../state/compile";
import { fallbackReasonText, languageName, plural, progressText } from "../shell/status";
import { ChooseFoldersDialog } from "./ChooseFoldersDialog";
import "./FirstCompile.css";

/** "34 TypeScript files and 50 Python files." */
export function languageSentence(summary: FolderSummary): string {
  const parts = summary.languages.filter((entry) => entry.files > 0).map((entry) => plural(entry.files, `${languageName(entry.language)} file`));
  if (parts.length === 0) {
    return "No TypeScript or Python files to graph.";
  }
  const last = parts.pop() ?? "";
  return `${parts.length > 0 ? `${parts.join(", ")} and ${last}` : last}.`;
}

/** Lowercases a reason's first letter to run on inside a sentence, unless its first word is a name ("JavaScript"). */
function sentenceCase(text: string): string {
  const [word = ""] = text.split(" ");
  return /[A-Z]/.test(word.slice(1)) ? text : text.charAt(0).toLowerCase() + text.slice(1);
}

/** The skipped report, one clause per reason: "node_modules, dist (skipped by default)". */
export function skippedClauses(skipped: readonly SkippedFolder[], limit = 6): string[] {
  const byReason = new Map<string, string[]>();
  for (const entry of skipped) {
    const name = entry.folder === "" || entry.folder === "." ? "files at the top level" : entry.folder;
    byReason.set(entry.reasonText, [...(byReason.get(entry.reasonText) ?? []), name]);
  }
  return [...byReason].map(([reason, names]) => {
    const shown = names.slice(0, limit);
    const rest = names.length - shown.length;
    return `${shown.join(", ")}${rest > 0 ? ` and ${rest} more` : ""} (${sentenceCase(reason)})`;
  });
}

export function FirstCompile() {
  const summary = useEngineQuery("folder_summary", {});
  const running = useCompileState((state) => state.running);
  const progress = useCompileState((state) => state.progress);
  const error = useCompileState((state) => state.error);
  const [choosing, setChoosing] = useState(false);
  const shortcut = useShortcut(ActionIds.compile) ?? undefined;

  if (summary.error) {
    return (
      <main className="first-compile">
        <div className="first-compile-inner">
          <p className="first-compile-error">{summary.error.message}</p>
        </div>
      </main>
    );
  }
  if (!summary.data) {
    return <main className="first-compile" aria-busy="true" />;
  }
  const data = summary.data;
  const clauses = skippedClauses(data.skipped);
  const live = progressText(progress);

  return (
    <main className="first-compile">
      <div className="first-compile-inner">
        <h1 className="first-compile-name">{data.name}</h1>
        <p className="first-compile-counts">{languageSentence(data)}</p>
        {clauses.length > 0 ? <p className="first-compile-skipped">Skipped: {clauses.join("; ")}.</p> : null}
        {data.unindexed > 0 ? (
          <p className="first-compile-note">{plural(data.unindexed, "source-like file")} won't be graphed (see the reasons above).</p>
        ) : null}
        {data.settingsError ? (
          <p className="first-compile-warn">
            The settings file can't be used, so this uses the defaults: {data.settingsError}
          </p>
        ) : null}
        {data.problems.map((problem) => (
          <p key={problem} className="first-compile-warn">
            {problem}
          </p>
        ))}
        <div className="first-compile-choose">
          <Button
            size="small"
            disabled={running !== null}
            onClick={() => {
              setChoosing(true);
            }}
          >
            Choose folders…
          </Button>
        </div>

        <div className="first-compile-action">
          {running ? (
            <div className="first-compile-progress">
              <ProgressBar
                label="Compiling"
                value={progress && progress.total > 0 && (progress.stage === "snapshot" || progress.stage === "extract") ? progress.done / progress.total : null}
              />
              <span className="first-compile-progress-text">{live ? `Compiling · ${live}` : "Compiling…"}</span>
              <Button size="small" variant="quiet" onClick={() => void cancelCompile()}>
                Cancel
              </Button>
            </div>
          ) : (
            <Button
              variant="primary"
              size="large"
              disabled={data.files === 0}
              {...(shortcut ? { shortcut } : {})}
              onClick={() => {
                actions.run(ActionIds.compile);
              }}
            >
              Compile
            </Button>
          )}
          {error ? <p className="first-compile-error">Compile failed: {error.message}</p> : null}
        </div>

        <div className="first-compile-promise">
          <p>Reads files only. Runs nothing and installs nothing.</p>
          <p>
            {data.fallback
              ? `Keeps the graph and your drafts in ${data.localPath}. ${fallbackReasonText(data.fallback)}`
              : "Creates .unfold/ in this folder for the graph and your boards."}
          </p>
          <p>After this, the graph updates itself as files change.</p>
        </div>
      </div>
      {choosing ? (
        <ChooseFoldersDialog
          onClose={() => {
            setChoosing(false);
          }}
        />
      ) : null}
    </main>
  );
}
