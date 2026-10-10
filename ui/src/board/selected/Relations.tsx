/** Selected's label/value rows: an 88px label column (72px for links), then the value. */

import type { ReactNode } from "react";

export interface RelationRowProps {
  /** The label, shown on the first row of a relation only. */
  readonly label?: string;
  readonly children: ReactNode;
  /** A trailing action ("+ Open"). */
  readonly action?: ReactNode;
  /** Makes the value clickable (focus or open what it names). */
  readonly onActivate?: () => void;
  readonly title?: string;
}

export function RelationRow({ label, children, action, onActivate, title }: RelationRowProps) {
  return (
    <div className="relation-row">
      <span className="relation-label">{label}</span>
      {onActivate ? (
        <button type="button" className="relation-value" title={title} onClick={onActivate}>
          {children}
        </button>
      ) : (
        <span className="relation-value" title={title}>
          {children}
        </span>
      )}
      {action}
    </div>
  );
}
