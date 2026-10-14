/**
 * Selected for a note: its text (edited in place, which makes it yours) and who wrote it,
 * what it is attached to (selecting it) or that it is free, and Remove note. Notes aren't signed and
 * have no health. The file holds a note's text trimmed, so while you type the field shows what you
 * typed (a space before the next word, a line break) and saves it trimmed; leaving it shows the saved
 * text.
 */

import { useState } from "react";
import { Button } from "../../components";
import type { Note } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { NOTE_MAX, editNote, findPage, noteLength, removeItem, useOpenBoard, type PageTarget } from "../../state/boards";
import { AuthoredField } from "./AuthoredField";
import { useCardNames } from "../../state/cards";
import { itemName, selectItem } from "./ClaimRows";

/** The focus target of a note's text field in Selected. */
const NOTE_FIELD = "selected.note";

export function SelectedNote({ target, note }: { readonly target: PageTarget; readonly note: Note }) {
  const author = useAuthor();
  const names = useCardNames(target);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const [draft, setDraft] = useState<string | null>(null);
  if (!page) {
    return null;
  }
  const { attach } = note;
  const text = draft ?? note.text;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name ui">Note</h3>
        <p className="selected-location">
          {attach !== undefined ? (
            <>
              {"On "}
              <button
                type="button"
                className="selected-link"
                onClick={() => {
                  selectItem(target, page, attach);
                }}
              >
                {itemName(page, attach, names)}
              </button>
              {", moving with it"}
            </>
          ) : (
            "Free on the page"
          )}
        </p>
      </div>
      <AuthoredField
        label="Note"
        value={{ text, author: note.author, signature: "" }}
        placeholder="What a reader would otherwise miss…"
        focusKey={NOTE_FIELD}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={<span className="authored-note">{`${noteLength(text)} of ${NOTE_MAX} characters`}</span>}
        onChange={(typed) => {
          if (author !== null) {
            setDraft(typed);
            editNote(target, note.id, typed, author);
          }
        }}
        onBlur={() => {
          setDraft(null);
        }}
      />
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            removeItem(target, note.id);
          }}
        >
          Remove note
        </Button>
      </div>
    </>
  );
}
