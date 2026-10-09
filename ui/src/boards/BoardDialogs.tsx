/**
 * The board dialogs (one at a time): Publish (showing the file it will write), a taken name,
 * Delete (asked first), a health item's repairs (each explained before it is applied), and Make a
 * group from lines.
 */

import { useQuery } from "@tanstack/react-query";
import { useState, type SyntheticEvent } from "react";
import { cardQuery } from "../canvas/cardData";
import { groupConflict, groupSelection } from "../code/outline";
import { Button, ConfirmDialog, Dialog, PromptDialog, SearchField, TextField } from "../components";
import type { HealthItem, RepairKind } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { addGroup, cardKey, findCard } from "../state/boards/document";
import { isValidName, publishedPath, slugOf } from "../state/boards/files";
import { itemReference, kindWord, repairEffect, repairLabel, repairSpec, targetName } from "../state/boards/health";
import { editBoard, getBoard, setPendingGroup, useBoards } from "../state/boards/store";
import { closeBoardDialog, useBoardDialogs, type BoardDialog } from "./dialogs";
import { runDelete, runPublish, runRenameFile, runRepair } from "./flows";
import "./boards.css";

function nameProblem(value: string): string | null {
  return isValidName(value.trim()) ? null : "Use letters, digits, “.”, “_”, and “-”, starting with a letter or digit.";
}

function PublishDialog({ boardId }: { readonly boardId: string }) {
  const title = useBoards((state) => state.boards[boardId]?.document.title ?? "");
  const [busy, setBusy] = useState(false);
  return (
    <Dialog
      open
      onClose={closeBoardDialog}
      title="Publish board"
      description={
        <>
          Writes <code className="board-path">{publishedPath(slugOf(title))}</code>. After this, edits save into that file, so <code>git diff</code> shows them.
          Unfold never runs Git.
        </>
      }
      footer={
        <>
          <Button onClick={closeBoardDialog}>Cancel</Button>
          <Button
            variant="primary"
            busy={busy}
            onClick={() => {
              setBusy(true);
              void runPublish(boardId);
            }}
          >
            Publish
          </Button>
        </>
      }
    />
  );
}

function NameDialog({ dialog }: { readonly dialog: Extract<BoardDialog, { kind: "name" }> }) {
  return (
    <PromptDialog
      open
      title={dialog.action === "publish" ? "That name is taken" : "Another board has that name"}
      description={
        <>
          <code className="board-path">{dialog.path}</code> already exists, and Unfold never overwrites a board. Choose another file name.
        </>
      }
      label="File name"
      defaultValue={dialog.suggestion}
      confirmLabel={dialog.action === "publish" ? "Publish" : "Rename"}
      validate={nameProblem}
      onCancel={closeBoardDialog}
      onSubmit={(value) => {
        const name = value.trim();
        if (dialog.action === "publish") {
          void runPublish(dialog.boardId, name);
        } else {
          void runRenameFile(dialog.boardId, name);
        }
      }}
    />
  );
}

function DeleteDialog({ boardId }: { readonly boardId: string }) {
  const board = useBoards((state) => state.boards[boardId]);
  const where = board?.place === "published" ? <code className="board-path">{board.id}</code> : "its draft on this machine";
  return (
    <ConfirmDialog
      open
      danger
      title={`Delete “${board?.document.title ?? "this board"}”?`}
      message={<>This deletes {where} and closes its tab. It can't be undone.</>}
      confirmLabel="Delete"
      onCancel={closeBoardDialog}
      onConfirm={() => void runDelete(boardId)}
    />
  );
}

function ReconnectPicker({ entrance, value, onChange }: { readonly entrance: boolean; readonly value: string; readonly onChange: (key: string) => void }) {
  const [query, setQuery] = useState("");
  const results = useEngineQuery("search", { query, kinds: [entrance ? "entrances" : "definitions"], limit: 12 }, { enabled: query.trim().length > 0 });
  const hits = (entrance ? results.data?.entrances : results.data?.definitions) ?? [];
  return (
    <div className="repair-reconnect">
      <SearchField value={query} onChange={setQuery} label={entrance ? "Find the entrance" : "Find the definition"} placeholder={entrance ? "Find the entrance…" : "Find the definition it became…"} />
      <div role="listbox" aria-label="Reconnect to" className="repair-hits">
        {hits.map((hit) => (
          <button
            key={hit.key}
            type="button"
            role="option"
            aria-selected={value === hit.key}
            className={`repair-hit${value === hit.key ? " chosen" : ""}`}
            onClick={() => {
              onChange(hit.key);
            }}
          >
            <span className="repair-hit-name">{hit.qualifiedName || hit.name}</span>
            <span className="repair-hit-path">
              {hit.path}
              {hit.line !== null ? `:${hit.line}` : ""}
            </span>
          </button>
        ))}
        {query.trim().length > 0 && results.isSuccess && hits.length === 0 ? <p className="repair-hint">Nothing matches.</p> : null}
      </div>
    </div>
  );
}

export function RepairDialog({ boardId, item, initial }: { readonly boardId: string; readonly item: HealthItem; readonly initial?: RepairKind | undefined }) {
  const document = useBoards((state) => state.boards[boardId]?.document);
  const [kind, setKind] = useState<RepairKind | null>(initial ?? item.repairs[0] ?? null);
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  if (!document) {
    return null;
  }
  const spec = kind !== null ? repairSpec(kind, item, document, target) : null;
  const reference = itemReference(item, document);
  const apply = async (event?: SyntheticEvent) => {
    event?.preventDefault();
    if (!spec) {
      return;
    }
    setBusy(true);
    const applied = await runRepair(boardId, spec);
    setBusy(false);
    if (applied) {
      closeBoardDialog();
    }
  };
  return (
    <Dialog
      open
      width="medium"
      onClose={closeBoardDialog}
      title={`${kindWord(item.kind)}: ${targetName(item.target, document)}`}
      description={item.message}
      footer={
        <>
          <Button onClick={closeBoardDialog}>Cancel</Button>
          <Button variant="primary" busy={busy} disabled={!spec} onClick={() => void apply()}>
            {kind !== null ? repairLabel(kind).replace(/…$/, "") : "Repair"}
          </Button>
        </>
      }
    >
      <fieldset className="repair-options">
        <legend className="visually-hidden">Repairs</legend>
        {item.repairs.map((option) => (
          <label key={option} className={`repair-option${kind === option ? " chosen" : ""}`}>
            <input
              type="radio"
              name="repair"
              value={option}
              checked={kind === option}
              onChange={() => {
                setKind(option);
              }}
            />
            <span>
              <strong>{repairLabel(option)}</strong>
              <small>{repairEffect(option, item, document)}</small>
            </span>
          </label>
        ))}
      </fieldset>
      {kind === "reconnect" ? <ReconnectPicker entrance={reference?.startsWith("entrance:") ?? false} value={target} onChange={setTarget} /> : null}
      <p className="repair-hint">The repair is saved right away.</p>
    </Dialog>
  );
}

/** "27-31" or "27" → [27, 31]; null when it isn't a line range. */
export function parseLines(text: string): [number, number] | null {
  const match = /^\s*(\d+)\s*(?:[-–—:]\s*(\d+))?\s*$/.exec(text);
  if (!match?.[1]) {
    return null;
  }
  const from = Number(match[1]);
  const to = match[2] !== undefined ? Number(match[2]) : from;
  return [Math.min(from, to), Math.max(from, to)];
}

function MakeGroupDialog({ boardId, cardId }: { readonly boardId: string; readonly cardId: string }) {
  const board = getBoard(boardId);
  const card = board ? findCard(board.document, cardId) : undefined;
  const key = card ? cardKey(card) : "";
  const query = useQuery({ ...cardQuery(key), enabled: key.length > 0 });
  const code = query.data?.type === "block" && query.data.code?.source.state === "current" ? query.data.code.source : null;
  const selectedKey = board?.view.selectedCardId === cardId ? board.view.selectedKey : null;
  const selected = code?.outline.find((item) => item.key === selectedKey && item.key !== key);
  const [lines, setLines] = useState(selected ? `${selected.startLine}-${selected.endLine}` : "");
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const formId = "make-group-form";
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
    const groups = (getBoard(boardId)?.document.groups ?? []).filter((group) => group.card === cardId);
    const conflict = groupConflict(code.outline, selection.members, groups);
    if (conflict !== null) {
      setError(conflict);
      return;
    }
    if (title.trim().length === 0) {
      setError("Give the group a title.");
      return;
    }
    setPendingGroup(boardId, null);
    editBoard(boardId, (document) => addGroup(document, cardId, selection.members, title.trim()));
    closeBoardDialog();
  };
  return (
    <Dialog
      open
      onClose={closeBoardDialog}
      title="Make a group"
      description="A group is a run of consecutive statements in one body of this card. It shows on a magenta rail with its title."
      footer={
        <>
          <Button onClick={closeBoardDialog}>Cancel</Button>
          <Button variant="primary" type="submit" form={formId}>
            Make Group
          </Button>
        </>
      }
    >
      <form id={formId} className="group-form" onSubmit={submit} noValidate>
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

/** Draws the open board dialog, if any. Mount once in the workbench. */
export function BoardDialogs() {
  const dialog = useBoardDialogs((state) => state.dialog);
  const exists = useBoards((state) => (dialog ? state.boards[dialog.boardId] !== undefined : false));
  if (!dialog || !exists) {
    return null;
  }
  switch (dialog.kind) {
    case "publish":
      return <PublishDialog boardId={dialog.boardId} />;
    case "name":
      return <NameDialog key={`${dialog.path}:${dialog.suggestion}`} dialog={dialog} />;
    case "delete":
      return <DeleteDialog boardId={dialog.boardId} />;
    case "repair":
      return <RepairDialog key={`${dialog.boardId}:${dialog.item.message}`} boardId={dialog.boardId} item={dialog.item} initial={dialog.repair} />;
    case "group":
      return <MakeGroupDialog key={dialog.cardId} boardId={dialog.boardId} cardId={dialog.cardId} />;
  }
}
