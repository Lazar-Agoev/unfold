/**
 * Authored text in Selected: a description or a Flow label in the authored color on its tint, edited
 * in place (the field grows with its lines), with who wrote it underneath ("Claude · agent" for an
 * agent). Every keystroke is a board edit, and makes the text yours; typing in one field merges into
 * one undo step.
 */

import { useRef, type ReactNode } from "react";
import { Author } from "../../agent";
import type { Description } from "../../ipc/generated";
import { useFocusTarget } from "../../keyboard";

export interface AuthoredFieldProps {
  /** The field's accessible name ("Description", "Flow label"). */
  readonly label: string;
  readonly value: Description | undefined;
  readonly placeholder: string;
  /** Named focus target (`requestFocus`), e.g. `selected.description`. */
  readonly focusKey: string;
  /** Why the field can't be edited now (no author yet, a held reference). */
  readonly disabledReason?: string | null;
  /** Shown under the field: what needs review about it, the code name a label replaces. */
  readonly status?: ReactNode;
  readonly onChange: (text: string) => void;
  /** Called when the field loses focus (a note drops the draft it shows while typing). */
  readonly onBlur?: () => void;
}

export function AuthoredField({ label, value, placeholder, focusKey, disabledReason = null, status, onChange, onBlur }: AuthoredFieldProps) {
  const field = useRef<HTMLTextAreaElement>(null);
  useFocusTarget(focusKey, field);
  const text = value?.text ?? "";
  return (
    <div className="authored-field">
      <textarea
        ref={field}
        className="authored-input"
        aria-label={label}
        value={text}
        placeholder={placeholder}
        disabled={disabledReason !== null}
        rows={1}
        spellCheck
        onChange={(event) => {
          onChange(event.target.value);
        }}
        onBlur={onBlur}
      />
      {value ? (
        <span className="authored-byline">
          <Author author={value.author} />
        </span>
      ) : null}
      {disabledReason !== null ? <span className="authored-note">{disabledReason}</span> : null}
      {status}
    </div>
  );
}
