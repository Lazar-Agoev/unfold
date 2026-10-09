/** A sidebar view's header: its title in small caps, and quiet icon actions on the right. */

import type { ReactNode } from "react";
import "./views.css";

export function ViewHeader({ title, id, children }: { readonly title: string; readonly id: string; readonly children?: ReactNode }) {
  return (
    <div className="view-header">
      <h2 className="view-title" id={id}>
        {title}
      </h2>
      {children === undefined ? null : <div className="view-actions">{children}</div>}
    </div>
  );
}
