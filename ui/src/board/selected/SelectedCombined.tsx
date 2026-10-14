/**
 * Selected for a combined step: its title (renamed in place, which makes it yours) and who wrote it,
 * how many blocks it holds, its description, and Collapse or Open, and Uncombine. Uncombining draws its
 * blocks one by one again; nothing changes code.
 */

import { Author } from "../../agent";
import { Button } from "../../components";
import type { Combined } from "../../ipc/generated";
import { plural } from "../../lib/format";
import { useAuthor } from "../../state/author";
import { editCombined, itemsFor, uncombine, useOpenBoard, type PageTarget } from "../../state/boards";
import { AuthoredField } from "./AuthoredField";

export function SelectedCombined({ target, combined }: { readonly target: PageTarget; readonly combined: Combined }) {
  const author = useAuthor();
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { combined: combined.id });
  return (
    <>
      <div className="selected-identity">
        <input
          className="selected-name ui selected-title-input"
          aria-label="Combined step title"
          value={combined.title}
          onChange={(event) => {
            if (event.target.value.trim().length > 0) {
              editCombined(target, combined.id, { title: event.target.value, ...(author !== null ? { author } : {}) });
            }
          }}
        />
        <p className="selected-location">
          {`COMBINED · ${plural(combined.members.length, "block")} · titled by `}
          <Author author={combined.author} />
        </p>
      </div>
      <AuthoredField
        label="Combined step description"
        value={combined.description}
        placeholder="Why these blocks belong together…"
        focusKey={`selected.combined:${combined.id}`}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={issues.map((item) => (
          <span key={item.message} className="authored-note">
            {item.message}
          </span>
        ))}
        onChange={(text) => {
          if (author !== null) {
            editCombined(target, combined.id, { description: { text, author } });
          }
        }}
      />
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            editCombined(target, combined.id, { collapsed: !combined.collapsed });
          }}
        >
          {combined.collapsed ? "Open" : "Collapse"}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            uncombine(target, combined.id);
          }}
        >
          Uncombine
        </Button>
      </div>
    </>
  );
}
