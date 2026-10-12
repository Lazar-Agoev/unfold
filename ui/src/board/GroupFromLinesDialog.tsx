/**
 * Make a group from lines (`board.groupFromLines`): the line range and the title of a new group in the
 * selected card, for when dragging down the line numbers isn't handy. A group is a run of consecutive
 * statements in one body; the range is checked against the card's outline and its other groups.
 *
 * Combine… opens it with the statements of a Shift-selected run already chosen (`members`): titled
 * "Combine N steps", the run shown read-only as `file:from-to · N steps`, and a required title. It makes
 * the same group, collapsed into one block and selected, after checking the run again by the same rule.
 */

import { useQuery } from "@tanstack/react-query";
import { useRef, useState, type SyntheticEvent } from "react";
import { cardQuery } from "../canvas";
import { groupConflict, groupSelection } from "../code/outline";
import { shortPath } from "../code/text";
import { Button, Dialog, TextField } from "../components";
import type { CodeText, FlowModel, Group } from "../ipc/generated";
import { addGroup, cardKey, editPage, findCard, findPage, getPageView, select, setPendingGroup, useOpenBoard } from "../state/boards";
import { useAuthor } from "../state/author";
import type { DialogRequest } from "../state/overlays";
import { resolveRefProject, useProjectIndex } from "../state/workspace";
import { combinedSteps } from "./combine";
import { parseLines } from "./groupFromLines";

export interface GroupFromLinesDialogProps {
  readonly request: Extract<DialogRequest, { kind: "groupFromLines" }>;
  readonly onClose: () => void;
}

const FORM_ID = "group-from-lines";

/** Why Combine…'s chosen steps can't be grouped any more. */
const CHANGED = "The code changed since these steps were chosen. Choose them again.";

/** The lines and steps of a run chosen for Combine…, read from the card's outline and Flow. */
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

/**
 * The statements lines `from`–`to` group (they must be `chosen`, when given), or why they can't be a
 * group now: the gutter's rule, then the card's other groups.
 */
function checkedMembers(code: CodeText, from: number, to: number, groups: readonly Group[], chosen?: readonly string[]): { readonly members: string[] } | { readonly error: string } {
  const selection = groupSelection(code.outline, from, to);
  if ("error" in selection) {
    return selection;
  }
  if (chosen && selection.members.join("\u0000") !== chosen.join("\u0000")) {
    return { error: CHANGED };
  }
  const conflict = groupConflict(code.outline, selection.members, groups);
  return conflict !== null ? { error: conflict } : selection;
}

export function GroupFromLinesDialog({ request, onClose }: GroupFromLinesDialogProps) {
  const { target, cardId, members } = request;
  const author = useAuthor();
  const titleField = useRef<HTMLInputElement>(null);
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, cardId) : undefined;
  });
  const groups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const index = useProjectIndex();
  const project = card ? resolveRefProject(index, target.board.project, card.ref.project) : null;
  const key = card ? cardKey(card) : "";
  const query = useQuery({ ...cardQuery(project ?? "", key), enabled: project !== null && key.length > 0 });
  const model = query.data?.type === "block" ? query.data : undefined;
  const code = model?.code?.source.state === "current" ? model.code.source : null;
  const run = members ? chosenRun(code, model?.flow ?? null, members) : null;
  // A statement selected in the card proposes its own lines.
  const [lines, setLines] = useState(() => {
    const { selection } = getPageView(target);
    const selectedKey = selection.kind === "card" && selection.cardId === cardId ? selection.key : null;
    const item = code?.outline.find((entry) => entry.key === selectedKey && entry.key !== key);
    return item ? `${item.startLine}-${item.endLine}` : "";
  });
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const cardGroups = (groups ?? []).filter((group) => group.card === cardId);

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    if (!code) {
      setError("This card's code isn't available right now.");
      return;
    }
    const range: readonly [number, number] | null = members ? (run ? [run.from, run.to] : null) : parseLines(lines);
    if (!range) {
      setError(members ? CHANGED : "Type the lines as a range, such as 27-31.");
      return;
    }
    const selection = checkedMembers(code, range[0], range[1], cardGroups, members);
    if ("error" in selection) {
      setError(selection.error);
      return;
    }
    if (title.trim().length === 0) {
      setError(members ? "Give the combined block a title." : "Give the group a title.");
      return;
    }
    setPendingGroup(target, null);
    const options = { ...(author !== null ? { author } : {}), ...(members ? { collapsed: true } : {}) };
    const groupId = editPage(target, (page) => addGroup(page, cardId, selection.members, title.trim(), options));
    // A combined block shows collapsed and selected, ready to describe.
    if (members && groupId !== undefined) {
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
      title={members ? `Combine ${run ? run.steps : members.length} steps` : "Make a group"}
      description={
        members
          ? `Consecutive statements in one body of ${model?.header.qualifiedName ?? "this function"}. The block shows on a magenta rail with its title; the steps stay inside.`
          : "A group is a run of consecutive statements in one body of this card. It shows on a magenta rail with its title."
      }
      {...(members ? { initialFocus: titleField } : {})}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form={FORM_ID}>
            {members ? "Combine" : "Make group"}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} className="group-form" onSubmit={submit} noValidate>
        {members ? (
          <TextField label="Steps" value={stepsText} readOnly className="group-form-steps" />
        ) : (
          <TextField
            label="Lines"
            placeholder="27-31"
            value={lines}
            onChange={(event) => {
              setLines(event.target.value);
              setError(null);
            }}
          />
        )}
        <TextField
          ref={titleField}
          label="Title"
          placeholder={members ? "What these steps do" : "What these lines do"}
          value={title}
          error={error}
          required={members !== undefined}
          onChange={(event) => {
            setTitle(event.target.value);
            setError(null);
          }}
        />
      </form>
    </Dialog>
  );
}
