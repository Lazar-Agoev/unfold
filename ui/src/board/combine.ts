/**
 * Combine: a run of steps Shift-selected in one function
 * card's own Flow, and whether it can become a combined block (an existing step group). The run's
 * statements come from the same outline rule MCP `group_steps` uses: `groupSelection`
 * over the lines from its first step to its last, then `groupConflict` against the card's groups, so
 * gaps, overlaps, nested groups, the function's header, nested definitions, and statements of two
 * bodies are refused with the same reason. Steps shown inside a call expanded in place belong to
 * another function and are never part of a run.
 */

import { groupConflict, groupSelection, isWithin } from "../code/outline";
import type { CardView, FlowModel, FlowNode, Group, Key, Page } from "../ipc/generated";
import { cardKey, findCard, type Selection } from "../state/boards";
import type { SourceContext } from "./viewCode";

type BlockView = Extract<CardView, { type: "block" }>;

/** A run of steps: the lines it covers, the Flow steps in it, and its statements or why it can't be combined. */
export interface StepRun {
  /** Its first line: its first statement's when it can be combined, else its first step's. */
  readonly startLine: number;
  /** Its last line, likewise. */
  readonly endLine: number;
  /** The Flow steps it holds, in the model's order. */
  readonly steps: readonly FlowNode[];
  /** The statements a combined block of it holds; absent when it can't be one. */
  readonly members?: readonly Key[];
  /** Why it can't be combined. */
  readonly error?: string;
}

/** Whether a Flow node is a step (not the Start or End pill). */
function isStep(node: FlowNode): boolean {
  return node.kind !== "start" && node.kind !== "end" && node.key !== undefined;
}

/** The steps a combined block of `members` holds: each step that is one of them or inside one. */
export function combinedSteps(flow: FlowModel, members: readonly Key[]): FlowNode[] {
  return flow.nodes.filter((node) => isStep(node) && members.some((member) => node.key !== undefined && isWithin(node.key, member)));
}

/**
 * The run from step `first` to step `last` (either order) of a function's own Flow, checked against
 * its other groups (`groups`, the card's). Null when either isn't a step of its Flow.
 */
export function stepRun(model: BlockView, first: Key, last: Key, groups: readonly Pick<Group, "title" | "members">[]): StepRun | null {
  const flow = model.flow;
  const a = flow?.nodes.find((node) => node.key === first);
  const b = flow?.nodes.find((node) => node.key === last);
  if (!flow || !a || !b || !isStep(a) || !isStep(b) || a.startLine === undefined || b.startLine === undefined) {
    return null;
  }
  const from = Math.min(a.startLine, b.startLine);
  const to = Math.max(a.endLine ?? a.startLine, b.endLine ?? b.startLine);
  const between = (error: string): StepRun => ({
    startLine: from,
    endLine: to,
    steps: flow.nodes.filter((node) => isStep(node) && node.startLine !== undefined && node.startLine >= from && (node.endLine ?? node.startLine) <= to),
    error,
  });
  const code = model.code?.source.state === "current" ? model.code.source : null;
  if (!code) {
    return between("This function's code isn't available right now. Compile the project, then try again.");
  }
  const selection = groupSelection(code.outline, from, to);
  if ("error" in selection) {
    return between(selection.error);
  }
  const conflict = groupConflict(code.outline, selection.members, groups);
  if (conflict !== null) {
    return between(conflict);
  }
  const items = code.outline.filter((item) => selection.members.includes(item.key));
  const steps = combinedSteps(flow, selection.members);
  const lines = { startLine: Math.min(...items.map((item) => item.startLine)), endLine: Math.max(...items.map((item) => item.endLine)), steps };
  return steps.length < 2 ? { ...lines, error: "Choose two or more steps to combine." } : { ...lines, members: selection.members };
}

/** What Combine… does with a selection: the card and statements of a run it can combine, or why it can't. */
export type CombineChoice = { readonly cardId: string; readonly members: readonly Key[]; readonly run: StepRun } | { readonly reason: string };

/** Combine… for `selection` on `page`, from the card models loaded now. */
export function combineChoice(page: Page, selection: Selection, context: SourceContext): CombineChoice {
  if (selection.kind !== "card" || selection.key === null) {
    return { reason: "Select steps in a function's flow to combine them." };
  }
  if (selection.owner !== undefined) {
    return { reason: "These steps belong to the called function. Open it as its own card to combine its steps." };
  }
  if (selection.last === undefined) {
    return { reason: "Shift-click the last step of the run to combine it." };
  }
  const card = findCard(page, selection.cardId);
  const model = card ? context.model(cardKey(card)) : undefined;
  if (!card || model?.type !== "block") {
    return { reason: "This card's steps aren't loaded." };
  }
  const run = stepRun(
    model,
    selection.key,
    selection.last,
    page.groups.filter((group) => group.card === card.id),
  );
  if (!run) {
    return { reason: "Select steps in a function's flow to combine them." };
  }
  if (run.members === undefined || run.error !== undefined) {
    return { reason: run.error ?? "These steps can't be combined." };
  }
  return { cardId: card.id, members: run.members, run };
}
