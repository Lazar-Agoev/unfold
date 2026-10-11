/** One labeled field of the launch editor: the label (with a note on its right), the control, an error, and a hint. */

import type { ReactNode } from "react";

interface FieldProps {
  readonly label: string;
  /** The control's id, so the label names it; omit for a group the control labels itself. */
  readonly htmlFor?: string;
  /** For controls that aren't inputs (the folder picker): the label's id, for their `aria-labelledby`. */
  readonly labelId?: string;
  /** On the label's right ("as you deploy it", "optional"). */
  readonly aside?: string;
  readonly hint?: ReactNode;
  readonly error?: string | undefined;
  readonly className?: string;
  readonly children: ReactNode;
}

export function Field({ label, htmlFor, labelId, aside, hint, error, className, children }: FieldProps) {
  return (
    <div className={className === undefined ? "launch-field" : `launch-field ${className}`}>
      <div className="launch-field-head">
        <label id={labelId} htmlFor={htmlFor} className="launch-label">
          {label}
        </label>
        {aside === undefined ? null : <span className="launch-aside">{aside}</span>}
      </div>
      {children}
      {error === undefined ? null : <p className="launch-error">{error}</p>}
      {hint === undefined ? null : <p className="launch-hint">{hint}</p>}
    </div>
  );
}
