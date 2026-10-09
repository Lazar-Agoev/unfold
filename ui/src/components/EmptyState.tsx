/** A quiet, centered message for a view with nothing to show yet ("No boards yet"), with an optional action. */

import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./internal/dom";
import "./EmptyState.css";

export interface EmptyStateProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  readonly title: string;
  readonly body?: ReactNode;
  /** A button, usually one. */
  readonly action?: ReactNode;
}

export function EmptyState({ title, body, action, className, ...rest }: EmptyStateProps) {
  return (
    <div {...rest} className={cx("empty-state", className)}>
      <p className="empty-state-title">{title}</p>
      {body === undefined ? null : <p className="empty-state-body">{body}</p>}
      {action === undefined ? null : <div className="empty-state-action">{action}</div>}
    </div>
  );
}
