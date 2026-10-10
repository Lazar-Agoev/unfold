/** A short list of exclusive choices (Publish's Keep mine / Take theirs and drop my changes). */

import { useId } from "react";
import { cx } from "./internal/dom";
import "./RadioGroup.css";

export interface RadioGroupProps<V extends string> {
  readonly options: readonly { readonly value: V; readonly label: string }[];
  readonly value: V;
  readonly onChange: (value: V) => void;
  /** The group's accessible name. */
  readonly label: string;
  readonly className?: string;
}

export function RadioGroup<V extends string>({ options, value, onChange, label, className }: RadioGroupProps<V>) {
  const name = useId();
  return (
    <div role="radiogroup" aria-label={label} className={cx("radio-group", className)}>
      {options.map((option) => (
        <label key={option.value} className="radio">
          <input
            type="radio"
            name={name}
            value={option.value}
            className="radio-input"
            checked={option.value === value}
            onChange={() => {
              onChange(option.value);
            }}
          />
          <span className="radio-mark" aria-hidden="true" />
          <span className="radio-label">{option.label}</span>
        </label>
      ))}
    </div>
  );
}
