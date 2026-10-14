/**
 * Selected for a block drawn in a card: the page's name for it (else its words from the code: a part's,
 * the text that opens it), what it is, and where: a part's lines ("Part · lines 20–22"), else the card it is drawn in; then its name, edited in place.
 * A name applies page-wide, on every card that draws the block. A name that needs review says why,
 * with the card banner's Review menu.
 */

import { BLOCK_NAME_FIELD } from "../../canvas/cards/blockMenu";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
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

/** "lines 20–22", or "line 13" for one. */
function linesText([first, last]: readonly [number, number]): string {
  return last > first ? `lines ${first}–${last}` : `line ${first}`;
}

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
        {block?.lines ? (
          <p className="selected-location">{`${FORM_WORDS[block.form]} · ${linesText(block.lines)}`}</p>
        ) : (
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
        )}
      </div>
      <AuthoredField
        label="Name"
        value={name}
        placeholder={block?.label ?? "A plain-language name…"}
        focusKey={BLOCK_NAME_FIELD}
        disabledReason={held ? "Held for review: accept or reconnect it first." : author === null ? "Loading who you are…" : null}
        status={
          <>
            {block && name ? (
              <span className="authored-note">
                Replaces the name from the code: <code>{block.label}</code>
              </span>
            ) : null}
            {issues.length > 0 ? (
              <span className="authored-note">
                {issues.map((item) => item.message).join(" ")} <ReviewButton target={target} project={target.board.project} items={issues} />
              </span>
            ) : null}
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
