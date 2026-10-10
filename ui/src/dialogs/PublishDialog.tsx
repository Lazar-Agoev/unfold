/**
 * Publish, over the canvas, 520 wide. Three variants:
 *
 * - a draft's first Publish: "Publish <board>", the file name (the title's slug; Rust's suggestion when
 *   the name is taken), and where it writes;
 * - a published board's unpublished changes: "Publish changes to <board>" with their count (the engine
 *   returns a count, not a list);
 * - a conflict (the published file changed since the changes started, e.g. a pull): Keep mine writes
 *   them over it; Take theirs and drop my changes discards them (the button turns into a danger
 *   "Discard my changes").
 */

import { useEffect, useRef, useState } from "react";
import { Button, Dialog, RadioGroup, TextField, showToast } from "../components";
import { Icon } from "../icons";
import { toUnfoldError } from "../ipc";
import { plural, relativeTime } from "../lib/format";
import { discardChanges, isValidName, publishBoard, publishedPath, slugOf, type BoardRef } from "../state/boards";
import { useProjectRow } from "../state/workspace";
import { useBoardInfo, type BoardInfo } from "./boardInfo";

type Choice = "keep_mine" | "take_theirs";

const CHOICES = [
  { value: "keep_mine", label: "Keep mine" },
  { value: "take_theirs", label: "Take theirs and drop my changes" },
] as const satisfies readonly { value: Choice; label: string }[];

/** Why a file name can't be used, or null. */
function fileNameProblem(name: string): string | null {
  return isValidName(name.trim()) ? null : "Use letters, digits, “.”, “_”, and “-”, starting with a letter or digit.";
}

/** The conflict box's message: the published file changed or was deleted underneath the changes. */
function conflictText(deleted: boolean): string {
  return deleted
    ? "The published file was deleted since you started. Publishing writes it again."
    : "The published file changed since you started. Publishing replaces it.";
}

function Destination({ path, project }: { readonly path: string; readonly project: string }) {
  return (
    <span className="publish-destination">
      <span>Writes</span>
      <code>{path}</code>
      <span>in {project}. Commit it like code.</span>
    </span>
  );
}

interface PublishFormProps {
  readonly board: BoardRef;
  readonly info: BoardInfo;
  readonly onClose: () => void;
}

function PublishForm({ board, info, onClose }: PublishFormProps) {
  const project = useProjectRow(board.project)?.name ?? "the project";
  const draft = info.place === "draft";
  const field = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(() => slugOf(info.title));
  const [nameError, setNameError] = useState<string | null>(null);
  // Known before publishing for an open board whose published file changed (a pull), or learned from Publish.
  const [conflict, setConflict] = useState<{ readonly deleted: boolean } | null>(info.baseChanged ? { deleted: false } : null);
  const [choice, setChoice] = useState<Choice>("keep_mine");
  const [busy, setBusy] = useState(false);
  const nothing = !draft && info.changes === 0 && !info.dirty && conflict === null;
  const dropping = conflict !== null && choice === "take_theirs";

  const fail = (what: string, error: unknown) => {
    setBusy(false);
    showToast({ title: what, body: toUnfoldError(error).message, tone: "danger" });
  };

  const publish = async () => {
    if (draft) {
      const problem = fileNameProblem(name);
      setNameError(problem);
      if (problem !== null) {
        field.current?.focus();
        return;
      }
    }
    setBusy(true);
    try {
      if (dropping) {
        await discardChanges(board);
        onClose();
        showToast({ title: `Took the published ${info.title}`, body: "Your unpublished changes were dropped." });
        return;
      }
      const outcome = await publishBoard(board, draft ? { name: name.trim() } : conflict ? { resolve: "keep_mine" } : {});
      switch (outcome.status) {
        case "done":
          onClose();
          showToast({ title: `Published ${info.title}`, body: `Written to ${publishedPath(outcome.board.name)}. Commit it like code.` });
          return;
        case "name_taken":
          setBusy(false);
          setName(outcome.suggestion);
          setNameError(`${outcome.path} already exists, and Unfold never overwrites a board. Here is a free name.`);
          field.current?.focus();
          return;
        case "conflict":
          setBusy(false);
          setConflict({ deleted: outcome.theirsRevision === null });
          setChoice("keep_mine");
          return;
      }
    } catch (error) {
      fail(dropping ? "Couldn't discard the changes" : `Couldn't publish ${info.title}`, error);
    }
  };

  const path = publishedPath(draft ? name.trim() || slugOf(info.title) : board.id.replace(/^.*\//, "").replace(/\.json$/, ""));
  return (
    <Dialog
      open
      onClose={onClose}
      title={draft ? `Publish ${info.title}` : nothing ? `${info.title} is published` : `Publish changes to ${info.title}`}
      description={nothing ? "There are no unpublished changes to publish." : <Destination path={path} project={project} />}
      width={520}
      placement="canvas"
      className="publish-dialog"
      {...(draft ? { initialFocus: field } : {})}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button variant={dropping ? "danger" : "primary"} busy={busy} disabled={busy || nothing} onClick={() => void publish()}>
            {dropping ? "Discard my changes" : "Publish"}
          </Button>
        </>
      }
    >
      {draft ? (
        <form
          className="publish-section"
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            void publish();
          }}
        >
          <TextField
            ref={field}
            label="File name"
            value={name}
            error={nameError}
            hint="Chosen once: renaming the board later changes its title, not its file."
            onChange={(event) => {
              setName(event.target.value);
              setNameError(null);
            }}
          />
        </form>
      ) : null}
      {!draft && !nothing ? (
        <div className="publish-section">
          <h3 className="publish-caps">{plural(Math.max(info.changes, 1), "change")}</h3>
          <div className="publish-changes">
            <div className="publish-change">
              <Icon name="edit" className="publish-change-icon" />
              <span className="publish-change-text">Edited on this machine since the last Publish</span>
              {info.updatedAtMs !== null ? <span className="publish-change-meta">{relativeTime(info.updatedAtMs)}</span> : null}
            </div>
          </div>
        </div>
      ) : null}
      {conflict ? (
        <div className="publish-conflict">
          <div className="publish-conflict-message">
            <span className="publish-conflict-icon">
              <Icon name="warning-triangle" />
            </span>
            <span>{conflictText(conflict.deleted)}</span>
          </div>
          <RadioGroup<Choice> label="Which version to keep" options={CHOICES} value={choice} onChange={setChoice} className="publish-choices" />
        </div>
      ) : null}
    </Dialog>
  );
}

/** Publish for a board; closes itself if the board is gone. */
export function PublishDialog({ board, onClose }: { readonly board: BoardRef; readonly onClose: () => void }) {
  const info = useBoardInfo(board);
  const missing = info === null;
  useEffect(() => {
    if (missing) {
      onClose();
    }
  }, [missing, onClose]);
  return info ? <PublishForm board={board} info={info} onClose={onClose} /> : null;
}
