/**
 * Selected for a drawn link: "Link · drawn by <author>", its kind (a menu that makes it a connection),
 * its label (edited in place), the items it joins (cards with their projects' monograms), what a drawn
 * link is, and Edit label and Remove link.
 */

import { useId, useRef } from "react";
import { LINK_LABEL_FIELD } from "../../canvas/cards/select";
import { Author } from "../../agent";
import { kindEntries } from "../../canvas/connections/linkMenu";
import { Button, MenuButton, Monogram } from "../../components";
import type { Link, Page } from "../../ipc/generated";
import { requestFocus, useFocusTarget } from "../../keyboard";
import { useAuthor } from "../../state/author";
import { cardKey, findCard, findPage, focusCard, keyDisplayName, removeLink, setLinkLabel, useOpenBoard, type PageTarget } from "../../state/boards";
import { useProjectRow } from "../../state/workspace";
import { itemName, selectItem } from "./ClaimRows";
import { RelationRow } from "./Relations";

function End({ target, label, page, id, boardProject }: { readonly target: PageTarget; readonly label: string; readonly page: Page | undefined; readonly id: string; readonly boardProject: string }) {
  const card = page ? findCard(page, id) : undefined;
  if (page && !card) {
    return (
      <RelationRow
        label={label}
        onActivate={() => {
          selectItem(target, page, id);
        }}
      >
        <span className="relation-code">{itemName(page, id)}</span>
      </RelationRow>
    );
  }
  if (!card) {
    return <RelationRow label={label}>a removed card</RelationRow>;
  }
  return (
    <RelationRow
      label={label}
      onActivate={() => {
        focusCard(target, card.id);
      }}
    >
      <span className="relation-card">
        <Monogram name={card.ref.project ?? boardProject} size={14} />
        <span className="relation-code">{keyDisplayName(cardKey(card))}</span>
      </span>
    </RelationRow>
  );
}

export function SelectedLink({ target, link }: { readonly target: PageTarget; readonly link: Link }) {
  const author = useAuthor();
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const boardProject = useProjectRow(target.board.project)?.name ?? "";
  const field = useRef<HTMLInputElement>(null);
  const labelId = useId();
  useFocusTarget(LINK_LABEL_FIELD, field);

  return (
    <>
      <div className="selected-link-identity">
        <h3 className="selected-name ui">Link</h3>
        {link.label ? (
          <span className="selected-byline">
            · drawn by <Author author={link.label.author} />
          </span>
        ) : null}
      </div>
      <div className="selected-relations link">
        <div className="relation-row">
          <span className="relation-label">Kind</span>
          <MenuButton variant="text" size="sm" label="Drawn link" menuLabel="Link kind" entries={kindEntries(target, link.id, author)} />
        </div>
        <div className="relation-row">
          <label className="relation-label" htmlFor={labelId}>
            Label
          </label>
          <input
            ref={field}
            id={labelId}
            className="relation-input"
            value={link.label?.text ?? ""}
            placeholder="What happens along it…"
            disabled={author === null}
            onChange={(event) => {
              if (author !== null) {
                setLinkLabel(target, link.id, event.target.value, author);
              }
            }}
          />
        </div>
        <End target={target} label="From" page={page} id={link.from} boardProject={boardProject} />
        <End target={target} label="To" page={page} id={link.to} boardProject={boardProject} />
      </div>
      <p className="selected-note">Drawn by hand. Unfold doesn’t trace calls between projects; health checks that both ends still exist.</p>
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            requestFocus(LINK_LABEL_FIELD);
          }}
        >
          Edit label
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            removeLink(target, link.id);
          }}
        >
          Remove link
        </Button>
      </div>
    </>
  );
}
