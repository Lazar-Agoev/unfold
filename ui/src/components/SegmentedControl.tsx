/**
 * Mutually exclusive options side by side: the window's Code | Flow lens (`lens`, components §7.5),
 * a card's mini lens (`mini`, 02 §4.1), and Health | Activity (`switch`, 11). It is a radio group:
 * Tab reaches the chosen option, and the arrow keys move and choose.
 */

import { useRef, type KeyboardEvent } from "react";
import { cx } from "./internal/dom";
import "./SegmentedControl.css";

interface SegmentedOption<V extends string> {
  readonly value: V;
  readonly label: string;
  /** Shown after the label in the `switch` variant (Health's problem count). */
  readonly count?: number;
}

export interface SegmentedControlProps<V extends string> {
  readonly options: readonly SegmentedOption<V>[];
  readonly value: V;
  readonly onChange: (value: V) => void;
  /** The group's accessible name, e.g. "Lens". */
  readonly label: string;
  readonly variant: "lens" | "mini" | "switch";
  readonly className?: string;
}

export function SegmentedControl<V extends string>({ options, value, onChange, label, variant, className }: SegmentedControlProps<V>) {
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
    <div ref={group} role="radiogroup" aria-label={label} className={cx("segmented", `segmented-${variant}`, className)} onKeyDown={onKeyDown}>
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            aria-label={option.count === undefined ? undefined : `${option.label} ${option.count.toLocaleString("en-US")}`}
            data-value={option.value}
            tabIndex={checked ? 0 : -1}
            className="segmented-option"
            onClick={() => {
              onChange(option.value);
            }}
          >
            <span>{option.label}</span>
            {option.count === undefined ? null : <span className="segmented-count">{option.count.toLocaleString("en-US")}</span>}
          </button>
        );
      })}
    </div>
  );
}
