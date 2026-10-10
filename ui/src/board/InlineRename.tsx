/**
 * A title renamed in place (V-74): the board's in the sidebar header and a page's in its row. A
 * double-click (or `editing` from outside) turns it into a field; Enter commits, Escape or an empty
 * title keeps the old one, and leaving the field commits.
 */

import { useRef, type ReactNode } from "react";

export interface InlineRenameProps {
  readonly value: string;
  readonly editing: boolean;
  /** The field's accessible name ("Board title"). */
  readonly label: string;
  readonly className: string;
  /** Called once with the new title (trimmed), or null when the edit is abandoned. */
  readonly onDone: (title: string | null) => void;
  readonly children: ReactNode;
}

export function InlineRename({ value, editing, label, className, onDone, children }: InlineRenameProps) {
  return editing ? <RenameField value={value} label={label} className={className} onDone={onDone} /> : children;
}

function RenameField({ value, label, className, onDone }: Omit<InlineRenameProps, "editing" | "children">) {
  // Enter or Escape ends the edit once; the blur that follows the field's removal must not end it again.
  const done = useRef(false);
  const finish = (text: string | null) => {
    if (done.current) {
      return;
    }
    done.current = true;
    const title = text?.trim() ?? "";
    onDone(title.length > 0 && title !== value ? title : null);
  };
  return (
    <input
      autoFocus
      className={`inline-rename ${className}`}
      aria-label={label}
      defaultValue={value}
      onFocus={(event) => {
        event.currentTarget.select();
      }}
      onClick={(event) => {
        event.stopPropagation();
      }}
      onDoubleClick={(event) => {
        event.stopPropagation();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") {
          event.preventDefault();
          finish(event.currentTarget.value);
        } else if (event.key === "Escape") {
          event.preventDefault();
          finish(null);
        }
      }}
      onBlur={(event) => {
        finish(event.currentTarget.value);
      }}
    />
  );
}
