/**
 * A project's state as one glyph (07): in the sidebar row's 16px slot (`row`) or before the words on
 * Home's state line (`line`). A current project's row slot stays empty; quiet is the normal state.
 * The problems dot and the failed triangle of an unavailable project explain themselves in a tooltip.
 */

import type { ReactNode } from "react";
import { StateGlyph } from "../icons";
import { plural } from "../lib/format";
import { cx } from "./internal/dom";
import { ProgressRing } from "./ProgressRing";
import { Tooltip } from "./Tooltip";
import "./StateIndicator.css";

/** A project's state (contract §4.1 `ProjectState`), as the engine reports it. */
type ProjectState =
  | { readonly kind: "current" }
  | { readonly kind: "compiling"; readonly done: number; readonly total: number }
  | { readonly kind: "updating"; readonly files: number | null }
  | { readonly kind: "problems"; readonly count: number }
  | { readonly kind: "paused"; readonly changed: number }
  | { readonly kind: "not_compiled" }
  | { readonly kind: "failed"; readonly message: string }
  | { readonly kind: "resting"; readonly last: ProjectState; readonly atMs: number }
  | { readonly kind: "unavailable"; readonly reason: string };

export interface StateIndicatorProps {
  readonly state: ProjectState;
  readonly variant: "row" | "line";
  readonly className?: string;
}

/** What the glyph means, for screen readers and the tooltip. */
function stateWords(state: ProjectState): string {
  switch (state.kind) {
    case "current":
      return "Graph current";
    case "compiling":
      return "Compiling";
    case "updating":
      return "Updating";
    case "problems":
      return plural(state.count, "problem");
    case "paused":
      return "Background updates paused";
    case "not_compiled":
      return "Not compiled yet";
    case "failed":
      return "Compile failed";
    case "resting":
      return "Resting";
    case "unavailable":
      return state.reason;
  }
}

function glyphFor(state: ProjectState, variant: "row" | "line"): ReactNode {
  switch (state.kind) {
    case "current":
      return variant === "row" ? null : <span className="state-dot state-dot-current" />;
    case "compiling":
    case "updating":
      return <ProgressRing size={variant === "row" ? 14 : 12} />;
    case "problems":
      return <span className="state-dot state-dot-problems" />;
    case "paused":
      return <StateGlyph glyph="paused" variant={variant} />;
    case "not_compiled":
      return <StateGlyph glyph="not-compiled" variant={variant} />;
    case "failed":
    case "unavailable":
      return <StateGlyph glyph="failed" variant={variant} />;
    case "resting":
      return <StateGlyph glyph="resting" variant={variant} />;
  }
}

export function StateIndicator({ state, variant, className }: StateIndicatorProps) {
  const glyph = glyphFor(state, variant);
  const words = stateWords(state);
  const indicator = (
    <span
      className={cx("state-indicator", `state-indicator-${variant}`, className)}
      data-state={state.kind}
      {...(glyph === null ? { "aria-hidden": true } : { role: "img", "aria-label": words })}
    >
      {glyph}
    </span>
  );
  if (state.kind === "problems" || state.kind === "unavailable") {
    return (
      <Tooltip content={words} describe={false}>
        {indicator}
      </Tooltip>
    );
  }
  return indicator;
}
