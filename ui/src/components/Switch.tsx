/** A labeled on/off switch: the label, then the track whose knob slides right when it is on. */

import { cx } from "./internal/dom";
import "./Switch.css";

export interface SwitchProps {
  /** The visible label, also its accessible name. */
  readonly label: string;
  readonly on: boolean;
  readonly onChange: (on: boolean) => void;
  readonly className?: string;
}

export function Switch({ label, on, onChange, className }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className={cx("switch", className)}
      onClick={() => {
        onChange(!on);
      }}
    >
      {label}
      <span className={cx("switch-track", on && "on")} aria-hidden="true" />
    </button>
  );
}
