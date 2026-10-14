/**
 * Selected for a block drawn in a card: the page's name for it (else its words from the code), what it
 * is, the card it is drawn in, and its name, edited in place. A name applies page-wide, on every card
 * that draws the block.
 */

import { BLOCK_NAME_FIELD } from "../../canvas/cards/blockMenu";
import type { Card, Form } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { findPage, itemsFor, nameBlock, select, useOpenBoard, type PageTarget } from "../../state/boards";
import { cardName, findBlock, useCard } from "../../state/cards";
import { AuthoredField } from "./AuthoredField";


/** What a block of each form is, in a word. */
const FORM_WORDS: Readonly<Record<Form, string>> = {
  module: "Module",
  class: "Class",
  function: "Function",
  method: "Method",
  constructor: "Constructor",
  getter: "Getter",
  setter: "Setter",
  lambda: "Callback",
  interface: "Interface",
  type_alias: "Type",
  enum: "Enum",
  if: "Decision",
  match: "Decision",
  switch: "Decision",
  ternary: "Decision",
  for: "Loop",
  while: "Loop",
  do: "Loop",
  comprehension: "Loop",
  try: "Try",
  with: "With",
  using: "With",
  then: "Part",
  elif: "Part",
  else: "Part",
  case: "Part",
  default: "Part",
  body: "Part",
  except: "Part",
  catch: "Part",
  finally: "Part",
  call: "Call",
  new: "Call",
  return: "Exit",
  raise: "Exit",
  throw: "Exit",
  break: "Exit",
  continue: "Exit",
  yield: "Exit",
};

export function SelectedBlock({ target, card, blockKey }: { readonly target: PageTarget; readonly card: Card; readonly blockKey: string }) {
  const author = useAuthor();
  const query = useCard(target, card);
  const name = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.names.find((entry) => entry.key === blockKey));
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { name: blockKey });
  const held = issues.some((item) => item.kind === "review");
  const view = query.data;
  const block = view?.type === "definition" ? findBlock(view.flow, blockKey) : undefined;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name ui">{block ? (block.name ?? block.label) : "Loading…"}</h3>
        <p className="selected-location">
          {block ? `${FORM_WORDS[block.form]} · ` : null}in{" "}
          <button
            type="button"
            className="selected-link"
            onClick={() => {
              select(target, { kind: "card", cardId: card.id, key: null });
            }}
          >
            {view ? cardName(view) : card.id}
          </button>
        </p>
      </div>
      <AuthoredField
        label="Name"
        value={name}
        placeholder={block?.label ?? "A plain-language name…"}
        focusKey={BLOCK_NAME_FIELD}
        disabledReason={held ? "Held for review: accept or reconnect it on the card first." : author === null ? "Loading who you are…" : null}
        status={
          <>
            {block && name ? (
              <span className="authored-note">
                Replaces the name from the code: <code>{block.label}</code>
              </span>
            ) : null}
            {issues.map((item) => (
              <span key={item.message} className="authored-note">
                {item.message}
              </span>
            ))}
          </>
        }
        onChange={(text) => {
          if (author !== null) {
            nameBlock(target, blockKey, text, author);
          }
        }}
      />
    </>
  );
}
