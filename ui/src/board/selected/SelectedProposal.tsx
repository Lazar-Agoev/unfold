/**
 * Selected for a proposed step (docs/proposals/unified-code-diagrams.md §7): "PROPOSED STEP", its title
 * (required, one line, edited in place) and optional Why, who wrote them (editing either makes them
 * yours; typing in one field merges into one undo step), and Where it would go: after which real step
 * (and on which path), with that step's `file:line` as View placement code, since a proposal has no code
 * of its own. What needs review about where it goes shows with Review (Reconnect to another step,
 * Accept, Remove). Copy link and Remove proposal close it; removing never edits code.
 */

import { useQuery } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { Author, copyLink } from "../../agent";
import { cardQuery } from "../../canvas";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { Button } from "../../components";
import type { Card, Proposal } from "../../ipc/generated";
import { useFocusTarget } from "../../keyboard";
import { useAuthor } from "../../state/author";
import {
  PROPOSAL_REASON_MAX,
  PROPOSAL_TITLE_MAX,
  cardKey,
  editPage,
  findCard,
  findPage,
  itemsFor,
  keyDisplayName,
  labelsFor,
  proposalTitle,
  removeProposal,
  updateProposal,
  useOpenBoard,
  useResolvedCardProject,
  type PageTarget,
} from "../../state/boards";
import { resolveRefProject, useProjectIndex } from "../../state/workspace";
import { PATH_WORDS, PROPOSAL_TITLE_FIELD } from "../proposeStep";
import { sourceLines, sourceOf, viewCode } from "../viewCode";
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

/** Where it would go: after which step (and path), and that step's lines as View placement code. */
function Where({ target, proposal, card }: { readonly target: PageTarget; readonly proposal: Proposal; readonly card: Card }) {
  const project = useResolvedCardProject(target.board, card);
  const query = useQuery({ ...cardQuery(project ?? "", cardKey(card)), enabled: project !== null });
  const labels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const model = query.data?.type === "block" ? query.data : undefined;
  const node = model?.flow?.nodes.find((item) => item.key === proposal.after);
  const label = labelsFor(labels ?? [], card.ref.project)[proposal.after] ?? node?.label;
  const path = proposal.path !== undefined ? ` · ${PATH_WORDS[proposal.path]}` : "";
  const selection = { kind: "proposal", proposalId: proposal.id } as const;
  // Read through the resolver once the card's model is in (`query.data` keeps this current).
  const placement = query.data ? sourceOf(target, selection) : null;
  return (
    <div className="selected-relations">
      <RelationRow label="Where" title={proposal.after} wrap>
        <span className="relation-path">{label !== undefined ? `after ${label}${path}` : `after ${keyDisplayName(proposal.after)} (missing)${path}`}</span>
      </RelationRow>
      {placement?.target ? (
        <>
          <RelationRow
            title="View placement code: the step it goes after"
            onActivate={() => {
              viewCode(target, selection);
            }}
          >
            <span className="relation-code" aria-label={`View placement code, ${sourceLines(placement.target)}`}>
              {sourceLines(placement.target)}
            </span>
          </RelationRow>
          <p className="selected-note">Proposed, not built: this opens the code of the step it would follow.</p>
        </>
      ) : (
        <RelationRow wrap>
          <span className="relation-path">{query.isPending ? "Loading…" : (placement?.reason ?? "The step it goes after isn't available.")}</span>
        </RelationRow>
      )}
    </div>
  );
}

export function SelectedProposal({ target, proposal }: { readonly target: PageTarget; readonly proposal: Proposal }) {
  const author = useAuthor();
  const card = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findCard(page, proposal.card) : undefined;
  });
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { proposal: proposal.id });
  const index = useProjectIndex();
  const project = resolveRefProject(index, target.board.project, card?.ref.project) ?? target.board.project;
  // What's typed shows while typing (a blank title isn't saved); leaving a field shows what is saved.
  const [title, setTitle] = useState<string | null>(null);
  const [reason, setReason] = useState<string | null>(null);
  const titleField = useRef<HTMLInputElement>(null);
  const whyId = useId();
  useFocusTarget(PROPOSAL_TITLE_FIELD, titleField);
  const shownTitle = title ?? proposal.title;
  const blank = proposalTitle(shownTitle) === null;
  return (
    <>
      <div className="selected-identity">
        <p className="selected-kicker proposed">
          <span className="proposal-glyph" aria-hidden="true" />
          PROPOSED STEP
        </p>
        <input
          ref={titleField}
          className="selected-name ui selected-title-input"
          aria-label="Proposed step title"
          aria-invalid={blank}
          value={shownTitle}
          maxLength={PROPOSAL_TITLE_MAX}
          placeholder="What it would do…"
          disabled={author === null}
          onChange={(event) => {
            const text = event.target.value;
            setTitle(text);
            if (author !== null && proposalTitle(text) !== null) {
              reword(target, proposal.id, { title: text }, author, "title");
            }
          }}
          onBlur={() => {
            setTitle(null);
          }}
        />
        {blank ? <p className="selected-byline proposal-error">A proposed step needs a title.</p> : null}
        <p className="selected-byline">
          <Author author={proposal.author} />
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
          maxLength={PROPOSAL_REASON_MAX}
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
      </div>
      {card ? <Where target={target} proposal={proposal} card={card} /> : null}
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
