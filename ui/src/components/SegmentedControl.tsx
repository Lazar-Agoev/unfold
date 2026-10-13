/**
 * Mutually exclusive options side by side: the window's Diagram | Code lens. It is a radio group:
 * Tab reaches the chosen option, and the arrow keys move and choose.
 */

import { useRef, type KeyboardEvent } from "react";
import { cx } from "./internal/dom";
import "./SegmentedControl.css";

interface SegmentedOption<V extends string> {
  readonly value: V;
  readonly label: string;
}

export interface SegmentedControlProps<V extends string> {
  readonly options: readonly SegmentedOption<V>[];
  readonly value: V;
  readonly onChange: (value: V) => void;
  /** The group's accessible name, e.g. "Lens". */
  readonly label: string;
  readonly className?: string;
}

export function SegmentedControl<V extends string>({ options, value, onChange, label, className }: SegmentedControlProps<V>) {
  const group = useRef<HTMLDivElement>(null);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (step === undefined || options.length === 0) {
      return;
    }
    event.preventDefault();
    const index = options.findIndex((option) => option.value === value);
    const next = options[(index + step + options.length) % options.length];
    if (next) {
      onChange(next.value);
      group.current?.querySelector<HTMLElement>(`[data-value="${CSS.escape(next.value)}"]`)?.focus();
    }
  };

  return (
    <div ref={group} role="radiogroup" aria-label={label} className={cx("segmented", "segmented-lens", className)} onKeyDown={onKeyDown}>
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            data-value={option.value}
            tabIndex={checked ? 0 : -1}
            className="segmented-option"
            onClick={() => {
              onChange(option.value);
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
