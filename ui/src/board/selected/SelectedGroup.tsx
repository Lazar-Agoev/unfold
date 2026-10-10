/** Selected for a group (V-60): its title (renamed in place), its description, the card it is in, and Ungroup. */

import { Button } from "../../components";
import type { Group } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { cardKey, editGroup, findCard, findPage, itemsFor, keyDisplayName, select, ungroup, useOpenBoard, type PageTarget } from "../../state/boards";
import { AuthoredField } from "./AuthoredField";

export function SelectedGroup({ target, group }: { readonly target: PageTarget; readonly group: Group }) {
  const author = useAuthor();
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, group.card) : undefined;
  });
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { group: group.id });
  return (
    <>
      <div className="selected-identity">
        <input
          className="selected-name ui selected-title-input"
          aria-label="Group title"
          value={group.title}
          onChange={(event) => {
            if (event.target.value.trim().length > 0) {
              editGroup(target, group.id, { title: event.target.value });
            }
          }}
        />
        {card ? (
          <p className="selected-location">
            Group in{" "}
            <button
              type="button"
              className="selected-link"
              onClick={() => {
                select(target, { kind: "card", cardId: card.id, key: null });
              }}
            >
              {keyDisplayName(cardKey(card))}
            </button>
          </p>
        ) : null}
      </div>
      <AuthoredField
        label="Group description"
        value={group.description}
        placeholder="Why these lines belong together…"
        focusKey={`selected.group:${group.id}`}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={issues.map((item) => (
          <span key={item.message} className="authored-note">
            {item.message}
          </span>
        ))}
        onChange={(text) => {
          if (author !== null) {
            editGroup(target, group.id, { description: { text, author } });
          }
        }}
      />
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            ungroup(target, group.id);
          }}
        >
          Ungroup
        </Button>
      </div>
    </>
  );
}
