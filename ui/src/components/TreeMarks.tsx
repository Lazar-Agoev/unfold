/**
 * Quiet marks for the right edge of tree and list rows (architecture §11.3): `→4` entrances, `●` on
 * the open board, an amber dot for a changed or pending file, a warning for a file that doesn't
 * parse, and muted text such as "not indexed" or "7 launches". Each carries a spoken label.
 */

import { TriangleAlert } from "lucide-react";
import "./TreeMarks.css";

/** An arrow and a count, e.g. `→4` for four entrances. */
export function CountMark({ count, label }: { readonly count: number; readonly label: string }) {
  return (
    <span className="mark mark-count" aria-label={label} title={label}>
      <span aria-hidden="true">→{count}</span>
    </span>
  );
}

/** A dot: `accent` (●, on the open board) or `pending` (amber, changed or waiting to compile). */
export function DotMark({ tone, label }: { readonly tone: "accent" | "pending"; readonly label: string }) {
  return <span className={`mark mark-dot mark-dot-${tone}`} role="img" aria-label={label} title={label} />;
}

/** A warning sign, e.g. for a file that doesn't parse. */
export function WarningMark({ label }: { readonly label: string }) {
  return (
    <span className="mark mark-warning" role="img" aria-label={label} title={label}>
      <TriangleAlert size={13} aria-hidden="true" />
    </span>
  );
}

/** Muted text, e.g. "not indexed", "ignored", "7 launches". */
export function TextMark({ children }: { readonly children: string }) {
  return <span className="mark mark-text">{children}</span>;
}
