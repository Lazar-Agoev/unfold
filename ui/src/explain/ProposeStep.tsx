/**
 * Add proposed step… (core-scope.html's NAMED, COMBINED, PROPOSED): a small popover beside the block
 * that asks for the intended step's title (required) and why (optional). It goes after the block, or at
 * the start of its body, as the block's actions allow; it is drawn dashed as PROPOSED, and Selected edits
 * or removes it. Proposing saves at once as one undo step and selects it; what the board refuses shows
 * here and changes nothing.
 */

import { useRef, useState, type SyntheticEvent } from "react";
import { Button, Popover } from "../components";
import { useAuthor } from "../state/author";
import { PROPOSAL_REASON_MAX, PROPOSAL_TITLE_MAX, proposalReason, proposalTitle, proposeStep, select, type PageTarget, type ProposalPlace } from "../state/boards";
import "./explain.css";

export interface ProposeStepProps {
  readonly target: PageTarget;
  readonly anchor: HTMLElement;
  readonly place: ProposalPlace;
  /** The block's words, for where it goes. */
  readonly label: string;
  readonly onClose: () => void;
}

export function ProposeStep({ target, anchor, place, label, onClose }: ProposeStepProps) {
  const author = useAuthor();
  const titleField = useRef<HTMLInputElement>(null);
  const [title, setTitle] = useState("");
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const written = proposalTitle(title);
  const ready = written !== null && author !== null && !busy;

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (written === null || author === null || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    const why = proposalReason(reason);
    const outcome = await proposeStep(target, { ...place, title: written, ...(why !== null ? { reason: why } : {}), author });
    if (outcome.ok) {
      select(target, { kind: "proposal", proposalId: outcome.value });
      onClose();
    } else {
      setError(outcome.message);
      setBusy(false);
    }
  };

  return (
    <Popover open anchor={anchor} label="Add proposed step" side="right" align="start" width={320} initialFocus={titleField} onClose={onClose}>
      <form className="explain-form" onSubmit={(event) => void submit(event)}>
        <p className="explain-title">Add proposed step</p>
        <div className="explain-fields">
          <div className="explain-field">
            <span className="explain-label">{"after" in place ? "After" : "At the start"}</span>
            <span className="explain-where" title={label}>
              {label}
            </span>
          </div>
          <label className="explain-field">
            <span className="explain-label">Title</span>
            <input
              ref={titleField}
              className="explain-input"
              value={title}
              maxLength={PROPOSAL_TITLE_MAX}
              placeholder="What it would do"
              autoComplete="off"
              onChange={(event) => {
                setTitle(event.target.value);
                setError(null);
              }}
            />
          </label>
          <label className="explain-field">
            <span className="explain-label">Why</span>
            <textarea
              className="explain-input"
              value={reason}
              maxLength={PROPOSAL_REASON_MAX}
              placeholder="Why it's wanted (optional)"
              rows={2}
              onChange={(event) => {
                setReason(event.target.value);
              }}
            />
          </label>
        </div>
        {error !== null ? <p className="explain-error">{error}</p> : null}
        <div className="explain-footer">
          <Button variant="secondary" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!ready}>
            Propose
          </Button>
        </div>
      </form>
    </Popover>
  );
}
