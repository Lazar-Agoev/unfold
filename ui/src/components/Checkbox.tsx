/**
 * A 16px check box with four states: checked, unchecked, mixed (some folders
 * inside are excluded), and disabled (ignored by default or by .gitignore). A native input under the
 * drawing keeps keyboard and screen-reader behavior.
 */

import { useEffect, useRef, type InputHTMLAttributes } from "react";
import { cx } from "./internal/dom";
import "./Checkbox.css";

export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "checked" | "onChange" | "type" | "size"> {
  readonly checked: boolean | "mixed";
  /** Called with the new state; a mixed box becomes checked. */
  readonly onChange: (checked: boolean) => void;
  /** The accessible name; the row beside it shows the visible text. */
  readonly label: string;
  readonly disabled?: boolean;
}

export function Checkbox({ checked, onChange, label, disabled = false, className, ...rest }: CheckboxProps) {
  const input = useRef<HTMLInputElement>(null);
  const mixed = checked === "mixed";
  useEffect(() => {
    if (input.current) {
      input.current.indeterminate = mixed;
    }
  }, [mixed]);
  return (
    <span className={cx("checkbox", className)} data-state={mixed ? "mixed" : checked ? "checked" : "unchecked"}>
      <input
        {...rest}
        ref={input}
        type="checkbox"
        className="checkbox-input"
        aria-label={label}
        checked={checked === true}
        disabled={disabled}
        onChange={() => {
          onChange(checked !== true);
        }}
      />
      <span className="checkbox-box" aria-hidden="true">
        {checked === true ? (
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" className="checkbox-check">
            <path d="M2 5.2L4.2 7.2L8 3" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : mixed ? (
          <span className="checkbox-bar" />
        ) : null}
      </span>
    </span>
  );
}
