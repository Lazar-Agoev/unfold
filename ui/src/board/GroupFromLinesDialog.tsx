/**
 * Make a group from lines (`board.groupFromLines`): the line range and the title of a new group in the
 * selected card, for when dragging down the line numbers isn't handy. A group is a run of consecutive
 * statements in one body; the range is checked against the card's outline and its other groups.
 */

import { useQuery } from "@tanstack/react-query";
import { useState, type SyntheticEvent } from "react";
import { cardQuery } from "../canvas";
import { groupConflict, groupSelection } from "../code/outline";
import { Button, Dialog, TextField } from "../components";
import { addGroup, cardKey, editPage, findCard, findPage, getPageView, setPendingGroup, useOpenBoard } from "../state/boards";
import type { DialogRequest } from "../state/overlays";
import { resolveRefProject, useProjectIndex } from "../state/workspace";
import { parseLines } from "./groupFromLines";

export interface GroupFromLinesDialogProps {
  readonly request: Extract<DialogRequest, { kind: "groupFromLines" }>;
  readonly onClose: () => void;
}

const FORM_ID = "group-from-lines";

export function GroupFromLinesDialog({ request, onClose }: GroupFromLinesDialogProps) {
  const { target, cardId } = request;
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, cardId) : undefined;
  });
  const groups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const index = useProjectIndex();
  const project = card ? resolveRefProject(index, target.board.project, card.ref.project) : null;
  const key = card ? cardKey(card) : "";
  const query = useQuery({ ...cardQuery(project ?? "", key), enabled: project !== null && key.length > 0 });
  const code = query.data?.type === "block" && query.data.code?.source.state === "current" ? query.data.code.source : null;
  // A statement selected in the card proposes its own lines.
  const [lines, setLines] = useState(() => {
    const { selection } = getPageView(target);
    const selectedKey = selection.kind === "card" && selection.cardId === cardId ? selection.key : null;
    const item = code?.outline.find((entry) => entry.key === selectedKey && entry.key !== key);
    return item ? `${item.startLine}-${item.endLine}` : "";
  });
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    if (!code) {
      setError("This card's code isn't available right now.");
      return;
    }
    const range = parseLines(lines);
    if (!range) {
      setError("Type the lines as a range, such as 27-31.");
      return;
    }
    const selection = groupSelection(code.outline, range[0], range[1]);
    if ("error" in selection) {
      setError(selection.error);
      return;
    }
    const conflict = groupConflict(
      code.outline,
      selection.members,
      (groups ?? []).filter((group) => group.card === cardId),
    );
    if (conflict !== null) {
      setError(conflict);
      return;
    }
    if (title.trim().length === 0) {
      setError("Give the group a title.");
      return;
    }
    setPendingGroup(target, null);
    editPage(target, (page) => addGroup(page, cardId, selection.members, title.trim()));
    onClose();
  };

  return (
    <Dialog
      open
      width={400}
      placement="canvas"
      onClose={onClose}
      title="Make a group"
      description="A group is a run of consecutive statements in one body of this card. It shows on a magenta rail with its title."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" form={FORM_ID}>
            Make group
          </Button>
        </>
      }
    >
      <form id={FORM_ID} className="group-form" onSubmit={submit} noValidate>
        <TextField
          label="Lines"
          placeholder="27-31"
          value={lines}
          onChange={(event) => {
            setLines(event.target.value);
            setError(null);
          }}
        />
        <TextField
          label="Title"
          placeholder="What these lines do"
          value={title}
          error={error}
          onChange={(event) => {
            setTitle(event.target.value);
            setError(null);
          }}
        />
      </form>
    </Dialog>
  );
}
