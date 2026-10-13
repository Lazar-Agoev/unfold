/**
 * Combine… (`board.combine`): the title of a combined block made from a Shift-selected run of steps.
 * The run is shown read-only as `file:from-to · N steps`, and the title is required. It makes a group
 * of the run's statements, collapsed into one block and selected, after checking the run again by the
 * rule that chose it.
 */

import { useQuery } from "@tanstack/react-query";
import { useRef, useState, type SyntheticEvent } from "react";
import { cardQuery } from "../canvas";
import { groupConflict, groupSelection } from "../code/outline";
import { shortPath } from "../code/text";
import { Button, Dialog, TextField } from "../components";
import type { CodeText, FlowModel, Group } from "../ipc/generated";
import { addGroup, cardKey, editPage, findCard, findPage, select, useOpenBoard } from "../state/boards";
import { useAuthor } from "../state/author";
import type { DialogRequest } from "../state/overlays";
import { combinedSteps } from "./combine";

export interface CombineDialogProps {
  readonly request: Extract<DialogRequest, { kind: "combine" }>;
  readonly onClose: () => void;
}

const FORM_ID = "combine-steps";

/** Why the chosen steps can't be combined any more. */
const CHANGED = "The code changed since these steps were chosen. Choose them again.";

/** The lines and steps of the chosen run, read from the card's outline and Flow. */
function chosenRun(code: CodeText | null, flow: FlowModel | null, members: readonly string[]): { readonly from: number; readonly to: number; readonly steps: number } | null {
  const items = code ? code.outline.filter((item) => members.includes(item.key)) : [];
  if (items.length !== members.length || items.length === 0) {
    return null;
  }
  return {
    from: Math.min(...items.map((item) => item.startLine)),
    to: Math.max(...items.map((item) => item.endLine)),
    steps: flow ? combinedSteps(flow, members).length : 0,
  };
}

/** Why the run's statements can't be combined now: they changed, or they meet another of the card's groups. */
function runProblem(code: CodeText, from: number, to: number, members: readonly string[], groups: readonly Group[]): string | null {
  const selection = groupSelection(code.outline, from, to);
  if ("error" in selection) {
    return selection.error;
  }
  if (selection.members.join("\u0000") !== members.join("\u0000")) {
    return CHANGED;
  }
  return groupConflict(code.outline, members, groups);
}

export function CombineDialog({ request, onClose }: CombineDialogProps) {
  const { target, cardId, members } = request;
  const author = useAuthor();
  const titleField = useRef<HTMLInputElement>(null);
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, cardId) : undefined;
  });
  const groups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const key = card ? cardKey(card) : "";
  const query = useQuery({ ...cardQuery(target.board.project, key), enabled: key.length > 0 });
  const model = query.data?.type === "block" ? query.data : undefined;
  const code = model?.code?.source.state === "current" ? model.code.source : null;
  const run = chosenRun(code, model?.flow ?? null, members);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    if (!code) {
      setError("This card's code isn't available right now.");
      return;
    }
    const problem = run ? runProblem(code, run.from, run.to, members, (groups ?? []).filter((group) => group.card === cardId)) : CHANGED;
    if (problem !== null) {
      setError(problem);
      return;
    }
    if (title.trim().length === 0) {
      setError("Give the combined block a title.");
      return;
    }
    const options = { ...(author !== null ? { author } : {}), collapsed: true };
    const groupId = editPage(target, (page) => addGroup(page, cardId, members, title.trim(), options));
    // A combined block shows collapsed and selected, ready to describe.
    if (groupId !== undefined) {
      select(target, { kind: "group", groupId });
    }
    onClose();
  };

  const stepsText = run && model ? `${shortPath(model.header.path)}:${run.from}-${run.to} · ${run.steps} ${run.steps === 1 ? "step" : "steps"}` : "";
  return (
    <Dialog
      open
      width={400}
      placement="canvas"
      onClose={onClose}
      title={`Combine ${run ? run.steps : members.length} steps`}
      description={`Consecutive statements in one body of ${model?.header.qualifiedName ?? "this function"}. The block shows on a magenta rail with its title; the steps stay inside.`}
      initialFocus={titleField}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form={FORM_ID}>
            Combine
          </Button>
        </>
      }
    >
      <form id={FORM_ID} className="combine-form" onSubmit={submit} noValidate>
        <TextField label="Steps" value={stepsText} readOnly className="combine-form-steps" />
        <TextField
          ref={titleField}
          label="Title"
          placeholder="What these steps do"
          value={title}
          error={error}
          required
          onChange={(event) => {
            setTitle(event.target.value);
            setError(null);
          }}
        />
      </form>
    </Dialog>
  );
}
