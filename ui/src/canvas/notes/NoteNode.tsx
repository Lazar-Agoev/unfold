/**
 * A note on the canvas (design §4.8): short plain text in the authored style, with who wrote it (the
 * agent mark for an agent). Free, it stays where it is put; attached, it moves with its card, frame,
 * resource card, or connection. A click selects it; dragging moves it (an attached note keeps its
 * attachment, at a new offset); double-clicking edits it in place (Enter keeps, Shift+Enter breaks the
 * line, Escape cancels), which makes it yours; its menu has Edit, Copy link, and Remove note.
 */

import type { Node, NodeProps } from "@xyflow/react";
import { memo, useState } from "react";
import { Author, copyLink } from "../../agent";
import { useContextMenu } from "../../components";
import { useAuthor } from "../../state/author";
import { NOTE_MAX, editNote, findNote, findPage, removeItem, select, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import "./notes.css";

interface NoteNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
}

export type NoteNodeType = Node<NoteNodeData, "note">;

/** The note's text, or the field that rewrites it in place. */
function NoteText({ text, editing, onDone }: { readonly text: string; readonly editing: boolean; readonly onDone: (text: string | null) => void }) {
  if (!editing) {
    return <p className="note-text">{text}</p>;
  }
  return (
    <textarea
      autoFocus
      className="note-input nodrag nopan"
      aria-label="Note"
      defaultValue={text}
      maxLength={NOTE_MAX}
      rows={Math.min(8, Math.max(2, text.split("\n").length + 1))}
      onClick={(event) => {
        event.stopPropagation();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          onDone(event.currentTarget.value);
        } else if (event.key === "Escape") {
          event.preventDefault();
          onDone(null);
        }
      }}
      onBlur={(event) => {
        onDone(event.currentTarget.value);
      }}
    />
  );
}

export const NoteNode = memo(function NoteNode({ id, data }: NodeProps<NoteNodeType>) {
  const { target } = data;
  const note = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findNote(page, id) : undefined;
  });
  const selected = usePageView(target, (view) => view.selection.kind === "note" && view.selection.noteId === id);
  const author = useAuthor();
  const [editing, setEditing] = useState(false);
  const menu = useContextMenu("Note actions", 196);
  if (!note) {
    return null;
  }
  const finish = (text: string | null) => {
    setEditing(false);
    if (text !== null && author !== null && text.trim().length > 0 && text.trim() !== note.text) {
      editNote(target, id, text, author);
    }
  };
  return (
    <aside
      className={`board-note note-drag${selected ? " selected" : ""}`}
      data-note-id={id}
      aria-label={`Note: ${note.text}`}
      onClick={() => {
        select(target, { kind: "note", noteId: id });
      }}
      onDoubleClick={() => {
        setEditing(author !== null);
      }}
      onContextMenu={(event) => {
        event.stopPropagation();
        select(target, { kind: "note", noteId: id });
        menu.open(event, [
          {
            id: "edit",
            label: "Edit",
            icon: "edit",
            disabled: author === null,
            onSelect: () => {
              setEditing(true);
            },
          },
          { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, id) },
          { kind: "separator", id: "s1" },
          {
            id: "remove",
            label: "Remove note",
            danger: true,
            onSelect: () => {
              removeItem(target, id);
            },
          },
        ]);
      }}
    >
      <NoteText text={note.text} editing={editing} onDone={finish} />
      <span className="note-byline">
        <Author author={note.author} />
      </span>
      {menu.element}
    </aside>
  );
});
