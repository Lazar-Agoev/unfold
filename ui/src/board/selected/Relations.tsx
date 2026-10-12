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
  /** Wraps a long value onto more lines instead of cutting it short. */
  readonly wrap?: boolean;
}

export function RelationRow({ label, children, action, onActivate, title, wrap = false }: RelationRowProps) {
  const value = `relation-value${wrap ? " wrap" : ""}`;
  return (
    <div className="relation-row">
      <span className="relation-label">{label}</span>
      {onActivate ? (
        <button type="button" className={value} title={title} onClick={onActivate}>
          {children}
        </button>
      ) : (
        <span className={value} title={title}>
          {children}
        </span>
      )}
      {action}
    </div>
  );
}
