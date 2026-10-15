/**
 * Selected for a proposed step: "PROPOSED STEP", its title (required, one line, edited in place) and
 * optional Why, who wrote them (editing either makes them yours; typing in one field merges into one
 * undo step), and Where it would go: after which block, or at the start of which body. The board's save
 * is the only check of the words: while you type, what it refuses shows under them. A proposal has
 * no code of its own. What needs review about where it goes shows with Review (Reconnect to another
 * block, Accept, Remove). Copy link and Remove proposal close it; removing never edits code.
 */

import { useId, useState } from "react";
import { Author, copyLink } from "../../agent";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { Button } from "../../components";
import type { Proposal } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { editPage, itemsFor, removeProposal, updateProposal, useOpenBoard, type PageTarget } from "../../state/boards";
import { usePageBlock } from "../../state/cards";
import { RelationRow } from "./Relations";

/** Rewords a proposed step as `author`; typing in one field merges into one undo step. */
function reword(target: PageTarget, proposalId: string, change: { readonly title?: string; readonly reason?: string | null }, author: string, field: string): void {
  editPage(
    target,
    (page) => {
      updateProposal(page, proposalId, change, author);
    },
    { coalesce: `proposal-${field}:${proposalId}` },
  );
}

/** Where it would go: after which block, or at the start of which body, as the page's cards draw it. */
function Where({ target, proposal }: { readonly target: PageTarget; readonly proposal: Proposal }) {
  const after = "after" in proposal;
  const key = after ? proposal.after : proposal.start;
  const drawn = usePageBlock(target, key);
  const label = drawn ? (drawn.block.name ?? drawn.block.label) : after ? "a block" : "a body";
  return (
    <div className="selected-relations">
      <RelationRow label="Where" title={key} wrap>
        <span className="relation-path">{after ? `after ${label}` : `at the start of ${label}`}</span>
      </RelationRow>
    </div>
  );
}

export function SelectedProposal({ target, proposal }: { readonly target: PageTarget; readonly proposal: Proposal }) {
  const author = useAuthor();
  const health = useOpenBoard(target.board, (board) => board.health);
  const saveError = useOpenBoard(target.board, (board) => board.saveError) ?? null;
  const issues = itemsFor(health, target.page, { proposal: proposal.id });
  const { project } = target.board;
  // What's typed shows while typing (a blank title isn't saved); leaving a field shows what is saved.
  const [title, setTitle] = useState<string | null>(null);
  const [reason, setReason] = useState<string | null>(null);
  const whyId = useId();
  const shownTitle = title ?? proposal.title;
  const blank = shownTitle.trim().length === 0;
  // Every keystroke is an edit, so a save that fails while you type refuses what you typed.
  const refused = title !== null || reason !== null ? saveError : null;
  return (
    <>
      <div className="selected-identity">
        <p className="selected-kicker proposed">
          <span className="proposal-glyph" aria-hidden="true" />
          PROPOSED STEP
        </p>
        <input
          className="selected-name ui selected-title-input"
          aria-label="Proposed step title"
          aria-invalid={blank}
          value={shownTitle}
          placeholder="What it would do…"
          disabled={author === null}
          onChange={(event) => {
            const text = event.target.value;
            setTitle(text);
            if (author !== null && text.trim().length > 0) {
              reword(target, proposal.id, { title: text }, author, "title");
            }
          }}
          onBlur={() => {
            setTitle(null);
          }}
        />
        {blank ? <p className="selected-byline proposal-error">A proposed step needs a title.</p> : null}
        <p className="selected-byline">
          by <Author author={proposal.author} />
        </p>
      </div>
      <div className="proposal-why">
        <label className="proposal-why-label" htmlFor={whyId}>
          Why
        </label>
        <textarea
          id={whyId}
          className="proposal-why-input"
          value={reason ?? proposal.reason ?? ""}
          placeholder="Why it's wanted (optional)…"
          rows={1}
          spellCheck
          disabled={author === null}
          onChange={(event) => {
            const text = event.target.value;
            setReason(text);
            if (author !== null) {
              reword(target, proposal.id, { reason: text.trim().length > 0 ? text : null }, author, "reason");
            }
          }}
          onBlur={() => {
            setReason(null);
          }}
        />
        {refused !== null ? <p className="selected-byline proposal-error">{refused}</p> : null}
      </div>
      <Where target={target} proposal={proposal} />
      {issues.length > 0 ? (
        <p className="selected-note proposal-review">
          {issues.map((item) => item.message).join(" ")} <ReviewButton target={target} project={project} items={issues} />
        </p>
      ) : null}
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            void copyLink(target, proposal.id);
          }}
        >
          Copy link
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            editPage(target, (page) => {
              removeProposal(page, proposal.id);
            });
          }}
        >
          Remove proposal
        </Button>
      </div>
    </>
  );
}
