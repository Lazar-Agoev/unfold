/**
 * A small set of mutually exclusive options shown side by side, e.g. the Code | Flow lens. It is a
 * radio group: Tab reaches the chosen option, and the arrow keys move and choose.
 */

import type { LucideIcon } from "lucide-react";
import { useRef, type KeyboardEvent } from "react";
import { cx } from "./internal/dom";
import "./SegmentedControl.css";

export interface SegmentedOption<V extends string> {
  readonly value: V;
  readonly label: string;
  readonly icon?: LucideIcon;
  /** Shows only the icon (the label stays the accessible name). */
  readonly iconOnly?: boolean;
  readonly disabled?: boolean;
}

export interface SegmentedControlProps<V extends string> {
  readonly options: readonly SegmentedOption<V>[];
  readonly value: V;
  readonly onChange: (value: V) => void;
  /** The group's accessible name, e.g. "Lens". */
  readonly label: string;
  readonly size?: "small" | "medium";
  readonly className?: string;
}

export function SegmentedControl<V extends string>({
  options,
  value,
  onChange,
  label,
  size = "medium",
  className,
}: SegmentedControlProps<V>) {
  const group = useRef<HTMLDivElement>(null);
  const enabled = options.filter((option) => !option.disabled);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (step === undefined || enabled.length === 0) {
      return;
    }
    event.preventDefault();
    const index = enabled.findIndex((option) => option.value === value);
    const next = enabled[(index + step + enabled.length) % enabled.length];
    if (next) {
      onChange(next.value);
      group.current?.querySelector<HTMLElement>(`[data-value="${CSS.escape(next.value)}"]`)?.focus();
    }
  };

  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={label}
      className={cx("segmented", `segmented-${size}`, className)}
      onKeyDown={onKeyDown}
    >
      {options.map((option) => {
        const checked = option.value === value;
        const Icon = option.icon;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            aria-label={option.iconOnly ? option.label : undefined}
            title={option.iconOnly ? option.label : undefined}
            data-value={option.value}
            tabIndex={checked ? 0 : -1}
            disabled={option.disabled}
            className="segmented-option"
            onClick={() => {
              onChange(option.value);
            }}
          >
            {Icon ? <Icon size={14} aria-hidden="true" /> : null}
            {option.iconOnly ? null : <span>{option.label}</span>}
          </button>
        );
      })}
    </div>
  );
}
