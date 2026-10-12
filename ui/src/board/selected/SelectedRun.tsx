/**
 * Selected for a run of steps Shift-selected in a function's own Flow: "N steps", "in <function> ·
 * lines a–b" (the lines open the run's source, as View code does), its steps in order, and whether it
 * can be combined ("consecutive · one body") or why not, by the rule Combine… and MCP `group_steps`
 * share.
 */

import { useMemo } from "react";
import type { Card, CardView } from "../../ipc/generated";
import { plural } from "../../lib/format";
import { findPage, labelsFor, select, useOpenBoard, type PageTarget } from "../../state/boards";
import { stepRun } from "../combine";
import { viewCode } from "../viewCode";
import { RelationRow } from "./Relations";

export interface SelectedRunProps {
  readonly target: PageTarget;
  readonly card: Card;
  /** The card's model, with its Flow and Code. */
  readonly model: Extract<CardView, { type: "block" }>;
  /** The run's first step (where it was started) and its last (Shift-clicked). */
  readonly first: string;
  readonly last: string;
}

/** What Selected shows for a run its steps don't make (it isn't a run of this card's steps). */
const NO_RUN = <p className="selected-note">Shift-click steps of one function's flow to select them as a run.</p>;

export function SelectedRun({ target, card, model, first, last }: SelectedRunProps) {
  const allGroups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const allLabels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const run = useMemo(
    () =>
      stepRun(
        model,
        first,
        last,
        (allGroups ?? []).filter((group) => group.card === card.id),
      ),
    [model, first, last, allGroups, card.id],
  );
  if (!run) {
    return NO_RUN;
  }
  const labels = labelsFor(allLabels ?? [], card.ref.project);
  const lines = run.startLine === run.endLine ? `line ${run.startLine}` : `lines ${run.startLine}–${run.endLine}`;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{plural(run.steps.length, "step")}</h3>
        <p className="selected-location">
          in{" "}
          <button
            type="button"
            className="selected-link"
            onClick={() => {
              select(target, { kind: "card", cardId: card.id, key: null });
            }}
          >
            {model.header.qualifiedName}
          </button>
          {" · "}
          <button
            type="button"
            className="selected-link"
            onClick={() => {
              viewCode(target, { kind: "card", cardId: card.id, key: first, last });
            }}
          >
            {lines}
          </button>
        </p>
      </div>
      <div className="selected-relations">
        {run.steps.map((step, index) => (
          <RelationRow key={step.id} {...(index === 0 ? { label: "Steps" } : {})}>
            <span className="relation-code">{(step.key !== undefined ? labels[step.key] : undefined) ?? step.label}</span>
          </RelationRow>
        ))}
        <RelationRow label="Run">{run.error ?? "consecutive · one body"}</RelationRow>
      </div>
    </>
  );
}
