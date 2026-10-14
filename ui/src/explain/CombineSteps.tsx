/**
 * Combine… (core-scope.html's A BLOCK'S MENU): a small popover beside the block that lists the drawn
 * steps of the body it can be combined in, the block's own step ticked. Clicking another step ticks the
 * run from the block to it, so what is chosen is always consecutive; steps the card doesn't draw between
 * them (hidden builtins) go with them. A whole control block combined by itself lists only itself. The
 * title is required. Combining saves at once as one undo step, collapsed and selected; what the board
 * refuses shows here and changes nothing.
 */

import { useMemo, useRef, useState, type SyntheticEvent } from "react";
import { Button, Popover } from "../components";
import type { DrawnFlow, Key } from "../ipc/generated";
import { useAuthor } from "../state/author";
import { combineSteps, select, type PageTarget } from "../state/boards";
import { findBlock } from "../state/cards";
import "./explain.css";

export interface CombineStepsProps {
  readonly target: PageTarget;
  readonly anchor: HTMLElement;
  /** The card's drawn flow, which names the steps. */
  readonly flow: DrawnFlow;
  /** The block Combine… was chosen on. */
  readonly block: Key;
  /** The steps of its body, in written order, drawn or not; null when it combines only by itself. */
  readonly steps: readonly Key[] | null;
  readonly onClose: () => void;
}

/** One drawn step of the body: where it is in the body's steps, and its words. */
interface Row {
  readonly key: Key;
  readonly at: number;
  readonly text: string;
  readonly named: boolean;
}

export function CombineSteps({ target, anchor, flow, block, steps, onClose }: CombineStepsProps) {
  const author = useAuthor();
  const titleField = useRef<HTMLInputElement>(null);
  const rows = useMemo(
    () =>
      (steps ?? [block]).flatMap((key, at): Row[] => {
        const drawn = findBlock(flow, key);
        return drawn ? [{ key, at, text: drawn.name ?? drawn.label, named: drawn.name !== undefined }] : [];
      }),
    [steps, block, flow],
  );
  const own = Math.max(
    0,
    rows.findIndex((row) => row.key === block),
  );
  const [end, setEnd] = useState(own);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const low = Math.min(own, end);
  const high = Math.max(own, end);
  const from = rows[low]?.at ?? 0;
  const to = rows[high]?.at ?? 0;
  const members = steps ? steps.slice(from, to + 1) : [block];
  const ready = title.trim() !== "" && author !== null && !busy;

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (!ready) {
      return;
    }
    setBusy(true);
    setError(null);
    const outcome = await combineSteps(target, { members, title: title.trim(), author });
    if (outcome.ok) {
      select(target, { kind: "combined", combinedId: outcome.value });
      onClose();
    } else {
      setError(outcome.message);
      setBusy(false);
    }
  };

  return (
    <Popover open anchor={anchor} label="Combine steps" side="right" align="start" width={320} initialFocus={titleField} onClose={onClose}>
      <form className="explain-form" onSubmit={(event) => void submit(event)}>
        <p className="explain-title">{`Combine ${members.length === 1 ? "1 step" : `${members.length} steps`}`}</p>
        <div className="explain-fields">
          <div className="explain-field">
            <span className="explain-label">Steps</span>
            <div className="combine-steps" role="listbox" aria-label="Steps" aria-multiselectable="true">
              {rows.map((row, index) => {
                const chosen = index >= low && index <= high;
                return (
                  <button
                    key={row.key}
                    type="button"
                    role="option"
                    aria-selected={chosen}
                    className="combine-step"
                    onClick={() => {
                      setEnd(index);
                      setError(null);
                    }}
                  >
                    <span className="combine-tick" aria-hidden="true">
                      {chosen ? "✓" : ""}
                    </span>
                    <span className={`combine-name${row.named ? " named" : ""}`}>{row.text}</span>
                  </button>
                );
              })}
            </div>
          </div>
          <label className="explain-field">
            <span className="explain-label">Title</span>
            <input
              ref={titleField}
              className="explain-input"
              value={title}
              placeholder="What these steps do"
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => {
                setTitle(event.target.value);
                setError(null);
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
            Combine
          </Button>
        </div>
      </form>
    </Popover>
  );
}
