/** A quiet, centered message for a view with nothing to show yet, with optional actions. */

import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cx } from "./internal/dom";
import "./EmptyState.css";

export interface EmptyStateProps {
  readonly title: string;
  readonly hint?: ReactNode;
  readonly icon?: LucideIcon;
  /** Buttons, usually one. */
  readonly actions?: ReactNode;
  /** `compact` for sidebars and panels; `page` for the editor area. */
  readonly size?: "compact" | "page";
  readonly className?: string;
}

export function EmptyState({ title, hint, icon: Icon, actions, size = "page", className }: EmptyStateProps) {
  return (
    <div className={cx("empty-state", `empty-state-${size}`, className)}>
      {Icon ? <Icon className="empty-state-icon" size={size === "page" ? 28 : 20} aria-hidden="true" /> : null}
      <p className="empty-state-title">{title}</p>
      {hint === undefined ? null : <p className="empty-state-hint">{hint}</p>}
      {actions === undefined ? null : <div className="empty-state-actions">{actions}</div>}
    </div>
  );
}
