/** An on/off switch: a 28×16 track with a 12px knob, accent when on. */

import type { ButtonHTMLAttributes } from "react";
import { cx } from "./internal/dom";
import "./Switch.css";

export interface SwitchProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "children"> {
  readonly checked: boolean;
  readonly onChange: (checked: boolean) => void;
  /** The accessible name; the row beside it shows the visible text. */
  readonly label: string;
  readonly disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled = false, className, ...rest }: SwitchProps) {
  return (
    <button
      {...rest}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={cx("switch", className)}
      onClick={() => {
        onChange(!checked);
      }}
    >
      <span className="switch-knob" />
    </button>
  );
}
