/**
 * Selected for a combined block (a step group): its title (renamed in place, which makes it yours) and
 * who wrote it, "COMBINED · in <function>", its description, the lines it spans (opening them, as View
 * code does), and View code and Ungroup. Ungroup restores the steps; neither action changes code.
 */

import { useQuery } from "@tanstack/react-query";
import { Author } from "../../agent";
import { cardQuery } from "../../canvas";
import { Button } from "../../components";
import type { Group } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { cardKey, editGroup, findCard, findPage, itemsFor, keyDisplayName, select, ungroup, useOpenBoard, type PageTarget } from "../../state/boards";
import { sourceLines, sourceOf, viewCode } from "../viewCode";
import { AuthoredField } from "./AuthoredField";
import { RelationRow } from "./Relations";

/** The lines a combined block spans, opening them (its card's model is read so they show once loaded). */
function GroupLines({ target, group }: { readonly target: PageTarget; readonly group: Group }) {
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, group.card) : undefined;
  });
  const query = useQuery({ ...cardQuery(target.board.project, card ? cardKey(card) : ""), enabled: card !== undefined });
  const selection = { kind: "group", groupId: group.id } as const;
  // Read through the resolver once the model is in (`query.data` keeps this current).
  const source = query.data ? sourceOf(target, selection).target : null;
  return (
    <div className="selected-relations">
      {source ? (
        <RelationRow
          label="Lines"
          title={source.path}
          onActivate={() => {
            viewCode(target, selection);
          }}
        >
          <span className="relation-code">{sourceLines(source)}</span>
        </RelationRow>
      ) : (
        <RelationRow label="Lines">
          <span className="relation-path">{query.isPending ? "Loading…" : "The code isn't available right now."}</span>
        </RelationRow>
      )}
    </div>
  );
}

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
              editGroup(target, group.id, { title: event.target.value, ...(author !== null ? { author } : {}) });
            }
          }}
        />
        {card ? (
          <p className="selected-location">
            COMBINED · in{" "}
            <button
              type="button"
              className="selected-link"
              onClick={() => {
                select(target, { kind: "card", cardId: card.id, key: null });
              }}
            >
              {keyDisplayName(cardKey(card))}
            </button>
            {group.author !== undefined ? (
              <>
                {" · titled by "}
                <Author author={group.author} />
              </>
            ) : null}
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
      {card ? <GroupLines target={target} group={group} /> : null}
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            viewCode(target, { kind: "group", groupId: group.id });
          }}
        >
          View code
        </Button>
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
