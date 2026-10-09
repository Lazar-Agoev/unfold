/**
 * An authored text field in the inspector: one quiet line with a **more** link that opens it to
 * several lines (architecture §10.7), who wrote it, and a flag when the code changed since. Every
 * keystroke is a board edit; typing in one field merges into one undo step.
 */

import { useId, useRef, useState, type ReactNode } from "react";
import type { Description } from "../ipc/generated";
import { useFocusTarget } from "../keyboard";
import { authorName } from "./useAuthor";

export interface AuthoredFieldProps {
  readonly label: string;
  readonly value: Description | undefined;
  readonly placeholder: string;
  /** Named focus target (`requestFocus`), e.g. `inspector.description`. */
  readonly focusKey?: string;
  /** Why the field can't be edited now (no author yet, a held reference). */
  readonly disabledReason?: string | null;
  /** Shown under the field: outdated, held for review. */
  readonly status?: ReactNode;
  readonly onChange: (text: string) => void;
}

export function AuthoredField({ label, value, placeholder, focusKey, disabledReason = null, status, onChange }: AuthoredFieldProps) {
  const id = useId();
  const text = value?.text ?? "";
  const [opened, setOpened] = useState(false);
  const multi = opened || text.includes("\n");
  const field = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);
  // The wrapper is the focus target: it stays while the field switches between one line and several.
  const wrapper = useRef<HTMLDivElement>(null);
  useFocusTarget(focusKey, wrapper);
  const disabled = disabledReason !== null;

  const toggle = () => {
    setOpened((current) => !current);
    // The other element mounts next; focus follows it.
    requestAnimationFrame(() => {
      field.current?.focus();
    });
  };

  return (
    <div
      ref={wrapper}
      className="authored-field"
      tabIndex={-1}
      onFocus={(event) => {
        if (event.target === event.currentTarget) {
          field.current?.focus();
        }
      }}
    >
      <div className="authored-field-head">
        <label htmlFor={id} className="inspector-label">
          {label}
        </label>
        {!text.includes("\n") ? (
          <button type="button" className="inspector-link" aria-expanded={multi} aria-controls={id} onClick={toggle}>
            {multi ? "less" : "more"}
          </button>
        ) : null}
      </div>
      {multi ? (
        <textarea
          id={id}
          ref={(element) => {
            field.current = element;
          }}
          className="authored-input multi"
          rows={4}
          value={text}
          placeholder={placeholder}
          disabled={disabled}
          spellCheck
          onChange={(event) => {
            onChange(event.target.value);
          }}
        />
      ) : (
        <input
          id={id}
          ref={(element) => {
            field.current = element;
          }}
          className="authored-input"
          value={text}
          placeholder={placeholder}
          disabled={disabled}
          spellCheck
          autoComplete="off"
          onChange={(event) => {
            onChange(event.target.value);
          }}
        />
      )}
      <div className="authored-meta">
        {value ? <span>by {authorName(value.author)}</span> : null}
        {disabledReason !== null ? <span>{disabledReason}</span> : null}
        {status}
      </div>
    </div>
  );
}
